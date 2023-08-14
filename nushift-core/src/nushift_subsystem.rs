use ckb_vm::Register;
use num_enum::{TryFromPrimitive, IntoPrimitive};

use super::accessibility_tree_space::{AccessibilityTreeSpace, AccessibilityTreeSpaceError, AccessibilityTreeCapId};
use super::shm_space::{CapType, ShmType, ShmSpace, ShmSpaceError};
use super::register_ipc::{SyscallEnter, SyscallReturn, SYSCALL_NUM_REGISTER_INDEX, FIRST_ARG_REGISTER_INDEX, SECOND_ARG_REGISTER_INDEX, THIRD_ARG_REGISTER_INDEX};

// Regarding the use of `u64`s in this file:
//
// It is intended that riscv32 should use the same numbers. I don't want to have
// duplicate enums/duplicate code for riscv32 apps.
//
// My idea is that riscv32 apps can encode the numbers in a multi-register
// encoding. But at the same time, I don't want that to use as many as 3
// registers, so these numbers should be u63, not u64.
//
// (Technically, 2 registers can encode slightly more than a u63, but a u63
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

    AccessibilityTreeNewCap = 7,
    AccessibilityTreeDestroyCap = 8,
    AccessibilityTreePublish = 9,
}

#[derive(IntoPrimitive)]
#[repr(u64)]
#[non_exhaustive]
pub enum SyscallError {
    UnknownSyscall = 0,

    InternalError = 1, // Should never happen, and indicates a bug in Nushift's code.
    Exhausted = 2,
    CapNotFound = 6,
    PermissionDenied = 12,

    ShmUnknownShmType = 3,
    ShmInvalidLength = 4,
    ShmCapacityNotAvailable = 5,
    ShmCapCurrentlyAcquired = 7,
    ShmAddressOutOfBounds = 8,
    ShmAddressNotAligned = 9,
    ShmOverlapsExistingAcquisition = 10,

    AccessibilityTreeInProgress = 11,
}

trait AsOption<T> {
    fn as_option(self) -> Option<T>;
}
impl<T> AsOption<T> for T {
    fn as_option(self) -> Option<T> {
        Some(self)
    }
}
impl<T> AsOption<T> for Option<T> {
    fn as_option(self) -> Option<T> {
        self
    }
}

pub struct SyscallReturnAndTask<R>(pub SyscallReturn<R>, pub Option<Task>);
pub enum Task {
    AccessibilityTreePublish { accessibility_tree_cap_id: AccessibilityTreeCapId },
}

fn set_error<R: Register>(error: SyscallError) -> SyscallReturnAndTask<R> {
    SyscallReturnAndTask(SyscallReturn::new_return(R::from_u64(u64::MAX), R::from_u64(error.into())), None)
}

fn set_success<R: Register>(return_value: u64) -> SyscallReturnAndTask<R> {
    set_success_with_task(return_value, None)
}

fn set_success_with_task<R, O>(return_value: u64, task: O) -> SyscallReturnAndTask<R>
where
    R: Register,
    O: AsOption<Task>,
{
    SyscallReturnAndTask(SyscallReturn::new_return(R::from_u64(return_value), R::from_u64(u64::MAX)), task.as_option())
}

fn user_exit<R>(exit_reason: u64) -> SyscallReturnAndTask<R> {
    SyscallReturnAndTask(SyscallReturn::UserExit { exit_reason }, None)
}

fn marshall_shm_space_error<R: Register>(shm_space_error: ShmSpaceError) -> SyscallReturnAndTask<R> {
    match shm_space_error {
        ShmSpaceError::DuplicateId
        | ShmSpaceError::AcquireReleaseInternalError => set_error(SyscallError::InternalError),
        ShmSpaceError::Exhausted => set_error(SyscallError::Exhausted),
        ShmSpaceError::InvalidLength => set_error(SyscallError::ShmInvalidLength),
        ShmSpaceError::CapacityNotAvailable
        | ShmSpaceError::BackingCapacityNotAvailable { .. }
        | ShmSpaceError::BackingCapacityNotAvailableOverflows => set_error(SyscallError::ShmCapacityNotAvailable),
        ShmSpaceError::CurrentlyAcquiredCap { .. }
        | ShmSpaceError::DestroyingCurrentlyAcquiredCap { .. } => set_error(SyscallError::ShmCapCurrentlyAcquired),
        ShmSpaceError::CapNotFound => set_error(SyscallError::CapNotFound),
        ShmSpaceError::AddressOutOfBounds => set_error(SyscallError::ShmAddressOutOfBounds),
        ShmSpaceError::AddressNotAligned => set_error(SyscallError::ShmAddressNotAligned),
        ShmSpaceError::OverlapsExistingAcquisition => set_error(SyscallError::ShmOverlapsExistingAcquisition),
        ShmSpaceError::PermissionDenied => set_error(SyscallError::PermissionDenied),
    }
}

fn marshall_accessibility_tree_space_error<R: Register>(accessibility_tree_space_error: AccessibilityTreeSpaceError) -> SyscallReturnAndTask<R> {
    match accessibility_tree_space_error {
        AccessibilityTreeSpaceError::DuplicateId
        | AccessibilityTreeSpaceError::ShmSpaceInternalError { .. }
        | AccessibilityTreeSpaceError::PublishInternalError => set_error(SyscallError::InternalError),
        AccessibilityTreeSpaceError::Exhausted
        | AccessibilityTreeSpaceError::ShmExhausted => set_error(SyscallError::Exhausted),
        AccessibilityTreeSpaceError::CapNotFound { .. }
        | AccessibilityTreeSpaceError::ShmCapNotFound { .. } => set_error(SyscallError::CapNotFound),
        AccessibilityTreeSpaceError::InProgress => set_error(SyscallError::AccessibilityTreeInProgress),
        AccessibilityTreeSpaceError::ShmPermissionDenied { .. } => set_error(SyscallError::PermissionDenied),
        AccessibilityTreeSpaceError::ShmCapacityNotAvailable => set_error(SyscallError::ShmCapacityNotAvailable),
    }
}

pub struct NushiftSubsystem {
    pub(crate) shm_space: ShmSpace,
    pub(crate) accessibility_tree_space: AccessibilityTreeSpace,
}

impl NushiftSubsystem {
    pub fn new() -> Self {
        NushiftSubsystem { shm_space: ShmSpace::new(), accessibility_tree_space: AccessibilityTreeSpace::new() }
    }

    pub(crate) fn shm_space(&self) -> &ShmSpace {
        &self.shm_space
    }

    pub(crate) fn shm_space_mut(&mut self) -> &mut ShmSpace {
        &mut self.shm_space
    }

    pub fn ecall<R: Register>(&mut self, registers: SyscallEnter<R>) -> SyscallReturnAndTask<R> {
        // TODO: When 32-bit apps are supported, convert into u64 from multiple
        // registers, instead of `.to_u64()` which can only act on a single
        // register here)
        let syscall = Syscall::try_from(registers[SYSCALL_NUM_REGISTER_INDEX].to_u64());

        match syscall {
            Err(_) => {
                set_error(SyscallError::UnknownSyscall)
            },

            Ok(Syscall::Exit) => {
                user_exit(registers[FIRST_ARG_REGISTER_INDEX].to_u64())
            },
            Ok(Syscall::ShmNew) => {
                let shm_type = match ShmType::try_from(registers[FIRST_ARG_REGISTER_INDEX].to_u64()) {
                    Ok(shm_type) => shm_type,
                    Err(_) => return set_error(SyscallError::ShmUnknownShmType),
                };
                let length = registers[SECOND_ARG_REGISTER_INDEX].to_u64();

                let shm_cap_id = match self.shm_space_mut().new_shm_cap(shm_type, length, CapType::UserCap) {
                    Ok((shm_cap_id, _)) => shm_cap_id,
                    Err(shm_space_error) => return marshall_shm_space_error(shm_space_error),
                };

                set_success(shm_cap_id)
            },
            Ok(Syscall::ShmAcquire) => {
                let shm_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();
                let address = registers[SECOND_ARG_REGISTER_INDEX].to_u64();

                match self.shm_space_mut().acquire_shm_cap(shm_cap_id, address) {
                    Ok(_) => {},
                    Err(shm_space_error) => return marshall_shm_space_error(shm_space_error),
                };

                set_success(0)
            },
            Ok(Syscall::ShmNewAndAcquire) => {
                let shm_type = match ShmType::try_from(registers[FIRST_ARG_REGISTER_INDEX].to_u64()) {
                    Ok(shm_type) => shm_type,
                    Err(_) => return set_error(SyscallError::ShmUnknownShmType),
                };
                let length = registers[SECOND_ARG_REGISTER_INDEX].to_u64();
                let address = registers[THIRD_ARG_REGISTER_INDEX].to_u64();

                let shm_cap_id = match self.shm_space_mut().new_shm_cap(shm_type, length, CapType::UserCap) {
                    Ok((shm_cap_id, _)) => shm_cap_id,
                    Err(shm_space_error) => return marshall_shm_space_error(shm_space_error),
                };

                match self.shm_space_mut().acquire_shm_cap(shm_cap_id, address) {
                    Ok(_) => {},
                    Err(shm_space_error) => {
                        // If an acquire error occurs, roll back the just-created cap.
                        let shm_space_error = self.shm_space_mut().destroy_shm_cap(shm_cap_id, CapType::UserCap)
                            .map_err(|_| ShmSpaceError::AcquireReleaseInternalError)
                            // If error occurred in destroy, use that (now mapped to internal) error. Otherwise, use the original shm_space_error.
                            .map_or_else(|err| err, |_| shm_space_error);

                        return marshall_shm_space_error(shm_space_error);
                    },
                };

                set_success(shm_cap_id)
            },
            Ok(Syscall::ShmRelease) => {
                let shm_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();

                match self.shm_space_mut().release_shm_cap(shm_cap_id, CapType::UserCap) {
                    Ok(_) => {},
                    Err(shm_space_error) => return marshall_shm_space_error(shm_space_error),
                };

                set_success(0)
            }
            Ok(Syscall::ShmDestroy) => {
                let shm_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();

                match self.shm_space_mut().destroy_shm_cap(shm_cap_id, CapType::UserCap) {
                    Ok(_) => {},
                    Err(shm_space_error) => return marshall_shm_space_error(shm_space_error),
                };

                set_success(0)
            },
            Ok(Syscall::ShmReleaseAndDestroy) => {
                let shm_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();

                match self.shm_space_mut().release_shm_cap(shm_cap_id, CapType::UserCap) {
                    Ok(_) => {},
                    Err(shm_space_error) => return marshall_shm_space_error(shm_space_error),
                };

                // If the release succeeded, destroy should never fail, thus do not rollback (re-acquire).
                match self.shm_space_mut().destroy_shm_cap(shm_cap_id, CapType::UserCap) {
                    Ok(_) => {},
                    Err(shm_space_error) => return marshall_shm_space_error(shm_space_error),
                };

                set_success(0)
            },

            Ok(Syscall::AccessibilityTreeNewCap) => {
                let accessibility_tree_cap_id = match self.accessibility_tree_space.new_accessibility_tree_cap() {
                    Ok(accessibility_tree_cap_id) => accessibility_tree_cap_id,
                    Err(accessibility_tree_space_error) => return marshall_accessibility_tree_space_error(accessibility_tree_space_error),
                };

                set_success(accessibility_tree_cap_id)
            },
            Ok(Syscall::AccessibilityTreeDestroyCap) => {
                let accessibility_tree_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();

                match self.accessibility_tree_space.destroy_accessibility_tree_cap(accessibility_tree_cap_id) {
                    Ok(_) => {},
                    Err(accessibility_tree_space_error) => return marshall_accessibility_tree_space_error(accessibility_tree_space_error),
                };

                set_success(0)
            },
            Ok(Syscall::AccessibilityTreePublish) => {
                let accessibility_tree_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();
                let input_shm_cap_id = registers[SECOND_ARG_REGISTER_INDEX].to_u64();

                match self.accessibility_tree_space.publish_accessibility_tree_blocking(accessibility_tree_cap_id, input_shm_cap_id, &mut self.shm_space) {
                    Ok(_) => {},
                    Err(accessibility_tree_space_error) => return marshall_accessibility_tree_space_error(accessibility_tree_space_error),
                };

                set_success_with_task(0, Task::AccessibilityTreePublish { accessibility_tree_cap_id })
            },
        }
    }
}
