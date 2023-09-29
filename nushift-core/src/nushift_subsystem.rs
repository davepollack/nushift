use std::collections::HashSet;
use std::sync::{Arc, Mutex, Condvar};

use ckb_vm::Register;
use num_enum::{TryFromPrimitive, IntoPrimitive};

use super::deferred_space::app_global_deferred_space::{AppGlobalDeferredSpace, AppGlobalDeferredSpaceError, Task, TaskId};
use super::deferred_space::DeferredSpaceError;
use super::accessibility_tree_space::AccessibilityTreeSpace;
use super::hypervisor::hypervisor_event::BoundHypervisorEventHandler;
use super::register_ipc::{SyscallEnter, SyscallReturn, SYSCALL_NUM_REGISTER_INDEX, FIRST_ARG_REGISTER_INDEX, SECOND_ARG_REGISTER_INDEX, THIRD_ARG_REGISTER_INDEX};
use super::shm_space::{CapType, ShmType, ShmSpace, ShmSpaceError};
use super::title_space::TitleSpace;

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
enum Syscall {
    Exit = 0,

    ShmNew = 1,
    ShmAcquire = 2,
    ShmNewAndAcquire = 3,
    ShmRelease = 4,
    ShmDestroy = 5,
    ShmReleaseAndDestroy = 6,

    AccessibilityTreeNew = 7,
    AccessibilityTreePublish = 8,
    AccessibilityTreeDestroy = 9,

    TitleNew = 10,
    TitlePublish = 11,
    TitleDestroy = 12,

    BlockOnDeferredTasks = 13,
}

#[derive(IntoPrimitive)]
#[repr(u64)]
pub enum SyscallError {
    UnknownSyscall = 0,

    InternalError = 1, // Should never happen, and indicates a bug in Nushift's code.
    Exhausted = 2,
    CapNotFound = 6,
    InProgress = 11,
    PermissionDenied = 12,

    ShmUnknownShmType = 3,
    ShmInvalidLength = 4,
    ShmCapacityNotAvailable = 5,
    ShmCapCurrentlyAcquired = 7,
    ShmAddressOutOfBounds = 8,
    ShmAddressNotAligned = 9,
    ShmOverlapsExistingAcquisition = 10,

    DeferredDeserializeTaskIdsError = 13,
    DeferredDuplicateTaskIds = 14,
    DeferredTaskIdNotFound = 15,
}

fn set_error<R: Register>(error: SyscallError) -> SyscallReturn<R> {
    SyscallReturn::new_return(R::from_u64(u64::MAX), R::from_u64(error.into()))
}

fn set_success<R: Register>(return_value: u64) -> SyscallReturn<R> {
    SyscallReturn::new_return(R::from_u64(return_value), R::from_u64(u64::MAX))
}

fn user_exit<R>(exit_reason: u64) -> SyscallReturn<R> {
    SyscallReturn::UserExit { exit_reason }
}

fn marshall_shm_space_error<R: Register>(shm_space_error: ShmSpaceError) -> SyscallReturn<R> {
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

fn marshall_deferred_space_error<R: Register>(deferred_space_error: DeferredSpaceError) -> SyscallReturn<R> {
    match deferred_space_error {
        DeferredSpaceError::DuplicateId
        | DeferredSpaceError::ShmSpaceInternalError { .. }
        | DeferredSpaceError::PublishInternalError => set_error(SyscallError::InternalError),
        DeferredSpaceError::Exhausted { .. } => set_error(SyscallError::Exhausted),
        DeferredSpaceError::CapNotFound { .. }
        | DeferredSpaceError::ShmCapNotFound { .. } => set_error(SyscallError::CapNotFound),
        DeferredSpaceError::InProgress { .. } => set_error(SyscallError::InProgress),
        DeferredSpaceError::ShmPermissionDenied { .. } => set_error(SyscallError::PermissionDenied),
    }
}

fn marshall_app_global_deferred_space_error<R: Register>(app_global_deferred_space_error: AppGlobalDeferredSpaceError) -> SyscallReturn<R> {
    match app_global_deferred_space_error {
        AppGlobalDeferredSpaceError::DuplicateId
        | AppGlobalDeferredSpaceError::ShmUnexpectedError => set_error(SyscallError::InternalError),
        AppGlobalDeferredSpaceError::Exhausted => set_error(SyscallError::Exhausted),
        AppGlobalDeferredSpaceError::DeserializeTaskIdsError { .. } => set_error(SyscallError::DeferredDeserializeTaskIdsError),
        AppGlobalDeferredSpaceError::Duplicates { .. } => set_error(SyscallError::DeferredDuplicateTaskIds),
        AppGlobalDeferredSpaceError::NotFound { .. } => set_error(SyscallError::DeferredTaskIdNotFound),
        AppGlobalDeferredSpaceError::ShmCapNotFound { .. } => set_error(SyscallError::CapNotFound),
        AppGlobalDeferredSpaceError::ShmPermissionDenied { .. } => set_error(SyscallError::PermissionDenied),
    }
}

pub type BlockingOnTasksCondvar = Arc<(Mutex<HashSet<TaskId>>, Condvar)>;

pub struct NushiftSubsystem {
    pub(crate) shm_space: ShmSpace,
    pub(crate) accessibility_tree_space: AccessibilityTreeSpace,
    pub(crate) title_space: TitleSpace,
    pub(crate) app_global_deferred_space: AppGlobalDeferredSpace,
    pub(crate) blocking_on_tasks: BlockingOnTasksCondvar,
}

impl NushiftSubsystem {
    pub(crate) fn new(bound_hypervisor_event_handler: BoundHypervisorEventHandler, blocking_on_tasks: BlockingOnTasksCondvar) -> Self {
        NushiftSubsystem {
            shm_space: ShmSpace::new(),
            accessibility_tree_space: AccessibilityTreeSpace::new(),
            title_space: TitleSpace::new(bound_hypervisor_event_handler),
            app_global_deferred_space: AppGlobalDeferredSpace::new(),
            blocking_on_tasks,
        }
    }

    pub(crate) fn shm_space(&self) -> &ShmSpace {
        &self.shm_space
    }

    pub(crate) fn shm_space_mut(&mut self) -> &mut ShmSpace {
        &mut self.shm_space
    }

    pub fn ecall<R: Register>(&mut self, registers: SyscallEnter<R>) -> SyscallReturn<R> {
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

                match self.shm_space_mut().acquire_shm_cap_user(shm_cap_id, address) {
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

                match self.shm_space_mut().acquire_shm_cap_user(shm_cap_id, address) {
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

                match self.shm_space_mut().release_shm_cap_user(shm_cap_id) {
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

                match self.shm_space_mut().release_shm_cap_user(shm_cap_id) {
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

            Ok(Syscall::AccessibilityTreeNew) => {
                let accessibility_tree_cap_id = match self.accessibility_tree_space.new_accessibility_tree_cap() {
                    Ok(accessibility_tree_cap_id) => accessibility_tree_cap_id,
                    Err(deferred_space_error) => return marshall_deferred_space_error(deferred_space_error),
                };

                set_success(accessibility_tree_cap_id)
            },
            Ok(Syscall::AccessibilityTreePublish) => {
                let accessibility_tree_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();
                let input_shm_cap_id = registers[SECOND_ARG_REGISTER_INDEX].to_u64();
                let output_shm_cap_id = registers[THIRD_ARG_REGISTER_INDEX].to_u64();

                let mut task = match self.app_global_deferred_space.allocate_task(Task::AccessibilityTreePublish { accessibility_tree_cap_id }) {
                    Ok(task) => task,
                    Err(app_global_deferred_space_error) => return marshall_app_global_deferred_space_error(app_global_deferred_space_error),
                };

                match self.accessibility_tree_space.publish_accessibility_tree_blocking(accessibility_tree_cap_id, input_shm_cap_id, output_shm_cap_id, &mut self.shm_space) {
                    Ok(_) => {},
                    Err(deferred_space_error) => return marshall_deferred_space_error(deferred_space_error),
                };

                let task_id = task.push_task();

                set_success(task_id)
            },
            Ok(Syscall::AccessibilityTreeDestroy) => {
                let accessibility_tree_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();

                match self.accessibility_tree_space.destroy_accessibility_tree_cap(accessibility_tree_cap_id) {
                    Ok(_) => {},
                    Err(deferred_space_error) => return marshall_deferred_space_error(deferred_space_error),
                };

                set_success(0)
            },

            Ok(Syscall::TitleNew) => {
                let title_cap_id = match self.title_space.new_title_cap() {
                    Ok(title_cap_id) => title_cap_id,
                    Err(deferred_space_error) => return marshall_deferred_space_error(deferred_space_error),
                };

                set_success(title_cap_id)
            },
            Ok(Syscall::TitlePublish) => {
                let title_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();
                let input_shm_cap_id = registers[SECOND_ARG_REGISTER_INDEX].to_u64();
                let output_shm_cap_id = registers[THIRD_ARG_REGISTER_INDEX].to_u64();

                let mut task = match self.app_global_deferred_space.allocate_task(Task::TitlePublish { title_cap_id }) {
                    Ok(task) => task,
                    Err(app_global_deferred_space_error) => return marshall_app_global_deferred_space_error(app_global_deferred_space_error),
                };

                match self.title_space.publish_title_blocking(title_cap_id, input_shm_cap_id, output_shm_cap_id, &mut self.shm_space) {
                    Ok(_) => {},
                    Err(deferred_space_error) => return marshall_deferred_space_error(deferred_space_error),
                };

                let task_id = task.push_task();

                set_success(task_id)
            },
            Ok(Syscall::TitleDestroy) => {
                let title_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();

                match self.title_space.destroy_title_cap(title_cap_id) {
                    Ok(_) => {},
                    Err(deferred_space_error) => return marshall_deferred_space_error(deferred_space_error),
                };

                set_success(0)
            },

            Ok(Syscall::BlockOnDeferredTasks) => {
                let input_shm_cap_id = registers[FIRST_ARG_REGISTER_INDEX].to_u64();

                match self.app_global_deferred_space.block_on_deferred_tasks(input_shm_cap_id, &mut self.shm_space, &self.blocking_on_tasks) {
                    Ok(_) => {},
                    Err(app_global_deferred_space_error) => return marshall_app_global_deferred_space_error(app_global_deferred_space_error),
                };

                set_success(0)
            },
        }
    }
}
