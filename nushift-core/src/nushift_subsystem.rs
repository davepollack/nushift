use ckb_vm::{Error as CKBVMError, registers::{A0, A1, A2, T0}, Register, CoreMachine};
use num_enum::{TryFromPrimitive, IntoPrimitive};
use reusable_id_pool::{ReusableIdPoolError, ReusableIdPoolManual};
use snafu::Snafu;
use snafu_cli_debug::SnafuCliDebug;
use std::{convert::TryFrom, collections::{BTreeMap, btree_map::Entry}};

use super::process_control_block::ProcessControlBlock;

// Regarding the use of `u64`s in this file:
//
// It is intended that riscv32 should use the same numbers. I don't want to have
// duplicate enums/duplicate code for riscv32 apps.
//
// My idea is that riscv32 apps can encode the numbers in a multi-register
// encoding. But at the same time, I don't want that to use as many as 3
// registers, so these numbers should be u63, not u64.
//
// (Technically, 2 registers can encode slighly more than a u63, but a u63
// fits.)

#[derive(TryFromPrimitive)]
#[repr(u64)]
enum Syscall {
    Exit = 0,
    ShmNew = 1,
    ShmAcquire = 2,
    ShmNewAndAcquire = 3,
    ShmRelease = 4,
    ShmDestroy = 5,
    ShmReleaseAndDestroy = 6,
}

#[derive(IntoPrimitive)]
#[repr(u64)]
pub enum SyscallError {
    UnknownSyscall = 0,

    ShmDuplicateId = 1,
    ShmExhausted = 2,
    ShmUnknownShmType = 3,
    ShmInvalidLength = 4,
    ShmCapacityNotAvailable = 5,
}

const SYSCALL_NUM_REGISTER: usize = A0;
const FIRST_ARG_REGISTER: usize = A1;
const SECOND_ARG_REGISTER: usize = A2;
const RETURN_VAL_REGISTER: usize = A0;
/// a1 is used by the RISC-V calling conventions for a second return value,
/// rather than t0, but my concern is with the whole 32-bit app thing using
/// multiple registers to encode a 64-bit value. Maybe the 32-bit ABI will just
/// use a0 and a2 and the 64-bit will use a0 and a1. For now, using t0.
const ERROR_RETURN_VAL_REGISTER: usize = T0;

const SV39_BITS: u8 = 39;

#[derive(TryFromPrimitive, Debug, Clone, Copy)]
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

#[derive(Debug)]
pub struct ShmCap {
    shm_type: ShmType,
    length: u64,
}
impl ShmCap {
    fn new(shm_type: ShmType, length: u64) -> Self {
        ShmCap { shm_type, length }
    }
}
type ShmCapId = u64;
/// 0 = number of 1 GiB caps, 1 = number of 2 MiB caps, 2 = number of 4 KiB caps
type Sv39SpaceStats = [u32; 3];

pub struct ShmSpace {
    id_pool: ReusableIdPoolManual,
    space: BTreeMap<ShmCapId, ShmCap>,
    stats: Sv39SpaceStats,
}

impl ShmSpace {
    pub fn new() -> Self {
        ShmSpace {
            id_pool: ReusableIdPoolManual::new(),
            space: BTreeMap::new(),
            stats: [0; 3],
        }
    }

    // TODO: Probably should add shm_resize. And the validation that has for length should be consistent with here.
    pub fn new_shm_cap(&mut self, shm_type: ShmType, length: u64) -> Result<(ShmCapId, &ShmCap), ShmSpaceError> {
        if length == 0 {
            return InvalidLengthSnafu.fail();
        }

        if length > self.sv39_available_caps(shm_type).into() {
            return CapacityNotAvailableSnafu.fail();
        }

        // Since we have got past the sv39_available_caps check and it returns a
        // u32, we now know length is < 2^32.
        let sv39_length = length as u32;

        let id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyConcurrentIDs => ExhaustedSnafu.build() })?;

        let shm_cap = match self.space.entry(id) {
            Entry::Occupied(_) => return DuplicateIdSnafu.fail(),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(ShmCap::new(shm_type, length)),
        };

        Self::sv39_increment_stats(&mut self.stats, shm_type, sv39_length);

        Ok((id, shm_cap))
    }

    pub fn destroy_shm_cap(&mut self, shm_cap_id: ShmCapId) {
        // TODO: There needs to be checks here that the SHM has been released, etc.
        let shm_cap = self.space.remove(&shm_cap_id);
        if let Some(shm_cap) = shm_cap {
            Self::sv39_decrement_stats(&mut self.stats, shm_cap);
        }
    }

    /// This assumes that all caps can be arranged as: all 1 GiB ones first,
    /// then all 2 MiB ones, then all 4 KiB ones. This is probably a very dumb
    /// assumption and I might regret it later.
    fn sv39_available_caps(&self, shm_type: ShmType) -> u32 {
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

    /// sv39_available_caps(...) MUST be checked before calling this, otherwise
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
            ShmType::FourKiB => stats[2] -= shm_cap.length as u32,
            ShmType::TwoMiB => stats[1] -= shm_cap.length as u32,
            ShmType::OneGiB => stats[0] -= shm_cap.length as u32,
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
}

fn set_error<R: Register>(pcb: &mut ProcessControlBlock<R>, error: SyscallError) {
    pcb.set_register(RETURN_VAL_REGISTER, R::from_u64(u64::MAX));
    pcb.set_register(ERROR_RETURN_VAL_REGISTER, R::from_u64(error.into()));
}

fn set_success<R: Register>(pcb: &mut ProcessControlBlock<R>, return_value: u64) {
    pcb.set_register(RETURN_VAL_REGISTER, R::from_u64(return_value));
    pcb.set_register(ERROR_RETURN_VAL_REGISTER, R::from_u64(u64::MAX));
}

pub struct NushiftSubsystem {
    shm_space: ShmSpace,
}

impl NushiftSubsystem {
    pub fn new() -> Self {
        NushiftSubsystem { shm_space: ShmSpace::new() }
    }

    pub fn ecall<R: Register>(pcb: &mut ProcessControlBlock<R>) -> Result<(), CKBVMError> {
        // TODO: When 32-bit apps are supported, convert into u64 from multiple
        // registers, instead of `.to_u64()` which can only act on a single
        // register here)
        let syscall = Syscall::try_from(pcb.registers()[SYSCALL_NUM_REGISTER].to_u64());

        match syscall {
            Err(_) => {
                set_error(pcb, SyscallError::UnknownSyscall);
                Ok(())
            },

            Ok(Syscall::Exit) => {
                pcb.user_exit(pcb.registers()[FIRST_ARG_REGISTER].to_u64());
                set_success(pcb, 0);
                Ok(())
            },
            Ok(Syscall::ShmNew) => {
                let shm_type = match ShmType::try_from(pcb.registers()[FIRST_ARG_REGISTER].to_u64()) {
                    Ok(shm_type) => shm_type,
                    Err(_) => { set_error(pcb, SyscallError::ShmUnknownShmType); return Ok(()); },
                };
                let length = pcb.registers()[SECOND_ARG_REGISTER].to_u64();

                let shm_cap_id = match pcb.subsystem.shm_space.new_shm_cap(shm_type, length) {
                    Ok((shm_cap_id, _)) => shm_cap_id,
                    Err(shm_space_error) => match shm_space_error {
                        ShmSpaceError::DuplicateId => { set_error(pcb, SyscallError::ShmDuplicateId); return Ok(()); },
                        ShmSpaceError::Exhausted => { set_error(pcb, SyscallError::ShmExhausted); return Ok(()); },
                        ShmSpaceError::InvalidLength => { set_error(pcb, SyscallError::ShmInvalidLength); return Ok(()); },
                        ShmSpaceError::CapacityNotAvailable => { set_error(pcb, SyscallError::ShmCapacityNotAvailable); return Ok(()); },
                    },
                };

                set_success(pcb, shm_cap_id);
                Ok(())
            },
            Ok(Syscall::ShmDestroy) => {
                let shm_cap_id = pcb.registers()[FIRST_ARG_REGISTER].to_u64();
                pcb.subsystem.shm_space.destroy_shm_cap(shm_cap_id);
                set_success(pcb, 0);
                Ok(())
            },

            _ => {
                // I don't think I should return an unimplemented syscall
                // SyscallError here. I think I should implement the
                // not-that-many-remaining syscalls. And then remove this match
                // arm.
                todo!("Unimplemented syscall")
            },
        }
    }

    pub fn ebreak<R: Register>(_pcb: &mut ProcessControlBlock<R>) -> Result<(), CKBVMError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shm_space_sv39_available_caps_none_used() {
        let shm_space = ShmSpace::new();

        assert_eq!(512, shm_space.sv39_available_caps(ShmType::OneGiB));
        assert_eq!(1 << (SV39_BITS - 21), shm_space.sv39_available_caps(ShmType::TwoMiB));
        assert_eq!(1 << (SV39_BITS - 12), shm_space.sv39_available_caps(ShmType::FourKiB));
    }

    #[test]
    fn shm_space_sv39_available_caps_one_gibs_used() {
        let mut shm_space = ShmSpace::new();
        shm_space.stats = [3, 0, 0];

        assert_eq!(509, shm_space.sv39_available_caps(ShmType::OneGiB));
        assert_eq!((1 << (SV39_BITS - 21)) - (3 << 9), shm_space.sv39_available_caps(ShmType::TwoMiB));
        assert_eq!((1 << (SV39_BITS - 12)) - (3 << 18), shm_space.sv39_available_caps(ShmType::FourKiB));
    }

    #[test]
    fn shm_space_sv39_available_caps_all_types_used() {
        let mut shm_space = ShmSpace::new();

        // Make a layout where a 1 GiB slot isn't completely used by 2 MiB caps,
        // and then 4 KiB caps fill the remainder and go over to the next space.
        shm_space.stats = [3, 511, 513];

        assert_eq!(507, shm_space.sv39_available_caps(ShmType::OneGiB));
        assert_eq!((1 << (SV39_BITS - 21)) - (4 << 9) - 1, shm_space.sv39_available_caps(ShmType::TwoMiB));
        assert_eq!((1 << (SV39_BITS - 12)) - (4 << 18) - 1, shm_space.sv39_available_caps(ShmType::FourKiB));

        // Same, but 4 KiB caps fill exactly the remainder.
        shm_space.stats = [3, 511, 512];

        assert_eq!(508, shm_space.sv39_available_caps(ShmType::OneGiB));
        assert_eq!((1 << (SV39_BITS - 21)) - (4 << 9), shm_space.sv39_available_caps(ShmType::TwoMiB));
        assert_eq!((1 << (SV39_BITS - 12)) - (4 << 18), shm_space.sv39_available_caps(ShmType::FourKiB));

        // Same, but 4 KiB caps fill almost the remainder except one.
        shm_space.stats = [3, 511, 511];

        assert_eq!(508, shm_space.sv39_available_caps(ShmType::OneGiB));
        assert_eq!((1 << (SV39_BITS - 21)) - (4 << 9), shm_space.sv39_available_caps(ShmType::TwoMiB));
        assert_eq!((1 << (SV39_BITS - 12)) - (4 << 18) + 1, shm_space.sv39_available_caps(ShmType::FourKiB));
    }
}
