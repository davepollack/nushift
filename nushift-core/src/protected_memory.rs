use std::{collections::BTreeMap, ops::Bound};

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
    /// This function currently does not have the responsibility of checking if
    /// address + length_in_bytes overflows and if address is page aligned,
    /// which should be checked by something.
    /// This function may also be assuming that length_in_bytes is not 0?
    fn is_allowed(&self, address: u64, length_in_bytes: u64) -> bool {
        let mut equal_or_below = self.0.range((Bound::Unbounded, Bound::Included(&address)));
        let equal_or_below = equal_or_below.next_back();

        // Check if the equal or below entry intersects.
        if let Some((eq_or_below_addr, eq_or_below_length_in_bytes)) = equal_or_below {
            // If equal addresses, return not allowed. Assumes both length in
            // the map and passed-in length are not 0.
            if *eq_or_below_addr == address {
                return false;
            }

            // Assumes does not overflow, which should have been validated
            // before entries are inserted into the map.
            if eq_or_below_addr + eq_or_below_length_in_bytes > address {
                return false;
            }
        }

        let mut above = self.0.range((Bound::Excluded(&address), Bound::Unbounded));
        let above = above.next();

        // Check if intersects the above entry.
        if let Some((above_addr, _)) = above {
            // Assumes address + length_in_bytes does not overflow. I am
            // currently saying this should be checked before `is_allowed` is
            // called.
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

struct PageTableLevel1 {
    entries: [Option<Box<PageTableLevel2>>; 512],
}

struct PageTableLevel2 {
    entries: [Option<Box<PageTableLeaf>>; 512],
}

struct PageTableLeaf {
    entries: [PageTableEntry; 512],
}

struct PageTableEntry {

}
