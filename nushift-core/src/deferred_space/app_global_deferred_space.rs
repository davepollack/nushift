use core::mem;
use std::collections::{HashMap, hash_map::{Entry, VacantEntry}};

use itertools::Itertools;
use postcard::Error as PostcardError;
use reusable_id_pool::{ReusableIdPoolManual, ReusableIdPoolError};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use crate::accessibility_tree_space::AccessibilityTreeCapId;
use crate::gfx_space::{GfxCapId, GfxCpuPresentBufferCapId};
use crate::nushift_subsystem::BlockingOnTasksCondvar;
use crate::shm_space::{ShmCapId, ShmSpace, ShmSpaceError};
use crate::title_space::TitleCapId;

pub type TaskId = u64;

#[derive(Debug, PartialEq, Eq)]
pub enum Task {
    AccessibilityTreePublish { accessibility_tree_cap_id: AccessibilityTreeCapId },
    TitlePublish { title_cap_id: TitleCapId },
    GfxGetOutputs { gfx_cap_id: GfxCapId },
    GfxCpuPresent { gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId },
}

enum ScheduledTask {
    Waiting(Task),
    Finished,
}

pub struct AppGlobalDeferredSpace {
    id_pool: ReusableIdPoolManual,
    space: HashMap<TaskId, ScheduledTask>,
}

impl AppGlobalDeferredSpace {
    pub fn new() -> Self {
        Self {
            id_pool: ReusableIdPoolManual::new(),
            space: HashMap::new(),
        }
    }

    pub fn allocate_task(&mut self, task: Task) -> Result<TaskAllocation<'_>, AppGlobalDeferredSpaceError> {
        let task_id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyLiveIDs => ExhaustedSnafu.build() })?;

        let vacant_entry = match self.space.entry(task_id) {
            Entry::Occupied(_) => return DuplicateIdSnafu.fail(),
            Entry::Vacant(vacant_entry) => vacant_entry,
        };

        Ok(TaskAllocation::new(task_id, task, vacant_entry, &mut self.id_pool))
    }

    pub fn finish_tasks(&mut self) -> Vec<(TaskId, Task)> {
        let mut tasks = vec![];
        for (task_id, scheduled_task) in self.space.iter_mut() {
            match mem::replace(scheduled_task, ScheduledTask::Finished) {
                ScheduledTask::Waiting(task) => tasks.push((*task_id, task)),
                _ => {},
            }
        }
        tasks
    }

    pub fn block_on_deferred_tasks(&mut self, input_shm_cap_id: ShmCapId, shm_space: &ShmSpace, blocking_on_tasks_condvar: &BlockingOnTasksCondvar) -> Result<(), AppGlobalDeferredSpaceError> {
        let input_shm_cap = shm_space.get_shm_cap_app(input_shm_cap_id).map_err(|shm_space_error| match shm_space_error {
            ShmSpaceError::CapNotFound => ShmCapNotFoundSnafu { id: input_shm_cap_id }.build(),
            ShmSpaceError::PermissionDenied => ShmPermissionDeniedSnafu { id: input_shm_cap_id }.build(),
            _ => ShmUnexpectedSnafu.build(),
        })?;

        let task_ids = postcard::from_bytes(input_shm_cap.backing()).context(DeserializeTaskIdsSnafu)?;
        self.validate_task_ids(&task_ids)?;

        // Consume tasks that are already finished even at this start point.
        let unfinished_task_ids = self.consume_finished_tasks(task_ids);

        if unfinished_task_ids.is_empty() {
            return Ok(());
        }

        // Wait on condvar for remaining tasks.
        let (lock, cvar) = &**blocking_on_tasks_condvar;
        let mut guard = lock.lock().unwrap();
        *guard = unfinished_task_ids.into_iter().collect();
        while !guard.is_empty() {
            guard = cvar.wait(guard).unwrap();
        }

        Ok(())
    }

    fn validate_task_ids(&self, task_ids: &Vec<TaskId>) -> Result<(), AppGlobalDeferredSpaceError> {
        // The validate method relies on the whole app being blocked making it
        // still valid once the deferred tasks are finished. Is this true?

        let duplicate_task_ids: Vec<TaskId> = task_ids.iter().map(|&task_id| task_id).duplicates().collect();
        if !duplicate_task_ids.is_empty() {
            return DuplicatesSnafu { duplicate_task_ids }.fail();
        }

        let not_found_task_ids: Vec<TaskId> = task_ids.iter()
            .filter_map(|&task_id| {
                if !self.space.contains_key(&task_id) { Some(task_id) } else { None }
            })
            .collect();
        if !not_found_task_ids.is_empty() {
            return NotFoundSnafu { not_found_task_ids }.fail();
        }

        Ok(())
    }

    fn consume_finished_tasks(&mut self, task_ids: Vec<TaskId>) -> Vec<TaskId> {
        let mut unfinished_task_ids = vec![];

        for task_id in task_ids {
            match self.space.entry(task_id) {
                Entry::Occupied(occupied_entry) if matches!(occupied_entry.get(), ScheduledTask::Finished) => {
                    occupied_entry.remove();
                    self.id_pool.release(task_id);
                },
                Entry::Occupied(_) => unfinished_task_ids.push(task_id),
                Entry::Vacant(_) => panic!("Vacant shouldn't be possible. The provided IDs should be validated before calling this function."),
            }
        }

        unfinished_task_ids
    }
}

/// A temporary view into a (vacant) entry of the AppGlobalDeferredSpace. If an
/// error in some other subsystem occurs before the task is committed, the
/// temporary ID allocation is rolled back.
pub struct TaskAllocation<'space> {
    task_id: TaskId,
    vacant_entry_and_task: Option<(VacantEntry<'space, TaskId, ScheduledTask>, Task)>,
    id_pool: &'space mut ReusableIdPoolManual,
}

impl<'space> TaskAllocation<'space> {
    fn new(task_id: TaskId, task: Task, vacant_entry: VacantEntry<'space, TaskId, ScheduledTask>, id_pool: &'space mut ReusableIdPoolManual) -> TaskAllocation<'space> {
        Self { task_id, vacant_entry_and_task: Some((vacant_entry, task)), id_pool }
    }

    /// Commits the task.
    ///
    /// This is not intended to be called more than once. It does nothing if it
    /// is. We can't make the signature `mut self` to enforce this because this
    /// has a Drop impl.
    pub fn push_task(&mut self) -> TaskId {
        match self.vacant_entry_and_task.take() {
            Some(vacant_entry_and_task) => {
                vacant_entry_and_task.0.insert(ScheduledTask::Waiting(vacant_entry_and_task.1));
            },
            None => {},
        }
        self.task_id
    }
}

impl Drop for TaskAllocation<'_> {
    fn drop(&mut self) {
        if self.vacant_entry_and_task.is_some() {
            // Rollback the ID allocation
            self.id_pool.release(self.task_id);
        }
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum AppGlobalDeferredSpaceError {
    #[snafu(display("The new pool ID was already present in the space. This should never happen, and indicates a bug in Nushift's code."))]
    DuplicateId,
    #[snafu(display("The maximum amount of deferred tasks have been reached. This is not a great situation, but maybe it's possible to wait for deferred tasks to finish."))]
    Exhausted,
    #[snafu(display("Error deserialising task IDs: {source}"))]
    DeserializeTaskIdsError { source: PostcardError },
    #[snafu(display("Duplicate task IDs were provided, duplicates are: {duplicate_task_ids:?}"))]
    Duplicates { duplicate_task_ids: Vec<TaskId> },
    #[snafu(display("Tasks with task IDs {not_found_task_ids:?} not found."))]
    NotFound { not_found_task_ids: Vec<TaskId> },
    #[snafu(display("The SHM cap with ID {id} was not found."))]
    ShmCapNotFound { id: ShmCapId },
    #[snafu(display("The SHM cap with ID {id} is not allowed to be used as an input cap, possibly because it is an ELF cap."))]
    ShmPermissionDenied { id: ShmCapId },
    ShmUnexpectedError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_task() {
        let mut space = AppGlobalDeferredSpace::new();

        {
            let _task = space.allocate_task(Task::TitlePublish { title_cap_id: 0 }).expect("Should work");
        }

        // After dropping an uncommitted TaskAllocation, the ID should be dropped from the ID pool.
        assert_eq!(0, space.id_pool.allocate()); // Observe pool by doing another allocation
        space.id_pool.release(0);
        assert_eq!(0, space.space.len());
    }

    #[test]
    fn allocate_task_and_push() {
        let mut space = AppGlobalDeferredSpace::new();

        {
            let mut task = space.allocate_task(Task::TitlePublish { title_cap_id: 0 }).expect("Should work");
            task.push_task();
        }

        assert_eq!(1, space.id_pool.allocate()); // Observe pool by doing another allocation
        space.id_pool.release(1);
        assert_eq!(1, space.space.len());
    }

    #[test]
    fn finish_tasks() {
        let mut space = AppGlobalDeferredSpace::new();

        {
            let mut task = space.allocate_task(Task::TitlePublish { title_cap_id: 0 }).expect("Should work");
            task.push_task();
        }

        // Should be 1 entry in space before finishing
        assert_eq!(1, space.space.len());

        // Finished tasks should be returned
        let tasks = space.finish_tasks();
        assert_eq!(vec![(0, Task::TitlePublish { title_cap_id: 0 })], tasks);

        // Should still be 1 entry in space
        assert_eq!(1, space.space.len());

        // All entries should now have the finished status
        assert!(space.space.values().all(|scheduled_task| matches!(scheduled_task, ScheduledTask::Finished)));
    }

    #[test]
    fn validate_task_ids() {
        let mut space = AppGlobalDeferredSpace::new();

        let task_id = {
            let mut task = space.allocate_task(Task::TitlePublish { title_cap_id: 0 }).expect("Should work");
            task.push_task();
            task.task_id
        };

        // Existent ID: valid
        assert!(matches!(
            space.validate_task_ids(&vec![task_id]),
            Ok(()),
        ));

        // Duplicate IDs: not valid
        assert!(matches!(
            space.validate_task_ids(&vec![task_id, task_id]),
            Err(AppGlobalDeferredSpaceError::Duplicates { duplicate_task_ids: m_duplicate_task_ids }) if m_duplicate_task_ids == vec![task_id],
        ));

        // Non-existent ID: not valid
        assert!(matches!(
            space.validate_task_ids(&vec![task_id + 1]),
            Err(AppGlobalDeferredSpaceError::NotFound { not_found_task_ids: m_not_found_task_ids }) if m_not_found_task_ids == vec![task_id + 1],
        ));
    }

    #[test]
    fn consume_finished_tasks() {
        let mut space = AppGlobalDeferredSpace::new();

        let task_id = {
            let mut task = space.allocate_task(Task::TitlePublish { title_cap_id: 0 }).expect("Should work");
            task.push_task();
            task.task_id
        };

        space.finish_tasks();
        space.consume_finished_tasks(vec![task_id]);

        // There should be no entries in the space
        assert_eq!(0, space.space.len());

        // The ID should be released!
        assert_eq!(0, space.id_pool.allocate()); // Observe pool by doing another allocation
        space.id_pool.release(0);
        assert_eq!(0, space.space.len());
    }
}
