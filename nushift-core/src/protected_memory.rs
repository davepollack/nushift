use std::{collections::{BTreeMap, HashMap}, ops::Bound, array::TryFromSliceError, mem};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use crate::nushift_subsystem::{ShmCapId, ShmCapLength, ShmSpace, ShmType, ShmCap, SV39_BITS};

// Costs of mapping, unmapping and accessing memory in this file:
//
// We keep a binary search tree of used regions, used when mapping and unmapping
// (also known as acquiring and releasing), but this is NOT hit on memory
// accesses.
//
// For accesses, we walk a page table which is technically constant time, though
// far from free. Both the page table and the BST should be kept consistent.

pub struct AcquisitionsAndPageTable {
    acquisitions: Acquisitions,
    page_table: Box<PageTableLevel1>,
}

impl AcquisitionsAndPageTable {
    pub fn new() -> Self {
        Self { acquisitions: Acquisitions::new(), page_table: Box::new(PageTableLevel1::new()) }
    }

    pub fn try_acquire(&mut self, shm_cap_id: ShmCapId, shm_cap: &ShmCap, address: u64) -> Result<(), AcquireError> {
        // Check that it doesn't exceed Sv39. First check 2^64, then 2^39.
        let length_in_bytes = shm_cap.shm_type().page_bytes()
            .checked_mul(shm_cap.length())
            .ok_or_else(|| AcquireExceedsSv39Snafu.build())?;

        let end_address = address
            .checked_add(length_in_bytes)
            .ok_or_else(|| AcquireExceedsSv39Snafu.build())?;

        if end_address > 1 << SV39_BITS {
            return AcquireExceedsSv39Snafu.fail();
        }

        // Check that address is page aligned.
        if address & (shm_cap.shm_type().page_bytes() - 1) != 0 {
            return AcquireAddressNotPageAlignedSnafu.fail();
        }

        // Insert to acquisitions.
        self.acquisitions.try_insert(shm_cap_id, address, length_in_bytes).map_err(|_| AcquireIntersectsExistingAcquisitionSnafu.build())?;
        // Insert to page table.
        match self.page_table.insert(shm_cap_id, shm_cap, address).context(AcquirePageTableInsertSnafu) {
            Ok(_) => {},
            Err(err) => {
                // Roll back the acquisitions insert.
                //
                // We shouldn't actually get here (i.e. an error when inserting
                // the page table), this indicates data structure corruption and
                // a bug in Nushift's code.
                self.acquisitions.remove(shm_cap_id).map_err(|_| AcquireRollbackSnafu.build())?;
                // Now return the error.
                return Err(err);
            }
        }

        Ok(())
    }

    pub fn is_acquired(&self, shm_cap_id: ShmCapId) -> Option<&ShmAcquisitionAddress> {
        self.acquisitions.is_acquired(shm_cap_id)
    }

    pub fn try_release(&mut self, shm_cap_id: ShmCapId, shm_cap: &ShmCap) -> Result<(), AcquireError> {
        // Remove from acquisitions.
        let address = self.acquisitions.remove(shm_cap_id).map_err(|_| AcquireReleasingNonAcquiredCapSnafu.build())?;
        // Remove from page table.
        match self.page_table.remove(shm_cap_id, shm_cap, address).context(AcquirePageTableInsertSnafu) {
            Ok(_) => {},
            Err(err) => {
                // Roll back the acquisitions remove.
                //
                // We shouldn't actually get here (i.e. an error when inserting
                // the page table), this indicates data structure corruption and
                // a bug in Nushift's code.
                let length_in_bytes = shm_cap.shm_type().page_bytes()
                    .checked_mul(shm_cap.length())
                    .ok_or_else(|| AcquireRollbackSnafu.build())?;
                self.acquisitions.try_insert(shm_cap_id, address, length_in_bytes).map_err(|_| AcquireRollbackSnafu.build())?;
                // Now return the error.
                return Err(err);
            },
        }

        Ok(())
    }

    pub fn walk<'space>(&self, vaddr: u64, shm_space: &'space ShmSpace) -> Result<WalkResult<'space>, PageTableError> {
        walk(vaddr, &self.page_table, shm_space)
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum AcquireError {
    AcquireExceedsSv39,
    AcquireAddressNotPageAligned,
    AcquireIntersectsExistingAcquisition,
    AcquireReleasingNonAcquiredCap,
    AcquirePageTableInsertError { source: PageTableError }, // Should never occur, indicates a bug in Nushift's code
    AcquireRollbackError, // Should never occur, indicates a bug in Nushift's code
}

type ShmAcquisitionAddress = u64;
type ShmAcquisitionLength = u64;

struct Acquisitions {
    cap_tracking: HashMap<ShmCapId, ShmAcquisitionAddress>,
    acquisitions: BTreeMap<ShmAcquisitionAddress, ShmAcquisitionLength>,
}

impl Acquisitions {
    fn new() -> Self {
        Self { cap_tracking: HashMap::new(), acquisitions: BTreeMap::new() }
    }

    /// address + length_in_bytes not overflowing, address being page aligned,
    /// and length_in_bytes > 0, must be checked before this function is called.
    fn is_allowed(&self, address: u64, length_in_bytes: u64) -> bool {
        let mut equal_or_below = self.acquisitions.range((Bound::Unbounded, Bound::Included(&address)));
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

        let mut above = self.acquisitions.range((Bound::Excluded(&address), Bound::Unbounded));
        let above = above.next();

        // Check if intersects the above entry.
        if let Some((above_addr, _)) = above {
            // Assumes address + length_in_bytes does not overflow. This is
            // checked before `is_allowed` is called.
            if address + length_in_bytes > *above_addr {
                return false;
            }
        }

        return true;
    }

    /// Check `is_allowed()` before calling this.
    fn insert(&mut self, shm_cap_id: ShmCapId, address: u64, length_in_bytes: u64) {
        self.cap_tracking.insert(shm_cap_id, address);
        self.acquisitions.insert(address, length_in_bytes);
    }

    /// Before calling this function, checking that address + length_in_bytes
    /// doesn't overflow and that address is page aligned, MUST be performed by
    /// the call site.
    /// This function also assumes that length_in_bytes is not 0. If it comes
    /// from a ShmCap, it won't be, since length is validated to be greater than
    /// 0 for an ShmCap to be registered.
    fn try_insert(&mut self, shm_cap_id: ShmCapId, address: u64, length_in_bytes: u64) -> Result<(), ()> {
        if self.is_allowed(address, length_in_bytes) {
            self.insert(shm_cap_id, address, length_in_bytes);
            Ok(())
        } else {
            Err(())
        }
    }

    fn is_acquired(&self, shm_cap_id: ShmCapId) -> Option<&ShmAcquisitionAddress> {
        self.cap_tracking.get(&shm_cap_id)
    }

    fn remove(&mut self, shm_cap_id: ShmCapId) -> Result<ShmAcquisitionAddress, ()> {
        let address = self.cap_tracking.remove(&shm_cap_id).ok_or(())?;
        self.acquisitions.remove(&address);
        Ok(address)
    }
}

pub struct PageTableLevel1 {
    entries: [Option<Box<PageTableLevel2>>; Self::NUM_ENTRIES],
}

enum PageTableLevel2 {
    Entries([Option<Box<PageTableLeaf>>; Self::NUM_ENTRIES]),
    OneGiBSuperpage(PageTableEntry),
}

enum PageTableLeaf {
    Entries([Option<PageTableEntry>; Self::NUM_ENTRIES]),
    TwoMiBSuperpage(PageTableEntry),
}

struct PageTableEntry {
    shm_cap_id: ShmCapId,
    /// The offset within the ShmCap referred to by `shm_cap_id`. For example,
    /// an ShmCap can have a length of 3, and a shm_cap_offset of 1 means we are
    /// associated with the second page of that cap.
    shm_cap_offset: ShmCapLength,
}

#[derive(Debug, Clone, Copy)]
enum PageTableOp {
    Insert,
    Remove,
}

impl PageTableLevel1 {
    const ENTRIES_BITS: u8 = 9;
    const NUM_ENTRIES: usize = 1 << Self::ENTRIES_BITS;

    fn new() -> Self {
        Self { entries: core::array::from_fn(|_| None) }
    }

    /// Check `is_allowed()` on Acquisitions before calling this. This also
    /// doesn't check whether `address` is aligned nor fits within Sv39, which
    /// should be checked by something.
    fn insert<B>(&mut self, shm_cap_id: ShmCapId, shm_cap: &ShmCap<B>, address: u64) -> Result<(), PageTableError> {
        self.insert_or_remove(PageTableOp::Insert, shm_cap_id, shm_cap, address)
    }

    /// Check `is_allowed()` on Acquisitions before calling this. This also
    /// doesn't check whether `address` is aligned nor fits within Sv39, which
    /// should be checked by something.
    fn remove<B>(&mut self, shm_cap_id: ShmCapId, shm_cap: &ShmCap<B>, address: u64) -> Result<(), PageTableError> {
        self.insert_or_remove(PageTableOp::Remove, shm_cap_id, shm_cap, address)
    }

    /// Check `is_allowed()` on Acquisitions before calling this. This also
    /// doesn't check whether `address` is aligned nor fits within Sv39, which
    /// should be checked by something.
    fn insert_or_remove<B>(&mut self, op: PageTableOp, shm_cap_id: ShmCapId, shm_cap: &ShmCap<B>, address: u64) -> Result<(), PageTableError> {
        let vpn2 = address >> 30;
        let vpn1 = (address >> 21) & ((1 << 9) - 1);
        let vpn0 = (address >> 12) & ((1 << 9) - 1);

        match shm_cap.shm_type() {
            ShmType::OneGiB => {
                let (start, end) = (
                    vpn2,
                    vpn2.checked_add(shm_cap.length())
                        .ok_or(PageInsertOutOfBoundsSnafu { shm_type: ShmType::OneGiB, length: shm_cap.length(), address }.build())?,
                );
                if end > PageTableLevel1::NUM_ENTRIES as u64 {
                    return PageInsertOutOfBoundsSnafu { shm_type: ShmType::OneGiB, length: shm_cap.length(), address }.fail();
                }
                for i in start..end {
                    let offset = i - start;
                    match op {
                        PageTableOp::Insert => self.entries[i as usize] = Some(Box::new(PageTableLevel2::OneGiBSuperpage(PageTableEntry { shm_cap_id, shm_cap_offset: offset }))),
                        PageTableOp::Remove => self.entries[i as usize] = None,
                    }
                }
                Ok(())
            },

            ShmType::TwoMiB => {
                let (start_vpn1, end_vpn1) = (
                    vpn1,
                    vpn1.checked_add(shm_cap.length())
                        .ok_or(PageInsertOutOfBoundsSnafu { shm_type: ShmType::TwoMiB, length: shm_cap.length(), address }.build())?,
                );

                let absolute_end_vpn1 = (vpn2 << PageTableLevel2::ENTRIES_BITS) + end_vpn1;
                if absolute_end_vpn1 > 1u64 << PageTableLevel1::ENTRIES_BITS << PageTableLevel2::ENTRIES_BITS {
                    return PageInsertOutOfBoundsSnafu { shm_type: ShmType::TwoMiB, length: shm_cap.length(), address }.fail();
                }

                for current_vpn1 in start_vpn1..end_vpn1 {
                    // Initialise level 2 table or get existing
                    let current_vpn2 = vpn2 + (current_vpn1 >> PageTableLevel2::ENTRIES_BITS);
                    let level_2_table = self.entries[current_vpn2 as usize].get_or_insert_with(|| Box::new(PageTableLevel2::Entries(core::array::from_fn(|_| None))));

                    let level_2_table = match level_2_table.as_mut() {
                        PageTableLevel2::OneGiBSuperpage(_) => return PageInsertCorruptedSnafu { shm_cap_id, vpn2: current_vpn2, current_vpn1: Some(current_vpn1), current_vpn0: None }.fail(),
                        PageTableLevel2::Entries(entries) => entries,
                    };

                    let current_vpn1_index = (current_vpn1 & ((1 << PageTableLevel2::ENTRIES_BITS) - 1)) as usize;
                    match op {
                        PageTableOp::Insert => {
                            // TODO: If you run this allocating line, 200,000 times, it is slow.
                            level_2_table[current_vpn1_index] = Some(Box::new(PageTableLeaf::TwoMiBSuperpage(PageTableEntry { shm_cap_id, shm_cap_offset: (current_vpn1 - start_vpn1) })));
                        },
                        PageTableOp::Remove => {
                            // TODO: If you run this dropping line, 200,000 times, it is slow.
                            level_2_table[current_vpn1_index] = None;

                            // TODO: How can we free self.entries[current_vpn2
                            // as usize] when no entries are occupied anymore,
                            // without looping through all entries?
                        },
                    }
                }
                Ok(())
            },

            ShmType::FourKiB => {
                let (start_vpn0, end_vpn0) = (
                    vpn0,
                    vpn0.checked_add(shm_cap.length())
                        .ok_or(PageInsertOutOfBoundsSnafu { shm_type: ShmType::FourKiB, length: shm_cap.length(), address }.build())?,
                );

                let absolute_end_vpn0 = (vpn2 << PageTableLevel2::ENTRIES_BITS << PageTableLeaf::ENTRIES_BITS) + (vpn1 << PageTableLeaf::ENTRIES_BITS) + end_vpn0;
                if absolute_end_vpn0 > 1u64 << PageTableLevel1::ENTRIES_BITS << PageTableLevel2::ENTRIES_BITS << PageTableLeaf::ENTRIES_BITS {
                    return PageInsertOutOfBoundsSnafu { shm_type: ShmType::FourKiB, length: shm_cap.length(), address }.fail();
                }

                for current_vpn0 in start_vpn0..end_vpn0 {
                    // Initialise level 2 table or get existing
                    let current_vpn1 = vpn1 + (current_vpn0 >> PageTableLeaf::ENTRIES_BITS);
                    let current_vpn2 = vpn2 + (current_vpn1 >> PageTableLevel2::ENTRIES_BITS);
                    let level_2_table = self.entries[current_vpn2 as usize].get_or_insert_with(|| Box::new(PageTableLevel2::Entries(core::array::from_fn(|_| None))));

                    let level_2_table = match level_2_table.as_mut() {
                        PageTableLevel2::OneGiBSuperpage(_) => return PageInsertCorruptedSnafu { shm_cap_id, vpn2: current_vpn2, current_vpn1: None, current_vpn0: Some(current_vpn0) }.fail(),
                        PageTableLevel2::Entries(entries) => entries,
                    };

                    // Initialise leaf table or get existing
                    let current_vpn1_index = (current_vpn1 & ((1 << PageTableLevel2::ENTRIES_BITS) - 1)) as usize;
                    let leaf_table = level_2_table[current_vpn1_index].get_or_insert_with(|| Box::new(PageTableLeaf::Entries(core::array::from_fn(|_| None))));

                    let leaf_table = match leaf_table.as_mut() {
                        PageTableLeaf::TwoMiBSuperpage(_) => return PageInsertCorruptedSnafu { shm_cap_id, vpn2: current_vpn2, current_vpn1: Some(current_vpn1), current_vpn0: Some(current_vpn0) }.fail(),
                        PageTableLeaf::Entries(entries) => entries,
                    };

                    let current_vpn0_index = (current_vpn0 & ((1 << PageTableLeaf::ENTRIES_BITS) - 1)) as usize;
                    match op {
                        PageTableOp::Insert => leaf_table[current_vpn0_index] = Some(PageTableEntry { shm_cap_id, shm_cap_offset: (current_vpn0 - start_vpn0) }),
                        PageTableOp::Remove => {
                            leaf_table[current_vpn0_index] = None;

                            // TODO: How can we free self.entries[current_vpn2
                            // as usize] when no entries are occupied anymore,
                            // without looping through all entries?

                            // TODO: How can we free
                            // level_2_table[current_vpn1_index] when no entries
                            // are occupied anymore, without looping through all
                            // entries?
                        }
                    }
                }
                Ok(())
            },
        }
    }
}

impl PageTableLevel2 {
    const ENTRIES_BITS: u8 = 9;
    const NUM_ENTRIES: usize = 1 << Self::ENTRIES_BITS;
}

impl PageTableLeaf {
    const ENTRIES_BITS: u8 = 9;
    const NUM_ENTRIES: usize = 1 << Self::ENTRIES_BITS;
}

pub struct WalkResult<'space> {
    /// This is always one page. The page size depends on the SHM cap that was
    /// walked.
    space_slice: &'space [u8],
    byte_offset_in_space_slice: usize,
}

pub fn walk<'space>(vaddr: u64, page_table: &PageTableLevel1, shm_space: &'space ShmSpace) -> Result<WalkResult<'space>, PageTableError> {
    let vpn = vaddr >> 12;
    let level_2_table = page_table.entries[(vpn & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?;

    let (entry, shm_cap) = 'superpage_check: {
        let leaf_table = match level_2_table.as_ref() {
            PageTableLevel2::OneGiBSuperpage(pte) => {
                let shm_cap = shm_space.get(pte.shm_cap_id).ok_or_else(|| PageEntryCorruptedSnafu { shm_cap_id: pte.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: None, shm_cap_length: None }.build())?;
                check_shm_type_mismatch(1, &pte, shm_cap, ShmType::OneGiB)?;
                break 'superpage_check (pte, shm_cap);
            },
            PageTableLevel2::Entries(entries) => entries[((vpn >> 9) & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?,
        };

        let four_k_entry = match leaf_table.as_ref() {
            PageTableLeaf::TwoMiBSuperpage(pte) => {
                let shm_cap = shm_space.get(pte.shm_cap_id).ok_or_else(|| PageEntryCorruptedSnafu { shm_cap_id: pte.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: None, shm_cap_length: None }.build())?;
                check_shm_type_mismatch(2, &pte, shm_cap, ShmType::TwoMiB)?;
                break 'superpage_check (pte, shm_cap);
            },
            PageTableLeaf::Entries(entries) => entries[((vpn >> 18) & ((1 << 9) - 1)) as usize].as_ref().ok_or(PageNotFoundSnafu.build())?,
        };
        let shm_cap = shm_space.get(four_k_entry.shm_cap_id).ok_or_else(|| PageEntryCorruptedSnafu { shm_cap_id: four_k_entry.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: None, shm_cap_length: None }.build())?;
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
        .map_err(|_| PageTooLargeToFitInHostPlatformWordSnafu { shm_cap_id: entry.shm_cap_id, shm_type: shm_cap.shm_type(), offset: entry.shm_cap_offset }.build())?;
    let byte_end = byte_start
        .checked_add(
            shm_cap.shm_type().page_bytes().try_into().map_err(|_| PageTooLargeToFitInHostPlatformWordSnafu { shm_cap_id: entry.shm_cap_id, shm_type: shm_cap.shm_type(), offset: entry.shm_cap_offset }.build())?
        )
        .ok_or(PageEntryCorruptedSnafu { shm_cap_id: entry.shm_cap_id, mismatched_entry_found_at_level: None, shm_cap_offset: Some(entry.shm_cap_offset), shm_cap_length: Some(shm_cap.length()) }.build())?;

    let space_slice = &shm_cap.backing()[byte_start..byte_end];
    let byte_offset_in_space_slice = (vaddr & (shm_cap.shm_type().page_bytes() - 1))
        .try_into()
        .map_err(|_| PageTooLargeToFitInHostPlatformWordSnafu { shm_cap_id: entry.shm_cap_id, shm_type: shm_cap.shm_type(), offset: entry.shm_cap_offset }.build())?;

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
    #[snafu(display("Page insert out of bounds due to cap length being too high. This can certainly be caused by the user, but it should have been checked before we got to this page insert function."))]
    PageInsertOutOfBounds { shm_type: ShmType, length: ShmCapLength, address: u64 },
    #[snafu(display("Could not insert page due to pages already being present. Although attempting to map to an already-mapped range is a user-visible error, this particular variant should never occur and indicates a bug in Nushift's code."))]
    PageInsertCorrupted { shm_cap_id: ShmCapId, vpn2: u64, current_vpn1: Option<u64>, current_vpn0: Option<u64> },
    #[snafu(display("The requested page was not present"))]
    PageNotFound,
    #[snafu(display("The SHM cap ID was not found or the offset was higher than the cap's length, both of which should never happen, and this indicates a bug in Nushift's code."))]
    PageEntryCorrupted { shm_cap_id: ShmCapId, mismatched_entry_found_at_level: Option<(u8, ShmType)>, shm_cap_offset: Option<ShmCapLength>, shm_cap_length: Option<ShmCapLength> },
    #[snafu(display("A large superpage {shm_type:?} offset at {offset}, does not fit into the host platform's usize of {} bytes. For example, running some 64-bit Nushift apps on a 32-bit host platform. This limitation of Nushift could be resolved in the future.", core::mem::size_of::<usize>()))]
    PageTooLargeToFitInHostPlatformWord { shm_cap_id: ShmCapId, shm_type: ShmType, offset: ShmCapLength },
}

struct ProtectedMemory;

impl ProtectedMemory {
    fn check_within_sv39(addr: u64, word_bytes: usize) -> Result<(), ProtectedMemoryError> {
        if addr > (1 << SV39_BITS) - (word_bytes as u64) {
            OutOfSv39Snafu.fail()
        } else {
            Ok(())
        }
    }

    pub fn load8(&self, shm_space: &ShmSpace, addr: u64) -> Result<u8, ProtectedMemoryError> {
        Self::check_within_sv39(addr, 1)?;
        let walked = shm_space.acquisitions().walk(addr, shm_space).context(WalkSnafu)?;
        Ok(walked.space_slice[walked.byte_offset_in_space_slice])
    }

    pub fn load_multi_byte<T: Numeric>(&self, shm_space: &ShmSpace, addr: u64) -> Result<T, ProtectedMemoryError> {
        Self::check_within_sv39(addr, mem::size_of::<T>())?;
        let walked = shm_space.acquisitions().walk(addr, shm_space).context(WalkSnafu)?;

        // Fits in page (all aligned accesses are this, and some unaligned accesses).
        let diff_to_end_of_space_slice = walked.space_slice.len() - walked.byte_offset_in_space_slice;
        if diff_to_end_of_space_slice >= mem::size_of::<T>() {
            let bytes: T::ByteArray = walked.space_slice[walked.byte_offset_in_space_slice..walked.byte_offset_in_space_slice+mem::size_of::<T>()].try_into().unwrap();
            let word = T::from_le_bytes(bytes);
            return Ok(word);
        }

        // Goes onto next page. Not common, only unaligned accesses can do this.

        // Doesn't overflow, and is a valid argument to `walk`, because we
        // called `check_within_sv39` at the beginning.
        //
        // Casting to u64 is OK because a page size can't be more than u64 as
        // long as `addr` is still u64.
        let next_page = addr + (diff_to_end_of_space_slice as u64);
        let walked_next_page = shm_space.acquisitions().walk(next_page, shm_space).context(WalkSnafu)?;

        let bytes: T::ByteArray = [&walked.space_slice[walked.byte_offset_in_space_slice..], &walked_next_page.space_slice[..(mem::size_of::<T>() - diff_to_end_of_space_slice)]]
            .concat().try_into().unwrap();
        let word = T::from_le_bytes(bytes);
        Ok(word)
    }

    pub fn load16(&self, shm_space: &ShmSpace, addr: u64) -> Result<u16, ProtectedMemoryError> {
        self.load_multi_byte(shm_space, addr)
    }

    pub fn load32(&self, shm_space: &ShmSpace, addr: u64) -> Result<u32, ProtectedMemoryError> {
        self.load_multi_byte(shm_space, addr)
    }

    pub fn load64(&self, shm_space: &ShmSpace, addr: u64) -> Result<u64, ProtectedMemoryError> {
        self.load_multi_byte(shm_space, addr)
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum ProtectedMemoryError {
    OutOfSv39,
    WalkError { source: PageTableError },
}

/// Alternatively, could use the `funty` crate for this.
trait Numeric {
    type ByteArray: for<'a> TryFrom<&'a [u8], Error = TryFromSliceError> + TryFrom<Vec<u8>, Error = Vec<u8>>;
    fn from_le_bytes(bytes: Self::ByteArray) -> Self;
}
macro_rules! impl_numeric {
    ($($t:ty),+) => { $(
        impl Numeric for $t {
            type ByteArray = [u8; mem::size_of::<Self>()];
            fn from_le_bytes(bytes: Self::ByteArray) -> Self { Self::from_le_bytes(bytes) }
        }
    )+ };
}
impl_numeric!(u16, u32, u64);

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_ONE_GIB_PAGE: u64 = 1u64 << PageTableLevel2::ENTRIES_BITS << PageTableLeaf::ENTRIES_BITS << 12;

    #[test]
    fn acquisitions_is_allowed_empty_allowed() {
        let acquisitions = Acquisitions::new();

        assert!(acquisitions.is_allowed(0x30000, 0x2000));
    }

    #[test]
    fn acquisitions_is_allowed_boundary_of_previous_region_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(1, 0x30000, 0x2000).expect("should work");

        assert!(acquisitions.is_allowed(0x32000, 0x1000));
    }

    #[test]
    fn acquisitions_is_allowed_same_address_not_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(1, 0x30000, 0x2000).expect("should work");

        assert!(!acquisitions.is_allowed(0x30000, 0x1000));
    }

    #[test]
    fn acquisitions_is_allowed_boundary_of_above_region_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(1, 0x30000, 0x2000).expect("should work");

        assert!(acquisitions.is_allowed(0x2f000, 0x1000));
    }

    #[test]
    fn acquisitions_is_allowed_intersects_below_region_not_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(1, 0x30000, 0x2000).expect("should work");

        assert!(!acquisitions.is_allowed(0x31fff, 0x1000));
    }

    #[test]
    fn acquisitions_is_allowed_intersects_above_region_not_allowed() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(1, 0x30000, 0x2000).expect("should work");

        assert!(!acquisitions.is_allowed(0x2f001, 0x1000));
    }

    #[test]
    fn acquisitions_try_insert_is_ok_is_err() {
        let mut acquisitions = Acquisitions::new();

        assert!(acquisitions.try_insert(1, 0x30000, 0x2000).is_ok());
        assert!(acquisitions.try_insert(2, 0x30000, 0x1000).is_err());
    }

    #[test]
    fn acquisitions_remove_removes() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(1, 0x30000, 0x2000).expect("should work");
        acquisitions.remove(1).expect("should not fail");

        assert!(acquisitions.is_allowed(0x30000, 0x2000));
    }

    #[test]
    fn acquisitions_remove_non_existent_id() {
        let mut acquisitions = Acquisitions::new();
        acquisitions.try_insert(1, 0x30000, 0x2000).expect("should work");
        assert!(matches!(acquisitions.remove(2), Err(())));
    }

    #[test]
    fn page_table_insert_one_gib_out_of_bounds() {
        let mut page_table = PageTableLevel1::new();

        // A 1 GiB type cap starting at 400 with length 200: overflows 512
        let (shm_type, length, backing) = (ShmType::OneGiB, 200, &[0u8; 0]);
        let address = 400u64 << PageTableLevel2::ENTRIES_BITS << PageTableLeaf::ENTRIES_BITS << 12;
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(shm_type, length, backing), address),
            Err(PageTableError::PageInsertOutOfBounds { shm_type: m_shm_type, length: m_length, address: m_address }) if m_shm_type == shm_type && m_length == length && m_address == address,
        ));
        // Assert that nothing was inserted
        assert!(page_table.entries.iter().all(|entry| matches!(entry, None)));
    }

    #[test]
    fn page_table_insert_one_gib_boundary_ok() {
        let mut page_table = PageTableLevel1::new();

        // A 1 GiB type cap starting at 0 with length 512: fits
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::OneGiB, 512, &[0u8; 0]), 0),
            Ok(()),
        ));
        assert!(page_table.entries.iter().enumerate().all(|(i, entry)| {
            let Some(entry) = entry else { return false; };
            matches!(entry.as_ref(), PageTableLevel2::OneGiBSuperpage(PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_i }) if *m_i == i as u64)
        }));

        // A 1 GiB type cap starting at 400 with length 112: fits
        let mut page_table = PageTableLevel1::new();
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::OneGiB, 112, &[0u8; 0]), 400u64 << PageTableLevel2::ENTRIES_BITS << PageTableLeaf::ENTRIES_BITS << 12),
            Ok(()),
        ));
        assert!(page_table.entries[0..400].iter().all(|entry| matches!(entry, None)));
        assert!(page_table.entries[400..].iter().enumerate().all(|(i, entry)| {
            let Some(entry) = entry else { return false; };
            matches!(entry.as_ref(), PageTableLevel2::OneGiBSuperpage(PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_i }) if *m_i == i as u64)
        }));
    }

    #[test]
    fn page_table_insert_one_gib_middle_ok() {
        let mut page_table = PageTableLevel1::new();

        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::OneGiB, 1, &[0u8; 0]), 100u64 << PageTableLevel2::ENTRIES_BITS << PageTableLeaf::ENTRIES_BITS << 12),
            Ok(()),
        ));
        assert!(page_table.entries.iter().enumerate().all(|(i, entry)| {
            if i != 100 {
                matches!(entry, None)
            } else {
                let Some(entry) = entry else { return false; };
                matches!(entry.as_ref(), PageTableLevel2::OneGiBSuperpage(PageTableEntry { shm_cap_id: 1, shm_cap_offset: 0 }))
            }
        }))
    }

    #[test]
    fn page_table_insert_two_mib_out_of_bounds() {
        let mut page_table = PageTableLevel1::new();

        // A 2 MiB cap starting at 261118 (2 MiB equivalent) and has length 1027: overflows
        let address = (509 * ONE_ONE_GIB_PAGE) + (510u64 << PageTableLeaf::ENTRIES_BITS << 12);
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::TwoMiB, 1027, &[0u8; 0]), address),
            Err(PageTableError::PageInsertOutOfBounds { shm_type: ShmType::TwoMiB, length: 1027, address: m_address }) if m_address == address,
        ));
        assert!(page_table.entries.iter().all(|entry| matches!(entry, None))); // Expect all 1 GiB pages to not be populated

        // A 2 MiB cap starting at 261118 (2 MiB equivalent) and has length 1026: does NOT overflow
        let mut page_table = PageTableLevel1::new();
        let address = (509 * ONE_ONE_GIB_PAGE) + (510u64 << PageTableLeaf::ENTRIES_BITS << 12);
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::TwoMiB, 1026, &[0u8; 0]), address),
            Ok(()),
        ));
    }

    #[test]
    fn page_table_insert_two_mib_no_boundaries_crossed() {
        let mut page_table = PageTableLevel1::new();

        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::TwoMiB, 50, &[0u8; 0]), ONE_ONE_GIB_PAGE + (100u64 << PageTableLeaf::ENTRIES_BITS << 12)),
            Ok(()),
        ));

        assert!(matches!(page_table.entries[0], None)); // Expect first 1 GiB page to be not populated
        let level_2_table = page_table.entries[1].as_ref().expect("Expected second 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected second 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries.iter().enumerate().all(|(i, entry)| {
            if i < 100 || i >= 150 {
                matches!(entry, None)
            } else {
                let Some(entry) = entry else { return false; };
                matches!(entry.as_ref(), PageTableLeaf::TwoMiBSuperpage(PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_offset }) if *m_offset == (i - 100) as u64)
            }
        }));

        assert!(page_table.entries[2..].iter().all(|entry| matches!(entry, None))); // Expect remaining 1 GiB pages to not be populated
    }

    #[test]
    fn page_table_insert_two_mib_boundaries_crossed() {
        let mut page_table = PageTableLevel1::new();

        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::TwoMiB, 1000, &[0u8; 0]), ONE_ONE_GIB_PAGE + (510u64 << PageTableLeaf::ENTRIES_BITS << 12)),
            Ok(()),
        ));

        assert!(matches!(page_table.entries[0], None)); // Expect first 1 GiB page to be not populated
        let level_2_table = page_table.entries[1].as_ref().expect("Expected second 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected second 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries.iter().enumerate().all(|(i, entry)| {
            if i < 510 {
                matches!(entry, None)
            } else {
                let Some(entry) = entry else { return false; };
                matches!(entry.as_ref(), PageTableLeaf::TwoMiBSuperpage(PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_offset }) if *m_offset == (i - 510) as u64)
            }
        }));

        let level_2_table_2 = page_table.entries[2].as_ref().expect("Expected third 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries_2) = level_2_table_2.as_ref() else { panic!("Expected third 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries_2.iter().enumerate().all(|(i, entry)| {
            let Some(entry) = entry else { return false; };
            matches!(entry.as_ref(), PageTableLeaf::TwoMiBSuperpage(PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_offset }) if *m_offset == (i + 2) as u64)
        }));

        let level_2_table_3 = page_table.entries[3].as_ref().expect("Expected fourth 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries_3) = level_2_table_3.as_ref() else { panic!("Expected fourth 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries_3.iter().enumerate().all(|(i, entry)| {
            if i >= 486 {
                matches!(entry, None)
            } else {
                let Some(entry) = entry else { return false; };
                matches!(entry.as_ref(), PageTableLeaf::TwoMiBSuperpage(PageTableEntry{ shm_cap_id: 1, shm_cap_offset: m_offset }) if *m_offset == (i + 514) as u64)
            }
        }));

        assert!(page_table.entries[4..].iter().all(|entry| matches!(entry, None))); // Expect remaining 1 GiB pages to not be populated
    }

    #[test]
    fn page_table_insert_two_mib_get_existing() {
        let mut page_table = PageTableLevel1::new();

        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::TwoMiB, 1, &[0u8; 0]), ONE_ONE_GIB_PAGE + (100u64 << PageTableLeaf::ENTRIES_BITS << 12)),
            Ok(()),
        ));

        // Insert another one at 101. This should trigger the get case of
        // get_or_insert_with, reusing the existing level 2 table.
        assert!(matches!(
            page_table.insert(2, &ShmCap::new(ShmType::TwoMiB, 1, &[0u8; 0]), ONE_ONE_GIB_PAGE + (101u64 << PageTableLeaf::ENTRIES_BITS << 12)),
            Ok(()),
        ));

        assert!(matches!(page_table.entries[0], None)); // Expect first 1 GiB page to be not populated
        let level_2_table = page_table.entries[1].as_ref().expect("Expected second 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected second 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries.iter().enumerate().all(|(i, entry)| {
            match i {
                100 => {
                    let Some(entry) = entry else { return false; };
                    matches!(entry.as_ref(), PageTableLeaf::TwoMiBSuperpage(PageTableEntry{ shm_cap_id: 1, shm_cap_offset: 0 }))
                },
                101 => {
                    let Some(entry) = entry else { return false; };
                    matches!(entry.as_ref(), PageTableLeaf::TwoMiBSuperpage(PageTableEntry{ shm_cap_id: 2, shm_cap_offset: 0 }))
                },
                _ => matches!(entry, None),
            }
        }));
    }

    #[test]
    fn page_table_insert_four_kib_out_of_bounds() {
        let mut page_table = PageTableLevel1::new();

        // A 4 KiB cap starting at the second-last 4 KiB slot within the
        // third-last 1 GiB region, with length 524291: overflows
        let address = (509 * ONE_ONE_GIB_PAGE) + (511u64 << PageTableLeaf::ENTRIES_BITS << 12) + (510u64 << 12);
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::FourKiB, 524291, &[0u8; 0]), address),
            Err(PageTableError::PageInsertOutOfBounds { shm_type: ShmType::FourKiB, length: 524291, address: m_address }) if m_address == address,
        ));
        assert!(page_table.entries.iter().all(|entry| matches!(entry, None))); // Expect all 1 GiB pages to not be populated

        // A 4 KiB cap starting at the second-last 4 KiB slot within the
        // third-last 1 GiB region, with length 524290: does NOT overflow
        let mut page_table = PageTableLevel1::new();
        let address = (509 * ONE_ONE_GIB_PAGE) + (511u64 << PageTableLeaf::ENTRIES_BITS << 12) + (510u64 << 12);
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::FourKiB, 524290, &[0u8; 0]), address),
            Ok(()),
        ));
    }

    #[test]
    fn page_table_insert_four_kib() {
        let mut page_table = PageTableLevel1::new();

        // A 4 KiB cap starting at the second-last 4 KiB slot within the
        // third-last 1 GiB region, with length 262746:
        //
        // Fills those two second-last 4 KiB slots, then fills the whole
        // second-last 1 GiB region (262144 4 KiB equivalent), then fills a 2
        // MiB region (512 4 KiB equivalent), then fills 88 more 4 KiB slots.
        let address = (509 * ONE_ONE_GIB_PAGE) + (511u64 << PageTableLeaf::ENTRIES_BITS << 12) + (510u64 << 12);
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::FourKiB, 262746, &[0u8; 0]), address),
            Ok(()),
        ));

        assert!(page_table.entries[0..509].iter().all(|entry| matches!(entry, None))); // Expect first 509 1 GiB pages to not be populated

        // Check 510th 1 GiB page
        let level_2_table = page_table.entries[509].as_ref().expect("Expected 510th 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected 510th 1 GiB page to be entries, not superpage"); };

        assert!(level_2_entries[0..511].iter().all(|entry| matches!(entry, None))); // Expect first 511 2 MiB pages to not be populated
        let leaf_table = level_2_entries[511].as_ref().expect("Expected 512th 2 MiB page to be populated");
        let PageTableLeaf::Entries(leaf_entries) = leaf_table.as_ref() else { panic!("Expected 512th 2 MiB page to be entries, not superpage"); };
        assert!(leaf_entries.iter().enumerate().all(|(i, entry)| {
            if i < 510 {
                matches!(entry, None)
            } else {
                let Some(entry) = entry else { return false; };
                matches!(entry, PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_offset } if *m_offset == (i - 510) as u64)
            }
        }));

        // Check 511th 1 GiB page. Should all be occupied.
        let level_2_table = page_table.entries[510].as_ref().expect("Expected 511th 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected 511th 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries.iter().enumerate().all(|(j, leaf_table)| {
            let Some(leaf_table) = leaf_table else { return false; };
            let PageTableLeaf::Entries(leaf_entries) = leaf_table.as_ref() else { return false; };
            leaf_entries.iter().enumerate().all(|(i, entry)| {
                let Some(entry) = entry else { return false; };
                matches!(entry, PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_offset } if *m_offset == ((j * 512) + i + 2) as u64)
            })
        }));

        // Check 512th 1 GiB page
        let level_2_table = page_table.entries[511].as_ref().expect("Expected 512th 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected 512th 1 GiB page to be entries, not superpage"); };
        let leaf_table = level_2_entries[0].as_ref().expect("Expected 1st 2 MiB page to be populated");
        let PageTableLeaf::Entries(leaf_entries) = leaf_table.as_ref() else { panic!("Expected 1st 2 MiB page to be entries, not superpage"); };
        assert!(leaf_entries.iter().enumerate().all(|(i, entry)| {
            let Some(entry) = entry else { return false; };
            matches!(entry, PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_offset } if *m_offset == (i + 262146) as u64)
        }));
        let leaf_table = level_2_entries[1].as_ref().expect("Expected 2nd 2 MiB page to be populated");
        let PageTableLeaf::Entries(leaf_entries) = leaf_table.as_ref() else { panic!("Expected 2nd 2 MiB page to be entries, not superpage"); };
        assert!(leaf_entries.iter().enumerate().all(|(i, entry)| {
            if i >= 88 {
                matches!(entry, None)
            } else {
                let Some(entry) = entry else { return false; };
                matches!(entry, PageTableEntry { shm_cap_id: 1, shm_cap_offset: m_offset } if *m_offset == (i + 262658) as u64)
            }
        }));
        assert!(level_2_entries[2..].iter().all(|entry| matches!(entry, None))); // Expect remaining 2 MiB pages to not be populated
    }

    #[test]
    fn page_table_remove_one_gib_boundary() {
        let mut page_table = PageTableLevel1::new();

        // A 1 GiB type cap starting at 400 with length 112: fits
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::OneGiB, 112, &[0u8; 0]), 400u64 << PageTableLevel2::ENTRIES_BITS << PageTableLeaf::ENTRIES_BITS << 12),
            Ok(()),
        ));
        // Remove: succeeds
        assert!(matches!(
            page_table.remove(1, &ShmCap::new(ShmType::OneGiB, 112, &[0u8; 0]), 400u64 << PageTableLevel2::ENTRIES_BITS << PageTableLeaf::ENTRIES_BITS << 12),
            Ok(()),
        ));
        // Check it was removed
        assert!(page_table.entries.iter().all(|entry| matches!(entry, None)));
    }

    #[test]
    fn page_table_remove_two_mib_boundaries_crossed() {
        let mut page_table = PageTableLevel1::new();

        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::TwoMiB, 1000, &[0u8; 0]), ONE_ONE_GIB_PAGE + (510u64 << PageTableLeaf::ENTRIES_BITS << 12)),
            Ok(()),
        ));
        assert!(matches!(
            page_table.remove(1, &ShmCap::new(ShmType::TwoMiB, 1000, &[0u8; 0]), ONE_ONE_GIB_PAGE + (510u64 << PageTableLeaf::ENTRIES_BITS << 12)),
            Ok(()),
        ));

        assert!(matches!(page_table.entries[0], None)); // Expect first 1 GiB page to be not populated
        let level_2_table = page_table.entries[1].as_ref().expect("Expected second 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected second 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries.iter().all(|entry| matches!(entry, None)));

        let level_2_table = page_table.entries[2].as_ref().expect("Expected third 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected third 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries.iter().all(|entry| matches!(entry, None)));

        let level_2_table = page_table.entries[3].as_ref().expect("Expected fourth 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected fourth 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries.iter().all(|entry| matches!(entry, None)));

        assert!(page_table.entries[4..].iter().all(|entry| matches!(entry, None))); // Expect remaining 1 GiB pages to not be populated
    }

    #[test]
    fn page_table_remove_four_kib() {
        let mut page_table = PageTableLevel1::new();

        // A 4 KiB cap starting at the second-last 4 KiB slot within the
        // third-last 1 GiB region, with length 262746:
        //
        // Fills those two second-last 4 KiB slots, then fills the whole
        // second-last 1 GiB region (262144 4 KiB equivalent), then fills a 2
        // MiB region (512 4 KiB equivalent), then fills 88 more 4 KiB slots.
        let address = (509 * ONE_ONE_GIB_PAGE) + (511u64 << PageTableLeaf::ENTRIES_BITS << 12) + (510u64 << 12);
        assert!(matches!(
            page_table.insert(1, &ShmCap::new(ShmType::FourKiB, 262746, &[0u8; 0]), address),
            Ok(()),
        ));
        assert!(matches!(
            page_table.remove(1, &ShmCap::new(ShmType::FourKiB, 262746, &[0u8; 0]), address),
            Ok(()),
        ));

        assert!(page_table.entries[0..509].iter().all(|entry| matches!(entry, None))); // Expect first 509 1 GiB pages to not be populated

        // Check 510th 1 GiB page
        let level_2_table = page_table.entries[509].as_ref().expect("Expected 510th 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected 510th 1 GiB page to be entries, not superpage"); };

        assert!(level_2_entries[0..511].iter().all(|entry| matches!(entry, None))); // Expect first 511 2 MiB pages to not be populated
        let leaf_table = level_2_entries[511].as_ref().expect("Expected 512th 2 MiB page to be populated");
        let PageTableLeaf::Entries(leaf_entries) = leaf_table.as_ref() else { panic!("Expected 512th 2 MiB page to be entries, not superpage"); };
        assert!(leaf_entries.iter().all(|entry| matches!(entry, None)));

        // Check 511th 1 GiB page
        let level_2_table = page_table.entries[510].as_ref().expect("Expected 511th 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected 511th 1 GiB page to be entries, not superpage"); };
        assert!(level_2_entries.iter().all(|leaf_table| {
            let Some(leaf_table) = leaf_table else { return false; };
            let PageTableLeaf::Entries(leaf_entries) = leaf_table.as_ref() else { return false; };
            leaf_entries.iter().all(|entry| matches!(entry, None))
        }));

        // Check 512th 1 GiB page
        let level_2_table = page_table.entries[511].as_ref().expect("Expected 512th 1 GiB page to be populated");
        let PageTableLevel2::Entries(level_2_entries) = level_2_table.as_ref() else { panic!("Expected 512th 1 GiB page to be entries, not superpage"); };
        let leaf_table = level_2_entries[0].as_ref().expect("Expected 1st 2 MiB page to be populated");
        let PageTableLeaf::Entries(leaf_entries) = leaf_table.as_ref() else { panic!("Expected 1st 2 MiB page to be entries, not superpage"); };
        assert!(leaf_entries.iter().all(|entry| matches!(entry, None)));
        let leaf_table = level_2_entries[1].as_ref().expect("Expected 2nd 2 MiB page to be populated");
        let PageTableLeaf::Entries(leaf_entries) = leaf_table.as_ref() else { panic!("Expected 2nd 2 MiB page to be entries, not superpage"); };
        assert!(leaf_entries.iter().all(|entry| matches!(entry, None)));
        assert!(level_2_entries[2..].iter().all(|entry| matches!(entry, None))); // Expect remaining 2 MiB pages to not be populated
    }
}
