use std::{collections::BTreeMap, ops::Bound, convert::TryInto};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use crate::nushift_subsystem::{ShmCapId, ShmCapLength, ShmSpace};

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

struct PageTableLevel2 {
    entries: [Option<Box<PageTableLeaf>>; 512],
}

struct PageTableLeaf {
    entries: [Option<PageTableEntry>; 512],
}

struct PageTableEntry {
    shm_cap_id: ShmCapId,
    offset: ShmCapLength,
}

pub fn walk<'space>(vpn: u64, page_table: &PageTableLevel1, shm_space: &'space ShmSpace) -> Result<&'space [u8], PageTableError> {
    let level_2_table = page_table.entries[(vpn & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?;
    // TODO: Support superpages
    let leaf_table = level_2_table.entries[((vpn >> 9) & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?;
    let entry = leaf_table.entries[((vpn >> 18) & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?;

    let shm_cap = shm_space.get(&entry.shm_cap_id).ok_or_else(|| PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, offset: None, shm_cap_length: None }.build())?;
    if entry.offset >= shm_cap.length() {
        return PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, offset: Some(entry.offset), shm_cap_length: Some(shm_cap.length()) }.fail();
    }
    let byte_start: usize = entry.offset
        .checked_mul(shm_cap.shm_type().page_bytes())
        .ok_or(PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, offset: Some(entry.offset), shm_cap_length: Some(shm_cap.length()) }.build())?
        .try_into()
        .map_err(|_| PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, offset: Some(entry.offset), shm_cap_length: Some(shm_cap.length()) }.build())?;
    let byte_end = byte_start
        .checked_add(
            shm_cap.shm_type().page_bytes().try_into().map_err(|_| PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, offset: Some(entry.offset), shm_cap_length: Some(shm_cap.length()) }.build())?
        )
        .ok_or(PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, offset: Some(entry.offset), shm_cap_length: Some(shm_cap.length()) }.build())?;

    Ok(&shm_cap.backing()[byte_start..byte_end])
}

#[derive(Snafu, SnafuCliDebug)]
pub enum PageTableError {
    #[snafu(display("The requested page was not present"))]
    PageNotFound,
    #[snafu(display("The SHM cap ID was not found or the offset was higher than the cap's length, both of which should never happen, and this indicates a bug in Nushift's code."))]
    PageEntryCorrupted { shm_cap_id: ShmCapId, offset: Option<ShmCapLength>, shm_cap_length: Option<ShmCapLength> },
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
