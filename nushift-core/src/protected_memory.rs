use std::{collections::BTreeMap, ops::Bound, convert::TryInto};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use crate::nushift_subsystem::{ShmCapId, ShmCapLength, ShmSpace, ShmType, ShmCap};

// Costs of mapping, unmapping and accessing memory in this file:
//
// We keep a binary search tree of used regions, used when mapping and unmapping
// (also known as acquiring and releasing), but this is NOT hit on memory
// accesses.
//
// For accesses, we walk a page table which is technically constant time, though
// far from free. Both the page table and the BST should be kept consistent.

type ShmAcquisitionAddress = u64;
type ShmAcquisitionLength = u64;

struct Acquisitions(BTreeMap<ShmAcquisitionAddress, ShmAcquisitionLength>);

impl Acquisitions {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// This function currently does not have the responsibility of checking if
    /// address + length_in_bytes overflows and if address is page aligned,
    /// which should be checked by something.
    /// This function may also be assuming that length_in_bytes is not 0?
    fn is_allowed(&self, address: u64, length_in_bytes: u64) -> bool {
        let mut equal_or_below = self.0.range((Bound::Unbounded, Bound::Included(&address)));
        let equal_or_below = equal_or_below.next_back();

        // Check if the equal or below entry intersects.
        if let Some((eq_or_below_addr, eq_or_below_length_in_bytes)) = equal_or_below {
            // If equal addresses, not allowed. Assumes both length in the map
            // and passed-in length are not 0.
            if *eq_or_below_addr == address {
                return false;
            }

            // Assumes this does not overflow, which should have been validated
            // before entries are inserted into the map.
            if eq_or_below_addr + eq_or_below_length_in_bytes > address {
                return false;
            }
        }

        let mut above = self.0.range((Bound::Excluded(&address), Bound::Unbounded));
        let above = above.next();

        // Check if intersects the above entry.
        if let Some((above_addr, _)) = above {
            // Assumes address + length_in_bytes does not overflow. Currently, I
            // am thinking this should be checked before `is_allowed` is called.
            if address + length_in_bytes > *above_addr {
                return false;
            }
        }

        return true;
    }

    /// Check `is_allowed()` before calling this.
    fn insert(&mut self, address: u64, length_in_bytes: u64) {
        self.0.insert(address, length_in_bytes);
    }

    /// This function currently does not have the responsibility of checking if
    /// address + length_in_bytes overflows and if address is page aligned,
    /// which should be checked by something.
    /// This function may also be assuming that length_in_bytes is not 0?
    pub fn try_insert(&mut self, address: u64, length_in_bytes: u64) -> Result<(), ()> {
        if self.is_allowed(address, length_in_bytes) {
            self.insert(address, length_in_bytes);
            Ok(())
        } else {
            Err(())
        }
    }

    /// `length_in_bytes` is part of the interface because `free` interfaces
    /// should have that interface, even though it's not currently used in this
    /// case, but it could be in the future.
    pub fn free(&mut self, address: u64, _length_in_bytes: u64) {
        self.0.remove(&address);
    }
}

pub struct PageTableLevel1 {
    entries: [Option<Box<PageTableLevel2>>; 512],
}

enum PageTableLevel2 {
    Entries([Option<Box<PageTableLeaf>>; 512]),
    OneGiBSuperpage(PageTableEntry),
}

enum PageTableLeaf {
    Entries([Option<PageTableEntry>; 512]),
    TwoMiBSuperpage(PageTableEntry),
}

struct PageTableEntry {
    shm_cap_id: ShmCapId,
    shm_cap_offset: ShmCapLength,
}

pub struct WalkResult<'space> {
    space_slice: &'space [u8],
    byte_offset_in_space_slice: usize,
}

pub fn walk<'space>(vaddr: u64, page_table: &PageTableLevel1, shm_space: &'space ShmSpace) -> Result<WalkResult<'space>, PageTableError> {
    let vpn = vaddr >> 12;
    let level_2_table = page_table.entries[(vpn & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?;

    let (entry, shm_cap) = 'superpage_check: {
        let leaf_table = match level_2_table.as_ref() {
            PageTableLevel2::OneGiBSuperpage(pte) => {
                let shm_cap = shm_space.get(&pte.shm_cap_id).ok_or_else(|| PageEntryCorruptedSnafu { shm_cap_id: pte.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: None, shm_cap_length: None }.build())?;
                check_shm_type_mismatch(1, &pte, shm_cap, ShmType::OneGiB)?;
                break 'superpage_check (pte, shm_cap);
            },
            PageTableLevel2::Entries(entries) => entries[((vpn >> 9) & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?,
        };

        let four_k_entry = match leaf_table.as_ref() {
            PageTableLeaf::TwoMiBSuperpage(pte) => {
                let shm_cap = shm_space.get(&pte.shm_cap_id).ok_or_else(|| PageEntryCorruptedSnafu { shm_cap_id: pte.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: None, shm_cap_length: None }.build())?;
                check_shm_type_mismatch(2, &pte, shm_cap, ShmType::TwoMiB)?;
                break 'superpage_check (pte, shm_cap);
            },
            PageTableLeaf::Entries(entries) => entries[((vpn >> 18) & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?,
        };
        let shm_cap = shm_space.get(&four_k_entry.shm_cap_id).ok_or_else(|| PageEntryCorruptedSnafu { shm_cap_id: four_k_entry.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: None, shm_cap_length: None }.build())?;
        check_shm_type_mismatch(3, &four_k_entry, shm_cap, ShmType::FourKiB)?;

        (four_k_entry, shm_cap)
    };

    if entry.shm_cap_offset >= shm_cap.length() {
        return PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: Some(entry.shm_cap_offset), shm_cap_length: Some(shm_cap.length()) }.fail();
    }
    let byte_start: usize = entry.shm_cap_offset
        .checked_mul(shm_cap.shm_type().page_bytes())
        .ok_or(PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: Some(entry.shm_cap_offset), shm_cap_length: Some(shm_cap.length()) }.build())?
        .try_into()
        .map_err(|_| PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: Some(entry.shm_cap_offset), shm_cap_length: Some(shm_cap.length()) }.build())?;
    let byte_end = byte_start
        .checked_add(
            shm_cap.shm_type().page_bytes().try_into().map_err(|_| PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: Some(entry.shm_cap_offset), shm_cap_length: Some(shm_cap.length()) }.build())?
        )
        .ok_or(PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: Some(entry.shm_cap_offset), shm_cap_length: Some(shm_cap.length()) }.build())?;

    let space_slice = &shm_cap.backing()[byte_start..byte_end];
    // TODO: If the size of a superpage is greater than the word size of the
    // platform this hypervisor is running on, this cast will currently panic.
    let byte_offset_in_space_slice = (vaddr & (shm_cap.shm_type().page_bytes() - 1)) as usize;

    Ok(WalkResult { space_slice, byte_offset_in_space_slice })
}

fn check_shm_type_mismatch(current_level: u8, entry: &PageTableEntry, shm_cap: &ShmCap, expected_shm_type: ShmType) -> Result<(), PageTableError> {
    if shm_cap.shm_type() != expected_shm_type {
        PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, mismatched_entry_found_at_level: Some((current_level, shm_cap.shm_type())), shm_cap_offset: None, shm_cap_length: None }.fail()
    } else {
        Ok(())
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum PageTableError {
    #[snafu(display("The requested page was not present"))]
    PageNotFound,
    #[snafu(display("The SHM cap ID was not found or the offset was higher than the cap's length, both of which should never happen, and this indicates a bug in Nushift's code."))]
    PageEntryCorrupted { shm_cap_id: ShmCapId, mismatched_entry_found_at_level: Option<(u8, ShmType)>, shm_cap_offset: Option<ShmCapLength>, shm_cap_length: Option<ShmCapLength> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquisitions_is_allowed_empty_allowed() {
        let acquisitions = Acquisitions::new();

        assert!(acquisitions.is_allowed(0x30000, 0x2000));
    }

    #[test]
    fn acquisitions_is_allowed_boundary_of_previous_region_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(0x30000, 0x2000).expect("should work");

        assert!(acquisitions.is_allowed(0x32000, 0x1000));
    }

    #[test]
    fn acquisitions_is_allowed_same_address_not_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(0x30000, 0x2000).expect("should work");

        assert!(!acquisitions.is_allowed(0x30000, 0x1000));
    }

    #[test]
    fn acquisitions_is_allowed_boundary_of_above_region_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(0x30000, 0x2000).expect("should work");

        assert!(acquisitions.is_allowed(0x2f000, 0x1000));
    }

    #[test]
    fn acquisitions_is_allowed_intersects_below_region_not_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(0x30000, 0x2000).expect("should work");

        assert!(!acquisitions.is_allowed(0x31fff, 0x1000));
    }

    #[test]
    fn acquisitions_is_allowed_intersects_above_region_not_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(0x30000, 0x2000).expect("should work");

        assert!(!acquisitions.is_allowed(0x2f001, 0x1000));
    }

    #[test]
    fn acquisitions_try_insert_is_ok_is_err() {
        let mut acquisitions = Acquisitions::new();

        assert!(acquisitions.try_insert(0x30000, 0x2000).is_ok());
        assert!(acquisitions.try_insert(0x30000, 0x1000).is_err());
    }

    #[test]
    fn acquisitions_free_frees() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(0x30000, 0x2000).expect("should work");
        acquisitions.free(0x30000, 0x2000);

        assert!(acquisitions.is_allowed(0x30000, 0x2000));
    }
}
