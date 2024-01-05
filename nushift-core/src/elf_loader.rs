// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::{BTreeMap, HashMap}, ops::Bound};

use elfloader::{ElfLoader, ElfLoaderErr, LoadableHeaders, Flags, VAddr, RelocationEntry};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use super::shm_space::{CapType, ShmSpace, ShmType, ShmCapId, acquisitions_and_page_table::Sv39Flags};

// The loader in this file should be robust against:
//
// * Overlapping ELF LOAD headers (including fully overlapping, i.e. specifying the same base and size)
// * Integer overflowing ELF LOAD headers when you add the base and size
// * ELF LOAD headers with a size of 0
// * An ELF with so many LOAD headers or with such a large total size that it exhausts our Sv39 stats capacity
//
// And more. As far as I know, the current implementation as of writing this
// comment is robust against these four, returning an ELF loading error for
// these four cases.

struct CheckedSections(BTreeMap<u64, u64>);

impl CheckedSections {
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    fn add_region(&mut self, vpn: u64, number_of_pages: u64) -> Result<(), CheckedSectionsError> {
        // This check is correct because vpn is off from u64::MAX by factor of
        // 4096 (if we did reach u64::MAX, it wouldn't be correct because it
        // would be valid to have a number of pages that reaches the end
        // overflowing to 0 exactly).
        let end_vpn = vpn.checked_add(number_of_pages).ok_or(VpnPlusNumPagesOverflowSnafu.build())?;

        // Check if the equal or below entry intersects.
        let mut equal_or_below = self.0.range((Bound::Unbounded, Bound::Included(&vpn)));
        let equal_or_below = equal_or_below.next_back();

        match equal_or_below {
            Some((&existing_vpn, &existing_num_pages)) if existing_vpn.checked_add(existing_num_pages).expect("Should be impossible for existing entry to overflow because we validated inputs") > vpn => return OverlapsSnafu.fail(),
            _ => {},
        }

        // Check if intersects the above entry.
        let mut above = self.0.range((Bound::Excluded(&vpn), Bound::Unbounded));
        let above = above.next();

        match above {
            Some((&above_vpn, _)) if end_vpn > above_vpn => return OverlapsSnafu.fail(),
            _ => {},
        }

        self.0.insert(vpn, number_of_pages);
        Ok(())
    }
}

#[derive(Snafu, SnafuCliDebug)]
enum CheckedSectionsError {
    VpnPlusNumPagesOverflow, // Internal error and indicates a bug in Nushift's code, since the VPN and number of pages are created by us by shifting u64s >> 12
    Overlaps,
}

pub struct Loader<'space> {
    vpn_to_shm_cap_id: HashMap<u64, ShmCapId>,
    shm_space: &'space mut ShmSpace,
}

impl<'space> Loader<'space> {
    pub fn new(shm_space: &'space mut ShmSpace) -> Self {
        Self { vpn_to_shm_cap_id: HashMap::new(), shm_space }
    }
}

fn flags_map(flags: Flags) -> Result<Sv39Flags, ()> {
    // The compiler can't tell that is_read, is_write, is_execute from a
    // different crate don't have side effects, so it keeps calling them if we
    // don't do them at the top here. Perhaps a different LTO mode would behave
    // differently.
    let (r, w, x) = (flags.is_read(), flags.is_write(), flags.is_execute());
    match () {
        _ if r && !w && !x => Ok(Sv39Flags::R),
        _ if r && w && !x => Ok(Sv39Flags::RW),
        _ if r && !w && x => Ok(Sv39Flags::RX),
        _ if !r && !w && x => Ok(Sv39Flags::X),
        _ => Err(()),
    }
}

fn last_occupied_page_number(virtual_addr: u64, mem_size: u64) -> Result<u64, ()> {
    let mem_size_minus_one = mem_size.checked_sub(1).ok_or(())?;

    let last_byte_addr = virtual_addr
        .checked_add(mem_size_minus_one)
        .ok_or(())?;

    Ok(last_byte_addr >> 12)
}

impl ElfLoader for Loader<'_> {
    fn allocate(&mut self, load_headers: LoadableHeaders) -> Result<(), ElfLoaderErr> {
        let mut checked_sections = CheckedSections::new();
        let mut errored_caps = vec![];

        for header in load_headers {
            let flags = header.flags();

            // Only allow certain combinations of flags.
            let sv39_flags = flags_map(flags).map_err(|_| {
                tracing::error!(
                    "Section at vaddr {:#x} has unsupported flags, the only supported combinations are r--, rw-, r-x, --x.",
                    header.virtual_addr(),
                );
                ElfLoaderErr::UnsupportedSectionData
            })?;

            let rounded_down_start_vpn = header.virtual_addr() >> 12;

            let last_occupied_vpn = last_occupied_page_number(header.virtual_addr(), header.mem_size())
                .map_err(|_| {
                    tracing::error!(
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

            checked_sections.add_region(rounded_down_start_vpn, number_of_pages)
                .map_err(|err| {
                    match err {
                        CheckedSectionsError::VpnPlusNumPagesOverflow => tracing::error!(
                            "Section at vaddr {:#x} and mem_size {:#x}: An internal error when adding VPN and number of pages occurred, this should never happen regardless of the data in the ELF and indicates a bug in Nushift's code.",
                            header.virtual_addr(),
                            header.mem_size(),
                        ),
                        CheckedSectionsError::Overlaps => tracing::error!(
                            concat!(
                                "Section at vaddr {:#x} and mem_size {:#x} when rounded down to the nearest 4 KiB page, overlaps a previously loaded section. ",
                                "The reason why we don't currently allow sub-page sections that also overlap a particular page, is because we apply section permissions on a page level, and if your sections have the same permissions, we don't yet support merging them.",
                            ),
                            header.virtual_addr(),
                            header.mem_size(),
                        ),
                    }
                    ElfLoaderErr::UnsupportedSectionData
                })?;

            // If new_shm_cap fails, no need to clean up or destroy
            // anything. It makes sure that it only increments the Sv39
            // stats after no other errors have occurred. If the
            // implementation of new_shm_cap changes such that this is no
            // longer the case, and you didn't check this usage of
            // new_shm_cap, well, that is not good.
            let (shm_cap_id, _) = self.shm_space.new_shm_cap(ShmType::FourKiB, number_of_pages, CapType::ElfCap)
                .map_err(|err| {
                    tracing::error!("ELF loading: new_shm_cap either exhausted or is an internal error: {err:?}");
                    ElfLoaderErr::UnsupportedSectionData
                })?;

            match self.shm_space.acquire_shm_cap_elf(shm_cap_id, rounded_down_start_vpn << 12, sv39_flags) {
                Ok(_) => {},
                Err(err) => {
                    // TODO: It's not necessarily an internal error. We haven't yet checked that it doesn't exceed 2^39.
                    tracing::error!("ELF loading: acquire_shm_cap internal error: {err:?}");
                    errored_caps.push(shm_cap_id);
                    break;
                }
            }

            self.vpn_to_shm_cap_id.insert(rounded_down_start_vpn, shm_cap_id);
        }

        if errored_caps.len() > 0 {
            for shm_cap_id in errored_caps.into_iter().rev() {
                self.shm_space.release_shm_cap_elf(shm_cap_id)
                    .map_err(|err| {
                        tracing::error!("Error while rolling back, release_shm_cap internal error: {err:?}");
                        ElfLoaderErr::UnsupportedSectionData
                    })?;
                self.shm_space.destroy_shm_cap(shm_cap_id, CapType::ElfCap)
                    .map_err(|err| {
                        tracing::error!("Error while rolling back, destroy_shm_cap internal error: {err:?}");
                        ElfLoaderErr::UnsupportedSectionData
                    })?;
            }
            Err(ElfLoaderErr::UnsupportedSectionData)
        } else {
            Ok(())
        }
    }

    fn load(&mut self, flags: Flags, base: VAddr, region: &[u8]) -> Result<(), ElfLoaderErr> {
        tracing::debug!(
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

        let rounded_down_start_vpn = base >> 12;
        let offset: usize = (base & ((1 << 12) - 1))
            .try_into()
            .map_err(|_| {
                tracing::error!("usize on this host machine is less than 12 bits. This is unexpected.");
                ElfLoaderErr::UnsupportedSectionData
            })?;

        let &shm_cap_id = self.vpn_to_shm_cap_id.get(&rounded_down_start_vpn).ok_or_else(|| {
            tracing::error!("Internal error refetching SHM cap ID when loading ELF data");
            ElfLoaderErr::UnsupportedSectionData
        })?;

        self.shm_space.get_mut_shm_cap_elf(shm_cap_id)
            .map_err(|_| {
                tracing::error!("Internal error refetching SHM cap when loading ELF data");
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
    fn checked_sections_overlapping_at_boundary() {
        let mut checked_sections = CheckedSections::new();

        checked_sections.add_region(16, 3).expect("Should be allowed");
        checked_sections.add_region(19, 20).expect("Should be allowed");
        checked_sections.add_region(39, 5).expect("Should be allowed");
    }

    #[test]
    fn checked_sections_overlapping_at_boundary_backwards() {
        let mut checked_sections = CheckedSections::new();

        checked_sections.add_region(39, 5).expect("Should be allowed");
        checked_sections.add_region(19, 20).expect("Should be allowed");
        checked_sections.add_region(16, 3).expect("Should be allowed");
    }

    #[test]
    fn checked_sections_overlapping_inner_and_outer() {
        let mut checked_sections = CheckedSections::new();

        assert!(matches!(checked_sections.add_region(1, 10), Ok(())));
        assert!(matches!(checked_sections.add_region(5, 1), Err(CheckedSectionsError::Overlaps)));
        // Add them in the other order
        let mut checked_sections = CheckedSections::new();
        assert!(matches!(checked_sections.add_region(25, 1), Ok(())));
        assert!(matches!(checked_sections.add_region(21, 10), Err(CheckedSectionsError::Overlaps)));
    }

    #[test]
    fn checked_sections_overlapping_partial() {
        let mut checked_sections = CheckedSections::new();

        assert!(matches!(checked_sections.add_region(1, 10), Ok(())));
        assert!(matches!(checked_sections.add_region(10, 3), Err(CheckedSectionsError::Overlaps)));
    }

    #[test]
    fn checked_sections_non_contiguous() {
        let mut checked_sections = CheckedSections::new();

        checked_sections.add_region(1, 2).expect("Should be allowed");
        checked_sections.add_region(4, 3).expect("Should be allowed");
        checked_sections.add_region(10, 4).expect("Should be allowed");
    }

    #[test]
    fn checked_sections_invalid_input() {
        let mut checked_sections = CheckedSections::new();

        // u64::MAX shouldn't be a valid VPN (because a VPN is off by a factor
        // of 4096), so it's valid to do an overflow check.
        assert!(matches!(checked_sections.add_region(u64::MAX, 1), Err(CheckedSectionsError::VpnPlusNumPagesOverflow)));
    }
}
