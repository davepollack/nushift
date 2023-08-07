use std::collections::BTreeMap;

use elfloader::{ElfLoader, ElfLoaderErr, LoadableHeaders};

use super::shm_space::ShmSpace;

/// Key is end VPN (exclusive), value is start VPN.
///
/// Note that end VPN (exclusive) does fit in a u64 because a VPN is off by a
/// factor of 4096 for 64-bit virtual addresses.
struct CoveredPages(BTreeMap<u64, u64>);

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
            .filter(|(&end, &start)| {
                start <= new_range.end && end >= new_range.start
            })
            .map(|(&end, &start)| (end, start))
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
        self.0.insert(merged_end, merged_start);

        Ok(())
    }

    // fn existing_vpn_checked_add(existing_vpn: u64, existing_num_pages: u64) -> Range<u64> {
    //     existing_vpn..existing_vpn.checked_add(existing_num_pages).expect("Can't overflow because we validated previous input, and the algorithm should not result in this happening, but we definitely want to panic if it does")
    // }

    // fn merged_checked_sub(merged_end: u64, merged_start: u64) -> u64 {
    //     merged_end.checked_sub(merged_start).expect("Can't overflow because we validated previous input, and the algorithm should not result in this happening, but we definitely want to panic if it does")
    // }

    fn drain(self) -> impl IntoIterator<Item = (u64, u64)> {
        self.0.into_iter()
            .map(|(end, start)| {
                let number_of_pages = end.checked_sub(start).expect("Must be valid due to add_region input validation and algorithm");
                (start, number_of_pages)
            })
    }
}

// impl IntoIterator for CoveredPages {
//     type Item = (u64, u64);
//     type IntoIter = impl Iterator<Item = (u64, u64)>;

//     fn into_iter(self) -> Self::IntoIter {
//         self.0.into_iter()
//             .map(|(end, start)| {
//                 let number_of_pages = end.checked_sub(start).expect("Must be valid due to add_region input validation and algorithm");
//                 (start, number_of_pages)
//             })
//     }
// }

pub struct Loader<'space> {
    covered_pages: CoveredPages,
    shm_space: &'space mut ShmSpace,
}

impl<'space> Loader<'space> {
    pub fn new(shm_space: &'space mut ShmSpace) -> Self {
        Self { covered_pages: CoveredPages::new(), shm_space }
    }
}

impl<'space> ElfLoader for Loader<'space> {
    fn allocate(&mut self, load_headers: LoadableHeaders) -> Result<(), ElfLoaderErr> {
        for header in load_headers {
            let flags = header.flags();

            // Do not support sections which are both writable and executable, for now.
            if flags.is_write() && flags.is_execute() {
                log::error!(
                    "Section at vaddr {:#x} is both writable and executable, not supported at the moment, aborting loading program.",
                    header.virtual_addr(),
                );
                return Err(ElfLoaderErr::UnsupportedSectionData);
            }

            let rounded_down_start_vpn = header.virtual_addr() >> 12;
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

            // TODO: Allocate caps for executable memory, based on covered_pages.
        }

        Ok(())
    }

    fn load(&mut self, flags: elfloader::Flags, base: elfloader::VAddr, region: &[u8]) -> Result<(), ElfLoaderErr> {
        todo!()
    }

    fn relocate(&mut self, entry: elfloader::RelocationEntry) -> Result<(), ElfLoaderErr> {
        todo!()
    }
}

fn last_occupied_page_number(virtual_addr: u64, mem_size: u64) -> Result<u64, ()> {
    let mem_size_minus_one = mem_size.checked_sub(1).ok_or(())?;

    let last_byte_addr = virtual_addr
        .checked_add(mem_size_minus_one)
        .ok_or(())?;

    Ok(last_byte_addr >> 12)
}

mod tests {
    use super::*;

    #[test]
    fn covered_pages_overlapping_at_boundary() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(16, 3).expect("Should be valid");
        covered_pages.add_region(19, 20).expect("Should be valid");
        covered_pages.add_region(39, 5).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.drain().into_iter().collect();
        assert_eq!(vec![(16, 28)], merged_regions);
    }

    #[test]
    fn covered_pages_overlapping_at_boundary_backwards() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(39, 5).expect("Should be valid");
        covered_pages.add_region(19, 20).expect("Should be valid");
        covered_pages.add_region(16, 3).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.drain().into_iter().collect();
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

        let merged_regions: Vec<(u64, u64)> = covered_pages.drain().into_iter().collect();
        assert_eq!(vec![(1, 10), (21, 10)], merged_regions);
    }

    #[test]
    fn covered_pages_overlapping_partial() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(1, 10).expect("Should be valid");
        covered_pages.add_region(8, 5).expect("Should be valid");
        covered_pages.add_region(12, 2).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.drain().into_iter().collect();
        assert_eq!(vec![(1, 13)], merged_regions);
    }

    #[test]
    fn covered_pages_non_contiguous() {
        let mut covered_pages = CoveredPages::new();

        covered_pages.add_region(1, 2).expect("Should be valid");
        covered_pages.add_region(4, 3).expect("Should be valid");
        covered_pages.add_region(10, 4).expect("Should be valid");

        let merged_regions: Vec<(u64, u64)> = covered_pages.drain().into_iter().collect();
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
