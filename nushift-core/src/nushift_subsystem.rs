use ckb_vm::{Error as CKBVMError, registers::{A0, A1, A2, A3, T0}, Register, CoreMachine};
use num_enum::{TryFromPrimitive, IntoPrimitive};

use super::process_control_block::ProcessControlBlock;
use super::shm_space::{ShmType, ShmSpace, ShmSpaceError};

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
#[non_exhaustive]
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
#[non_exhaustive]
pub enum SyscallError {
    UnknownSyscall = 0,

    ShmInternalError = 1, // Should never happen, and indicates a bug in Nushift's code.
    ShmExhausted = 2,
    ShmUnknownShmType = 3,
    ShmInvalidLength = 4,
    ShmCapacityNotAvailable = 5,
    ShmCapNotFound = 6,
    ShmCapCurrentlyAcquired = 7,
    ShmAddressOutOfBounds = 8,
    ShmAddressNotAligned = 9,
    ShmOverlapsExistingAcquisition = 10,
}

const SYSCALL_NUM_REGISTER: usize = A0;
const FIRST_ARG_REGISTER: usize = A1;
const SECOND_ARG_REGISTER: usize = A2;
const THIRD_ARG_REGISTER: usize = A3;
const RETURN_VAL_REGISTER: usize = A0;
/// a1 is used by the RISC-V calling conventions for a second return value,
/// rather than t0, but my concern is with the whole 32-bit app thing using
/// multiple registers to encode a 64-bit value. Maybe the 32-bit ABI will just
/// use a0 and a2 and the 64-bit will use a0 and a1. For now, using t0.
const ERROR_RETURN_VAL_REGISTER: usize = T0;

fn set_error<R: Register>(pcb: &mut ProcessControlBlock<R>, error: SyscallError) {
    pcb.set_register(RETURN_VAL_REGISTER, R::from_u64(u64::MAX));
    pcb.set_register(ERROR_RETURN_VAL_REGISTER, R::from_u64(error.into()));
}

fn set_success<R: Register>(pcb: &mut ProcessControlBlock<R>, return_value: u64) {
    pcb.set_register(RETURN_VAL_REGISTER, R::from_u64(return_value));
    pcb.set_register(ERROR_RETURN_VAL_REGISTER, R::from_u64(u64::MAX));
}

fn marshall_shm_space_error<R: Register>(pcb: &mut ProcessControlBlock<R>, shm_space_error: ShmSpaceError) {
    match shm_space_error {
        ShmSpaceError::DuplicateId
        | ShmSpaceError::AcquireReleaseInternalError => { set_error(pcb, SyscallError::ShmInternalError); },
        ShmSpaceError::Exhausted => { set_error(pcb, SyscallError::ShmExhausted); },
        ShmSpaceError::InvalidLength => { set_error(pcb, SyscallError::ShmInvalidLength); },
        ShmSpaceError::CapacityNotAvailable
        | ShmSpaceError::BackingCapacityNotAvailable { source: _ }
        | ShmSpaceError::BackingCapacityNotAvailableOverflows => { set_error(pcb, SyscallError::ShmCapacityNotAvailable); },
        ShmSpaceError::CurrentlyAcquiredCap { address: _ }
        | ShmSpaceError::DestroyingCurrentlyAcquiredCap { address: _ } => { set_error(pcb, SyscallError::ShmCapCurrentlyAcquired); },
        ShmSpaceError::CapNotFound => { set_error(pcb, SyscallError::ShmCapNotFound); },
        ShmSpaceError::AddressOutOfBounds => { set_error(pcb, SyscallError::ShmAddressOutOfBounds); },
        ShmSpaceError::AddressNotAligned => { set_error(pcb, SyscallError::ShmAddressNotAligned); },
        ShmSpaceError::OverlapsExistingAcquisition => { set_error(pcb, SyscallError::ShmOverlapsExistingAcquisition); },
    }
}

pub struct NushiftSubsystem {
    shm_space: ShmSpace,
}

impl NushiftSubsystem {
    pub fn new() -> Self {
        NushiftSubsystem { shm_space: ShmSpace::new() }
    }

    pub(crate) fn shm_space(&self) -> &ShmSpace {
        &self.shm_space
    }

    pub(crate) fn shm_space_mut(&mut self) -> &mut ShmSpace {
        &mut self.shm_space
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

                let shm_cap_id = match pcb.subsystem.shm_space_mut().new_shm_cap(shm_type, length) {
                    Ok((shm_cap_id, _)) => shm_cap_id,
                    Err(shm_space_error) => { marshall_shm_space_error(pcb, shm_space_error); return Ok(()); },
                };

                set_success(pcb, shm_cap_id);
                Ok(())
            },
            Ok(Syscall::ShmAcquire) => {
                let shm_cap_id = pcb.registers()[FIRST_ARG_REGISTER].to_u64();
                let address = pcb.registers()[SECOND_ARG_REGISTER].to_u64();

                match pcb.subsystem.shm_space_mut().acquire_shm_cap(shm_cap_id, address) {
                    Ok(_) => {},
                    Err(shm_space_error) => { marshall_shm_space_error(pcb, shm_space_error); return Ok(()); },
                };

                set_success(pcb, 0);
                Ok(())
            },
            Ok(Syscall::ShmNewAndAcquire) => {
                let shm_type = match ShmType::try_from(pcb.registers()[FIRST_ARG_REGISTER].to_u64()) {
                    Ok(shm_type) => shm_type,
                    Err(_) => { set_error(pcb, SyscallError::ShmUnknownShmType); return Ok(()); },
                };
                let length = pcb.registers()[SECOND_ARG_REGISTER].to_u64();
                let address = pcb.registers()[THIRD_ARG_REGISTER].to_u64();

                let shm_cap_id = match pcb.subsystem.shm_space_mut().new_shm_cap(shm_type, length) {
                    Ok((shm_cap_id, _)) => shm_cap_id,
                    Err(shm_space_error) => { marshall_shm_space_error(pcb, shm_space_error); return Ok(()); },
                };

                match pcb.subsystem.shm_space_mut().acquire_shm_cap(shm_cap_id, address) {
                    Ok(_) => {},
                    Err(shm_space_error) => { marshall_shm_space_error(pcb, shm_space_error); return Ok(()); },
                };

                set_success(pcb, 0);
                Ok(())
            },
            Ok(Syscall::ShmRelease) => {
                let shm_cap_id = pcb.registers()[FIRST_ARG_REGISTER].to_u64();

                match pcb.subsystem.shm_space_mut().release_shm_cap(shm_cap_id) {
                    Ok(_) => {},
                    Err(shm_space_error) => { marshall_shm_space_error(pcb, shm_space_error); return Ok(()); },
                };

                set_success(pcb, 0);
                Ok(())
            }
            Ok(Syscall::ShmDestroy) => {
                let shm_cap_id = pcb.registers()[FIRST_ARG_REGISTER].to_u64();

                match pcb.subsystem.shm_space_mut().destroy_shm_cap(shm_cap_id) {
                    Ok(_) => {},
                    Err(shm_space_error) => { marshall_shm_space_error(pcb, shm_space_error); return Ok(()); },
                };

                set_success(pcb, 0);
                Ok(())
            },
            Ok(Syscall::ShmReleaseAndDestroy) => {
                let shm_cap_id = pcb.registers()[FIRST_ARG_REGISTER].to_u64();

                match pcb.subsystem.shm_space_mut().release_shm_cap(shm_cap_id) {
                    Ok(_) => {},
                    Err(shm_space_error) => { marshall_shm_space_error(pcb, shm_space_error); return Ok(()); },
                };

                match pcb.subsystem.shm_space_mut().destroy_shm_cap(shm_cap_id) {
                    Ok(_) => {},
                    Err(shm_space_error) => { marshall_shm_space_error(pcb, shm_space_error); return Ok(()); },
                };

                set_success(pcb, 0);
                Ok(())
            },
        }
    }

    pub fn ebreak<R: Register>(_pcb: &mut ProcessControlBlock<R>) -> Result<(), CKBVMError> {
        // Terminate app.
        // TODO: As an improvement to terminating the app, provide debugging functionality.
        Err(CKBVMError::External(String::from("ebreak encountered; terminating app.")))
    }
}
