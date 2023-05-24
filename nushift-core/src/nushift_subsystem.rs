use ckb_vm::{Error as CKBVMError, registers::{A0, A1, T0}, Register, CoreMachine};
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
}

const SYSCALL_NUM_REGISTER: usize = A0;
const FIRST_ARG_REGISTER: usize = A1;
const RETURN_VAL_REGISTER: usize = A0;
// a1 is used by the RISC-V calling conventions for a second return value,
// rather than t0, but my concern is with the whole 32-bit app thing using
// multiple registers to encode a 64-bit value. Maybe the 32-bit ABI will just
// use a0 and a2 and the 64-bit will use a0 and a1. For now, using t0.
const ERROR_RETURN_VAL_REGISTER: usize = T0;

#[derive(TryFromPrimitive)]
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

pub struct ShmCap {
    shm_type: ShmType,
}

impl ShmCap {
    fn new(shm_type: ShmType) -> Self {
        ShmCap { shm_type }
    }
}

type ShmCapId = u64;
pub struct ShmSpace {
    id_pool: ReusableIdPoolManual,
    space: BTreeMap<ShmCapId, ShmCap>,
}

impl ShmSpace {
    pub fn new() -> Self {
        ShmSpace {
            id_pool: ReusableIdPoolManual::new(),
            space: BTreeMap::new(),
        }
    }

    pub fn new_shm_cap(&mut self, shm_type: ShmType) -> Result<(ShmCapId, &ShmCap), ShmSpaceError> {
        let id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyConcurrentIDs => ExhaustedSnafu.build() })?;

        let entry = self.space.entry(id);
        let shm_cap = match entry {
            Entry::Occupied(_) => return DuplicateIdSnafu.fail(),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(ShmCap::new(shm_type)),
        };

        Ok((id, shm_cap))
    }

    pub fn destroy_shm_cap(&mut self, shm_cap_id: ShmCapId) {
        // TODO: There needs to be checks here that the SHM has been released, etc.
        self.space.remove(&shm_cap_id);
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum ShmSpaceError {
    #[snafu(display("The new pool ID was already present in the space. This should never happen, and indicates a bug in Nushift's code."))]
    DuplicateId,
    #[snafu(display("The maximum amount of SHM capabilities have been used for this app. Please destroy some capabilities."))]
    Exhausted,
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

                let shm_cap_id = match pcb.subsystem.shm_space.new_shm_cap(shm_type) {
                    Ok((shm_cap_id, _)) => shm_cap_id,
                    Err(shm_space_error) => match shm_space_error {
                        ShmSpaceError::DuplicateId => { set_error(pcb, SyscallError::ShmDuplicateId); return Ok(()); },
                        ShmSpaceError::Exhausted => { set_error(pcb, SyscallError::ShmExhausted); return Ok(()); },
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
                // Return immediately, because these system calls should be
                // asynchronous.

                // TODO: Actually queue something, though.
                Ok(())
            },
        }
    }

    pub fn ebreak<R: Register>(_pcb: &mut ProcessControlBlock<R>) -> Result<(), CKBVMError> {
        todo!()
    }
}
