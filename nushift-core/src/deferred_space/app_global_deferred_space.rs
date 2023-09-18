use std::collections::{HashMap, hash_map::{Entry, VacantEntry}};

use reusable_id_pool::{ReusableIdPoolManual, ReusableIdPoolError};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use crate::accessibility_tree_space::AccessibilityTreeCapId;
use crate::title_space::TitleCapId;

pub type TaskId = u64;

pub enum Task {
    AccessibilityTreePublish { accessibility_tree_cap_id: AccessibilityTreeCapId },
    TitlePublish { title_cap_id: TitleCapId },
}

pub struct AppGlobalDeferredSpace {
    id_pool: ReusableIdPoolManual,
    space: HashMap<TaskId, Task>,
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

    pub fn drain_tasks(&mut self) -> Vec<Task> {
        self.space.drain()
            .map(|(task_id, task)| {
                self.id_pool.release(task_id);
                task
            })
            .collect()
    }
}

pub struct TaskAllocation<'space> {
    task_id: TaskId,
    vacant_entry_and_task: Option<(VacantEntry<'space, TaskId, Task>, Task)>,
    id_pool: &'space mut ReusableIdPoolManual,
}

impl<'space> TaskAllocation<'space> {
    fn new(task_id: TaskId, task: Task, vacant_entry: VacantEntry<'space, TaskId, Task>, id_pool: &'space mut ReusableIdPoolManual) -> TaskAllocation<'space> {
        Self { task_id, vacant_entry_and_task: Some((vacant_entry, task)), id_pool }
    }

    /// This is not intended to be called more than once. It does nothing if it
    /// is. We can't make the signature `mut self` to enforce this because this
    /// has a Drop impl.
    pub fn push_task(&mut self) -> TaskId {
        match self.vacant_entry_and_task.take() {
            Some(vacant_entry_and_task) => {
                vacant_entry_and_task.0.insert(vacant_entry_and_task.1);
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
    fn drain_tasks() {
        let mut space = AppGlobalDeferredSpace::new();

        {
            let mut task = space.allocate_task(Task::TitlePublish { title_cap_id: 0 }).expect("Should work");
            task.push_task();
        }

        let tasks = space.drain_tasks();
        assert_eq!(1, tasks.len());

        // The ID should be released!
        assert_eq!(0, space.id_pool.allocate()); // Observe pool by doing another allocation
        space.id_pool.release(0);
        assert_eq!(0, space.space.len());
    }
}
