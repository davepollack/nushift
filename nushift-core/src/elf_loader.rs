use std::{collections::BTreeMap, ops::Bound};

use elfloader::{ElfLoader, ElfLoaderErr, LoadableHeaders, Flags, VAddr, RelocationEntry};

use super::shm_space::{ShmSpace, ShmType, ShmCapId, acquisitions_and_page_table::Sv39Flags};

// The loader in this file should be robust against:
//
// * Overlapping ELF LOAD headers (including fully overlapping, i.e. specifying the same base and size)
// * Integer overflowing ELF LOAD headers when you add the base and size
// * ELF LOAD headers with a size of 0
// * An ELF with so many LOAD headers or with such a large total size that it exhausts our Sv39 stats capacity
//
// And more. As far as I know, the current implementation as of writing this
// comment is robust against these four. For the overlapping case, we are
// actually overwriting earlier data with later data (as in when processing a
// later header), not returning an ELF loading error. I would not be opposed to
// in the future changing this to an ELF loading error, if you're not meant to
// be able to do this. For the other three cases, it does return an ELF loading
// error at the time of writing this comment.

/// Key is end VPN (exclusive), value is start VPN and optional cap.
///
/// Note that end VPN (exclusive) does fit in a u64 because a VPN is off by a
/// factor of 4096 for 64-bit virtual addresses.
struct CoveredPages(BTreeMap<u64, (u64, Option<ShmCapId>)>);

impl CoveredPages {
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns an error if (vpn + number_of_pages) would overflow.
    fn add_region(&mut self, vpn: u64, number_of_pages: u64) -> Result<(), ()> {
        let end_vpn = vpn.checked_add(number_of_pages).ok_or(())?;

        let new_range = vpn..end_vpn;

        // Collect the keys of the overlapping or consecutive regions.
        //
        // Note that `vpn..` (with an unbounded right side) is linear in the
        // number of regions.
        //
        // However, consider that we are only calling add_region about 4 times,
        // once per ELF program header. That is 16 steps. In contrast, if we
        // implemented add_page which can be implemented easily in O(log n) with
        // the current data structure, we'd be calling add_page about 256 times
        // for a 1 MiB executable. So the 16 steps wins.
        //
        // Furthermore, if add_region is called with regions that are generally
        // in ascending order, which I have noticed is the case for all ELFs
        // I've seen (and I've also noticed that only the end/start boundary
        // overlaps for all ELFs I've seen), then only about one region should
        // be hit by `vpn..` each time. In fact, this is why we are structuring
        // it with the end VPN as the key of the BST, to make this possible.
        let overlapping_entries: Vec<(u64, u64)> = self.0.range(vpn..)
            .filter(|(&end, &(start, _))| {
                start <= new_range.end && end >= new_range.start
            })
            .map(|(&end, &(start, _))| (end, start))
            .collect();

        // Determine the start and end of the merged region
        let mut merged_start = new_range.start;
        let mut merged_end = new_range.end;

        for (existing_end_vpn, existing_start_vpn) in overlapping_entries {
            self.0.remove(&existing_end_vpn);
            merged_start = merged_start.min(existing_start_vpn);
            merged_end = merged_end.max(existing_end_vpn);
        }

        // Insert the merged region into the BTreeMap
        self.0.insert(merged_end, (merged_start, None));

        Ok(())
    }

    #[cfg(test)]
    fn iter(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.0.iter().map(|(&end, &(start, _))| {
            let number_of_pages = end.checked_sub(start).expect("Must be valid due to add_region input validation and algorithm");
            (start, number_of_pages)
        })
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = (u64, u64, &mut Option<ShmCapId>)> + '_ {
        self.0.iter_mut().map(|(end, (start, shm_cap_id))| {
            let number_of_pages = end.checked_sub(*start).expect("Must be valid due to add_region input validation and algorithm");
            (*start, number_of_pages, shm_cap_id)
        })
    }

    /// Returns: SHM cap ID, start addr.
    fn lookup(&self, address: u64) -> Option<(ShmCapId, u64)> {
        let vpn = address >> 12;

        let mut above = self.0.range((Bound::Excluded(&vpn), Bound::Unbounded));
        let (_, &(start, shm_cap_id)) = above.next()?;
        let start_addr = start << 12;

        (start <= vpn).then_some((shm_cap_id?, start_addr))
    }
}

pub struct Loader<'space> {
    covered_pages: CoveredPages,
    shm_space: &'space mut ShmSpace,
}

impl<'space> Loader<'space> {
    pub fn new(shm_space: &'space mut ShmSpace) -> Self {
        Self { covered_pages: CoveredPages::new(), shm_space }
    }
}

fn flags_map(flags: Flags) -> Result<Sv39Flags, ()> {
    match () {
        _ if flags.is_read() && !flags.is_write() && !flags.is_execute() => Ok(Sv39Flags::R),
        _ if flags.is_read() && flags.is_write() && !flags.is_execute() => Ok(Sv39Flags::RW),
        _ if flags.is_read() && !flags.is_write() && flags.is_execute() => Ok(Sv39Flags::RX),
        _ if !flags.is_read() && !flags.is_write() && flags.is_execute() => Ok(Sv39Flags::X),
        _ => Err(()),
    }
}

impl ElfLoader for Loader<'_> {
    fn allocate(&mut self, load_headers: LoadableHeaders) -> Result<(), ElfLoaderErr> {
        for header in load_headers {
            let flags = header.flags();

            // Only allow certain combinations of flags.
            flags_map(flags).map_err(|_| {
                log::error!(
                    "Section at vaddr {:#x} has unsupported flags, the only supported combinations are r--, rw-, r-x, --x.",
                    header.virtual_addr(),
                );
                ElfLoaderErr::UnsupportedSectionData
            })?;

            let rounded_down_start_vpn = header.virtual_addr() >> 12;

            fn last_occupied_page_number(virtual_addr: u64, mem_size: u64) -> Result<u64, ()> {
                let mem_size_minus_one = mem_size.checked_sub(1).ok_or(())?;

                let last_byte_addr = virtual_addr
                    .checked_add(mem_size_minus_one)
                    .ok_or(())?;

                Ok(last_byte_addr >> 12)
            }
            let last_occupied_vpn = last_occupied_page_number(header.virtual_addr(), header.mem_size())
                .map_err(|_| {
                    log::error!(
                        "Section at vaddr {:#x} and mem_size {:#x} either overflows, or mem_size is 0, aborting loading program.",
                        header.virtual_addr(),
                        header.mem_size(),
                    );
                    ElfLoaderErr::UnsupportedSectionData
                })?;

            let number_of_pages = last_occupied_vpn
                .checked_sub(rounded_down_start_vpn)
                .expect("This should not underflow because of the last_occupied_page_number logic, but definitely panic if it does")
                .checked_add(1)
                .expect("This should not overflow because a full amount of pages fits in u64 because it's off by a factor of 4096, and because of the last_occupied_page_number logic. Panic if it does.");

            self.covered_pages.add_region(rounded_down_start_vpn, number_of_pages)
                .expect("An overflow at this point is also an unexpected error, so panic.");
        }

        let mut errored_caps = vec![];
        for (vpn, number_of_pages, stored_shm_cap_id) in self.covered_pages.iter_mut() {
            // If new_shm_cap fails, no need to clean up or destroy
            // anything. It makes sure that it only increments the Sv39
            // stats after no other errors have occurred. If the
            // implementation of new_shm_cap changes such that this is no
            // longer the case, and you didn't check this usage of
            // new_shm_cap, well, that is not good.
            let (shm_cap_id, _) = self.shm_space.new_shm_cap(ShmType::FourKiB, number_of_pages)
                .map_err(|err| {
                    log::error!("ELF loading: new_shm_cap either exhausted or is an internal error: {err:?}");
                    ElfLoaderErr::UnsupportedSectionData
                })?;
            *stored_shm_cap_id = Some(shm_cap_id);

            // TODO: Should have rwx bits in the page entry, the ELF flags
            // should be copied to it (gonna be either r--, r-x or --x), to
            // prevent app from either reading, writing or executing it if
            // it's not allowed. Furthermore, other operations on the caps
            // by the app should be disallowed.
            // TODO: Actually use the flags. This might require significant
            // refactoring of this file, and thinking about sub-page-size
            // sections.
            match self.shm_space.acquire_shm_cap_executable(shm_cap_id, vpn << 12, Sv39Flags::RX) {
                Ok(_) => {},
                Err(err) => {
                    // TODO: It's not necessarily an internal error. We haven't yet checked that it doesn't exceed 2^39.
                    log::error!("ELF loading: acquire_shm_cap internal error: {err:?}");
                    errored_caps.push(shm_cap_id);
                    break;
                }
            }
        }

        if errored_caps.len() > 0 {
            for shm_cap_id in errored_caps.into_iter().rev() {
                self.shm_space.release_shm_cap(shm_cap_id)
                    .map_err(|err| {
                        log::error!("Error while rolling back, release_shm_cap internal error: {err:?}");
                        ElfLoaderErr::UnsupportedSectionData
                    })?;
                self.shm_space.destroy_shm_cap(shm_cap_id)
                    .map_err(|err| {
                        log::error!("Error while rolling back, destroy_shm_cap internal error: {err:?}");
                        ElfLoaderErr::UnsupportedSectionData
                    })?;
            }
            Err(ElfLoaderErr::UnsupportedSectionData)
        } else {
            Ok(())
        }
    }

    fn load(&mut self, flags: Flags, base: VAddr, region: &[u8]) -> Result<(), ElfLoaderErr> {
        log::debug!(
            "Loading region with base {:#x} and length {}, flags [{}]",
            base,
            region.len(),
            flags,
        );

        // We load here by getting the cap directly and writing to
        // backing_mut(). As opposed to writing through the page
        // table/ProtectedMemory. If we did the latter, we would have to NOT set
        // permissions restrictions in allocate, and then set them here only
        // after loading. That can be done, I have just chosen the former.

        // Due to choosing the former, and the CoveredPages structure, we can't
        // check if there's a discrepancy between the `base` addresses given to
        // `load` and the LoadableHeaders given to `allocate`, We trust the
        // elfloader code to provide the same values (which from the code,
        // currently appears to).

        let (shm_cap_id, start_addr) = self.covered_pages.lookup(base).ok_or_else(|| {
            log::error!("Internal error refetching SHM cap ID when loading ELF data");
            ElfLoaderErr::UnsupportedSectionData
        })?;

        let offset: usize = base.checked_sub(start_addr)
            .expect("Internal error if this underflowed")
            .try_into()
            .map_err(|_| {
                log::error!("The size of combined ELF load sections is larger than what fits inside a usize machine word on this host machine. This limitation could be resolved in a future version of Nushift.");
                ElfLoaderErr::UnsupportedSectionData
            })?;

        self.shm_space.get_mut(shm_cap_id)
            .ok_or_else(|| {
                log::error!("Internal error refetching SHM cap when loading ELF data");
                ElfLoaderErr::UnsupportedSectionData
            })?
            .backing_mut()[offset..offset.checked_add(region.len()).expect("Internal error if this overflows")]
            .copy_from_slice(region);

        Ok(())
    }

    fn relocate(&mut self, _entry: RelocationEntry) -> Result<(), ElfLoaderErr> {
        // Unimplemented
        Err(ElfLoaderErr::UnsupportedRelocationEntry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_pages_overlapping_at_boundary() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(16, 3).expect("Should be valid");
        covered_pages.add_region(19, 20).expect("Should be valid");
        covered_pages.add_region(39, 5).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.iter().collect();
        assert_eq!(vec![(16, 28)], merged_regions);
    }

    #[test]
    fn covered_pages_overlapping_at_boundary_backwards() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(39, 5).expect("Should be valid");
        covered_pages.add_region(19, 20).expect("Should be valid");
        covered_pages.add_region(16, 3).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.iter().collect();
        assert_eq!(vec![(16, 28)], merged_regions);
    }

    #[test]
    fn covered_pages_overlapping_inner_and_outer() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(1, 10).expect("Should be valid");
        covered_pages.add_region(5, 1).expect("Should be valid");
        // Add them in the other order
        covered_pages.add_region(25, 1).expect("Should be valid");
        covered_pages.add_region(21, 10).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.iter().collect();
        assert_eq!(vec![(1, 10), (21, 10)], merged_regions);
    }

    #[test]
    fn covered_pages_overlapping_partial() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(1, 10).expect("Should be valid");
        covered_pages.add_region(8, 5).expect("Should be valid");
        covered_pages.add_region(12, 2).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.iter().collect();
        assert_eq!(vec![(1, 13)], merged_regions);
    }

    #[test]
    fn covered_pages_non_contiguous() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(1, 2).expect("Should be valid");
        covered_pages.add_region(4, 3).expect("Should be valid");
        covered_pages.add_region(10, 4).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.iter().collect();
        assert_eq!(vec![(1, 2), (4, 3), (10, 4)], merged_regions);
    }

    #[test]
    fn covered_pages_invalid_input() {
        let mut covered_pages = CoveredPages::new();

        // u64::MAX shouldn't be a valid VPN (because a VPN is off by a factor
        // of 4096), so it's valid to do an overflow check.
        assert!(matches!(covered_pages.add_region(u64::MAX, 1), Err(())));
    }
}
