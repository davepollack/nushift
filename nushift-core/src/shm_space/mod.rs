use std::{collections::{HashMap, hash_map::Entry}, io, ops::{Deref, DerefMut}, num::NonZeroU64};

use memmap2::MmapMut;
use num_enum::TryFromPrimitive;
use reusable_id_pool::{ReusableIdPoolError, ReusableIdPoolManual};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use self::acquisitions_and_page_table::{AcquisitionsAndPageTable, AcquireError, WalkResult, PageTableError, WalkResultMut, Sv39Flags};

pub mod acquisitions_and_page_table;

pub const SV39_BITS: u8 = 39;

#[derive(TryFromPrimitive, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum ShmType {
    // Support page sizes available in the Sv39 scheme.
    FourKiB = 0,
    TwoMiB = 1,
    OneGiB = 2,

    // If supporting the Sv32 scheme (needed for supporting RV32 i.e. 32-bit
    // apps), FourMiB which corresponds to the superpage in that scheme will be
    // supported.
    // FourMiB = ...,

    // When/if Sv48 is supported in the future, the FiveTwelveGiB superpage in
    // that scheme will be supported.
    // FiveTwelveGiB = ...,
}

impl ShmType {
    pub fn page_bytes(&self) -> u64 {
        match self {
            Self::FourKiB => 1 << 12,
            Self::TwoMiB => 1 << 21,
            Self::OneGiB => 1 << 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapType {
    UserCap,
    ElfCap,
}

#[derive(Debug)]
pub struct ShmCap<B = MmapMut> {
    shm_type: ShmType,
    length: ShmCapLength,
    backing: B,
    cap_type: CapType,
}
impl<B> ShmCap<B> {
    pub fn new(shm_type: ShmType, length: ShmCapLength, backing: B, cap_type: CapType) -> Self {
        ShmCap { shm_type, length, backing, cap_type }
    }

    pub fn shm_type(&self) -> ShmType {
        self.shm_type
    }

    pub fn length(&self) -> ShmCapLength {
        self.length
    }

    pub fn length_u64(&self) -> u64 {
        self.length.get()
    }

    pub fn cap_type(&self) -> CapType {
        self.cap_type
    }
}
impl<B> ShmCap<B>
where
    B: Deref<Target = [u8]>,
{
    pub fn backing(&self) -> &[u8] {
        &self.backing
    }
}
impl<B> ShmCap<B>
where
    B: DerefMut<Target = [u8]>,
{
    pub fn backing_mut(&mut self) -> &mut [u8] {
        &mut self.backing
    }
}
pub type ShmCapId = u64;
pub type ShmCapLength = NonZeroU64;
pub type ShmCapOffset = u64;
pub type ShmSpaceMap = HashMap<ShmCapId, ShmCap>;
pub type OwnedShmIdAndCap = (ShmCapId, ShmCap);
/// 0 = number of 1 GiB caps, 1 = number of 2 MiB caps, 2 = number of 4 KiB caps
type Sv39SpaceStats = [u32; 3];

pub struct ShmSpace {
    id_pool: ReusableIdPoolManual,
    space: ShmSpaceMap,
    acquisitions: AcquisitionsAndPageTable,
    stats: Sv39SpaceStats,
}

impl ShmSpace {
    pub fn new() -> Self {
        ShmSpace {
            id_pool: ReusableIdPoolManual::new(),
            space: HashMap::new(),
            acquisitions: AcquisitionsAndPageTable::new(),
            stats: [0; 3],
        }
    }

    pub fn new_shm_cap(&mut self, shm_type: ShmType, length: u64, cap_type: CapType) -> Result<(ShmCapId, &ShmCap), ShmSpaceError> {
        let length = NonZeroU64::new(length).ok_or(InvalidLengthSnafu.build())?;
        let length_u64 = length.get();

        if length_u64 > self.sv39_available_pages(shm_type).into() {
            return CapacityNotAvailableSnafu.fail();
        }

        // Since we have got past the sv39_available_pages check and it returns
        // a u32, we now know length is < 2^32.
        let sv39_length = length_u64 as u32;

        let mmap_mut = MmapMut::map_anon(
            shm_type.page_bytes()
                .checked_mul(length_u64)
                .ok_or(BackingCapacityNotAvailableOverflowsSnafu.build())?
                .try_into()
                .map_err(|_| BackingCapacityNotAvailableOverflowsSnafu.build())?
        ).context(BackingCapacityNotAvailableSnafu)?;

        let id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyLiveIDs => ExhaustedSnafu.build() })?;

        let shm_cap = match self.space.entry(id) {
            Entry::Occupied(_) => return DuplicateIdSnafu.fail(),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(ShmCap::new(shm_type, length, mmap_mut, cap_type)),
        };

        Self::sv39_increment_stats(&mut self.stats, shm_type, sv39_length);

        Ok((id, shm_cap))
    }

    pub fn acquire_shm_cap(&mut self, shm_cap_id: ShmCapId, address: u64) -> Result<(), ShmSpaceError> {
        let shm_cap = self.space.get(&shm_cap_id).ok_or_else(|| CapNotFoundSnafu.build())?;
        matches!(shm_cap.cap_type(), CapType::UserCap).then_some(()).ok_or_else(|| PermissionDeniedSnafu.build())?;

        Self::try_acquire(&mut self.acquisitions, shm_cap_id, shm_cap, address, Sv39Flags::RW)
    }

    pub fn acquire_shm_cap_elf(&mut self, shm_cap_id: ShmCapId, address: u64, flags: Sv39Flags) -> Result<(), ShmSpaceError> {
        let shm_cap = self.space.get(&shm_cap_id).ok_or_else(|| CapNotFoundSnafu.build())?;
        matches!(shm_cap.cap_type(), CapType::ElfCap).then_some(()).ok_or_else(|| PermissionDeniedSnafu.build())?;

        Self::try_acquire(&mut self.acquisitions, shm_cap_id, shm_cap, address, flags)
    }

    fn try_acquire(acquisitions: &mut AcquisitionsAndPageTable, shm_cap_id: ShmCapId, shm_cap: &ShmCap, address: u64, flags: Sv39Flags) -> Result<(), ShmSpaceError> {
        // try_acquire does the out-of-bounds and alignment checks. We map the errors here.
        acquisitions.try_acquire(shm_cap_id, shm_cap, address, flags)
            .map_err(|acquire_error| match acquire_error {
                AcquireError::AcquiringAlreadyAcquiredCap { address } => CurrentlyAcquiredCapSnafu { address }.build(),
                AcquireError::AcquireExceedsSv39 => AddressOutOfBoundsSnafu.build(),
                AcquireError::AcquireAddressNotPageAligned => AddressNotAlignedSnafu.build(),
                AcquireError::AcquireIntersectsExistingAcquisition => OverlapsExistingAcquisitionSnafu.build(),
                _ => AcquireReleaseInternalSnafu.build(),
            })
    }

    pub fn release_shm_cap(&mut self, shm_cap_id: ShmCapId, expected_cap_type: CapType) -> Result<(), ShmSpaceError> {
        let shm_cap = self.space.get(&shm_cap_id).ok_or_else(|| CapNotFoundSnafu.build())?;
        (shm_cap.cap_type() == expected_cap_type).then_some(()).ok_or_else(|| PermissionDeniedSnafu.build())?;

        match self.acquisitions.try_release(shm_cap_id, shm_cap) {
            Ok(_) => Ok(()),
            Err(AcquireError::ReleasingNonAcquiredCap) => Ok(()), // Silently allow releasing non-acquired cap.
            Err(_) => AcquireReleaseInternalSnafu.fail(),
        }
    }

    pub fn destroy_shm_cap(&mut self, shm_cap_id: ShmCapId, expected_cap_type: CapType) -> Result<(), ShmSpaceError> {
        let shm_cap = self.space.get(&shm_cap_id).ok_or_else(|| CapNotFoundSnafu.build())?;
        (shm_cap.cap_type() == expected_cap_type).then_some(()).ok_or_else(|| PermissionDeniedSnafu.build())?;
        self.acquisitions.check_not_acquired(shm_cap_id).map_err(|address| DestroyingCurrentlyAcquiredCapSnafu { address }.build())?;
        // TODO: Check that it must not be contained by any other dependents. E.g. accessibility tree.

        let shm_cap = self.space.remove(&shm_cap_id).ok_or_else(|| CapNotFoundSnafu.build())?; // This error should be impossible to get to because we checked contains_key above, but still use this error variant.
        self.id_pool.release(shm_cap_id);
        Self::sv39_decrement_stats(&mut self.stats, shm_cap);
        Ok(())
    }

    /// Moves *without* decrementing the Sv39 stats. So it's still reserved and
    /// can be moved back in.
    ///
    /// Precondition: Must be released. And probably must not be depended on by
    /// any other dependents.
    pub fn move_shm_cap_to_other_space(&mut self, shm_cap_id: ShmCapId) -> Option<ShmCap> {
        self.space.remove(&shm_cap_id)
    }

    /// Moves *without* incrementing the Sv39 stats. Because it wasn't
    /// decremented when it was moved out.
    pub fn move_shm_cap_back_into_space(&mut self, shm_cap_id: ShmCapId, shm_cap: ShmCap) {
        self.space.insert(shm_cap_id, shm_cap);
    }

    pub fn walk(&self, vaddr: u64) -> Result<WalkResult<'_>, PageTableError> {
        self.acquisitions.walk(vaddr, &self.space)
    }

    pub fn walk_mut(&mut self, vaddr: u64) -> Result<WalkResultMut<'_>, PageTableError> {
        self.acquisitions.walk_mut(vaddr, &mut self.space)
    }

    pub fn walk_execute(&self, vaddr: u64) -> Result<WalkResult<'_>, PageTableError> {
        self.acquisitions.walk_execute(vaddr, &self.space)
    }

    #[allow(dead_code)]
    pub fn get(&self, shm_cap_id: ShmCapId) -> Option<&ShmCap> {
        self.space.get(&shm_cap_id)
    }

    pub fn get_mut(&mut self, shm_cap_id: ShmCapId) -> Option<&mut ShmCap> {
        self.space.get_mut(&shm_cap_id)
    }

    /// This assumes that all pages can be arranged as: all 1 GiB ones first,
    /// then all 2 MiB ones, then all 4 KiB ones. This is probably a very dumb
    /// assumption and I might regret it later.
    fn sv39_available_pages(&self, shm_type: ShmType) -> u32 {
        match shm_type {
            ShmType::FourKiB => {
                let four_kib_equivalent_used = (self.stats[0] << 18) + (self.stats[1] << 9) + self.stats[2];
                let four_kib_total: u32 = 1 << (SV39_BITS - 12);
                four_kib_total - four_kib_equivalent_used
            },
            ShmType::TwoMiB => {
                let two_mib_equivalent_used = (self.stats[0] << 9) + self.stats[1] + ((self.stats[2] >> 9) + (self.stats[2] & ((1 << 9) - 1) != 0) as u32);
                let two_mib_total: u32 = 1 << (SV39_BITS - 21);
                two_mib_total - two_mib_equivalent_used
            },
            ShmType::OneGiB => {
                let four_kib_equivalent_used_excl_one_gib = (self.stats[1] << 9) + self.stats[2];
                let one_gib_equivalent_used_by_non_one_gib = (four_kib_equivalent_used_excl_one_gib >> 18) + (four_kib_equivalent_used_excl_one_gib & ((1 << 18) - 1) != 0) as u32;
                let one_gib_equivalent_used = self.stats[0] + one_gib_equivalent_used_by_non_one_gib;
                let one_gib_total: u32 = 1 << (SV39_BITS - 30);
                one_gib_total - one_gib_equivalent_used
            },
        }
    }

    /// sv39_available_pages(...) MUST be checked before calling this, otherwise
    /// this can cause an overflow.
    fn sv39_increment_stats(stats: &mut Sv39SpaceStats, shm_type: ShmType, sv39_length: u32) {
        match shm_type {
            ShmType::FourKiB => stats[2] += sv39_length,
            ShmType::TwoMiB => stats[1] += sv39_length,
            ShmType::OneGiB => stats[0] += sv39_length,
        }
    }

    /// The passed-in ShmCap must be one removed from a real space that we have
    /// bookkept correctly, otherwise this could underflow. To help achieve
    /// this, this accepts a ShmCap that is moved in, not borrowed.
    fn sv39_decrement_stats(stats: &mut Sv39SpaceStats, shm_cap: ShmCap) {
        match shm_cap.shm_type {
            ShmType::FourKiB => stats[2] -= shm_cap.length.get() as u32,
            ShmType::TwoMiB => stats[1] -= shm_cap.length.get() as u32,
            ShmType::OneGiB => stats[0] -= shm_cap.length.get() as u32,
        }
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum ShmSpaceError {
    #[snafu(display("The new pool ID was already present in the space. This should never happen, and indicates a bug in Nushift's code."))]
    DuplicateId,
    #[snafu(display("The maximum amount of SHM capabilities have been used for this app. Please destroy some capabilities."))]
    Exhausted,
    #[snafu(display("The length provided was invalid, for example 0 is invalid."))]
    InvalidLength,
    #[snafu(display("There is not enough available capacity to support this length of this SHM type."))]
    CapacityNotAvailable,
    #[snafu(display("There is not enough available backing capacity, currently using mmap, to support this length of this SHM type."))]
    BackingCapacityNotAvailable { source: io::Error },
    #[snafu(display("The requested capacity in bytes overflows either u64 or usize on this platform. Note that length in the system call arguments is number of this SHM type's pages, not number of bytes."))]
    BackingCapacityNotAvailableOverflows,
    #[snafu(display("The requested cap is currently acquired at address 0x{address:x} and thus cannot be acquired again. Please release it first."))]
    CurrentlyAcquiredCap { address: u64 },
    #[snafu(display("The requested cap is currently acquired at address 0x{address:x} and thus cannot be destroyed. Please release it first."))]
    DestroyingCurrentlyAcquiredCap { address: u64 },
    #[snafu(display("A cap with the requested cap ID was not found."))]
    CapNotFound,
    #[snafu(display("The requested acquisition address is not within Sv39 (39-bit virtual addressing) bounds."))]
    AddressOutOfBounds,
    #[snafu(display("The requested acquisition address is not aligned at the SHM cap's type (e.g. 4 KiB-aligned, 2 MiB-aligned or 1 GiB-aligned)."))]
    AddressNotAligned,
    #[snafu(display("The specified address combined with the length in the cap forms a range that overlaps an existing acquisition. Please choose a different address."))]
    OverlapsExistingAcquisition,
    #[snafu(display("An internal error occurred while acquiring or releasing. This should never happen and indicates a bug in Nushift's code."))]
    AcquireReleaseInternalError,
    #[snafu(display("Operation is not allowed on this cap, for example it is an ELF cap that the user app is not allowed to operate on."))]
    PermissionDenied,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shm_space_sv39_available_pages_none_used() {
        let shm_space = ShmSpace::new();

        assert_eq!(512, shm_space.sv39_available_pages(ShmType::OneGiB));
        assert_eq!(1 << (SV39_BITS - 21), shm_space.sv39_available_pages(ShmType::TwoMiB));
        assert_eq!(1 << (SV39_BITS - 12), shm_space.sv39_available_pages(ShmType::FourKiB));
    }

    #[test]
    fn shm_space_sv39_available_pages_one_gibs_used() {
        let mut shm_space = ShmSpace::new();
        shm_space.stats = [3, 0, 0];

        assert_eq!(509, shm_space.sv39_available_pages(ShmType::OneGiB));
        assert_eq!((1 << (SV39_BITS - 21)) - (3 << 9), shm_space.sv39_available_pages(ShmType::TwoMiB));
        assert_eq!((1 << (SV39_BITS - 12)) - (3 << 18), shm_space.sv39_available_pages(ShmType::FourKiB));
    }

    #[test]
    fn shm_space_sv39_available_pages_all_types_used() {
        let mut shm_space = ShmSpace::new();

        // Make a layout where a 1 GiB slot isn't completely used by 2 MiB
        // pages, and then 4 KiB pages fill the remainder and go over to the
        // next space.
        shm_space.stats = [3, 511, 513];

        assert_eq!(507, shm_space.sv39_available_pages(ShmType::OneGiB));
        assert_eq!((1 << (SV39_BITS - 21)) - (4 << 9) - 1, shm_space.sv39_available_pages(ShmType::TwoMiB));
        assert_eq!((1 << (SV39_BITS - 12)) - (4 << 18) - 1, shm_space.sv39_available_pages(ShmType::FourKiB));

        // Same, but 4 KiB pages fill exactly the remainder.
        shm_space.stats = [3, 511, 512];

        assert_eq!(508, shm_space.sv39_available_pages(ShmType::OneGiB));
        assert_eq!((1 << (SV39_BITS - 21)) - (4 << 9), shm_space.sv39_available_pages(ShmType::TwoMiB));
        assert_eq!((1 << (SV39_BITS - 12)) - (4 << 18), shm_space.sv39_available_pages(ShmType::FourKiB));

        // Same, but 4 KiB pages fill almost the remainder except one.
        shm_space.stats = [3, 511, 511];

        assert_eq!(508, shm_space.sv39_available_pages(ShmType::OneGiB));
        assert_eq!((1 << (SV39_BITS - 21)) - (4 << 9), shm_space.sv39_available_pages(ShmType::TwoMiB));
        assert_eq!((1 << (SV39_BITS - 12)) - (4 << 18) + 1, shm_space.sv39_available_pages(ShmType::FourKiB));
    }
}
