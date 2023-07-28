use std::collections::{HashMap, hash_map::Entry};

use reusable_id_pool::{ReusableIdPoolManual, ReusableIdPoolError};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use super::shm_space::{OwnedShmIdAndCap, ShmSpace, ShmCapId, ShmSpaceError, ShmType};

struct InProgressCap {
    input: OwnedShmIdAndCap,
    output: OwnedShmIdAndCap,
}
impl InProgressCap {
    fn new(input: OwnedShmIdAndCap, output: OwnedShmIdAndCap) -> Self {
        Self { input, output }
    }
}

pub struct AccessibilityTreeCap {
    in_progress_cap: Option<InProgressCap>,
}
impl AccessibilityTreeCap {
    pub fn new() -> Self {
        Self { in_progress_cap: None }
    }
}

pub type AccessibilityTreeCapId = u64;

pub struct AccessibilityTreeSpace {
    id_pool: ReusableIdPoolManual,
    space: HashMap<AccessibilityTreeCapId, AccessibilityTreeCap>,
}

impl AccessibilityTreeSpace {
    pub fn new() -> Self {
        AccessibilityTreeSpace { id_pool: ReusableIdPoolManual::new(), space: HashMap::new() }
    }

    pub fn new_accessibility_tree_cap(&mut self) -> Result<AccessibilityTreeCapId, AccessibilityTreeSpaceError> {
        let id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyLiveIDs => ExhaustedSnafu.build() })?;

        match self.space.entry(id) {
            Entry::Occupied(_) => return DuplicateIdSnafu.fail(),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(AccessibilityTreeCap::new()),
        };

        Ok(id)
    }

    /// Releases SHM cap, but does not do further processing yet.
    pub fn publish_accessibility_tree_blocking(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId, input_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), AccessibilityTreeSpaceError> {
        let accessibility_tree_cap = self.space.get_mut(&accessibility_tree_cap_id).ok_or_else(|| CapNotFoundSnafu { id: accessibility_tree_cap_id }.build())?;

        shm_space.release_shm_cap(input_shm_cap_id).map_err(|shm_space_error| match shm_space_error {
            ShmSpaceError::CapNotFound => ShmCapNotFoundSnafu { id: input_shm_cap_id }.build(),
            err => AccessibilityTreeSpaceError::ShmSpaceInternalError { source: err },
        })?;

        // Move out of the SHM space for the duration of us processing it.
        let input_shm_cap = shm_space.move_shm_cap_to_other_space(input_shm_cap_id).ok_or_else(|| PublishInternalSnafu.build())?; // Internal error because presence was already checked in release
        // Create an output cap
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1).map_err(|shm_new_error| match shm_new_error {
            ShmSpaceError::CapacityNotAvailable
            | ShmSpaceError::BackingCapacityNotAvailable { .. }
            | ShmSpaceError::BackingCapacityNotAvailableOverflows => ShmCapacityNotAvailableSnafu.build(),
            ShmSpaceError::Exhausted => ShmExhaustedSnafu.build(),
            err => AccessibilityTreeSpaceError::ShmSpaceInternalError { source: err },
        })?;
        let output_shm_cap = shm_space.move_shm_cap_to_other_space(output_shm_cap_id).ok_or_else(|| PublishInternalSnafu.build())?; // Internal error because we just created it

        accessibility_tree_cap.in_progress_cap = Some(InProgressCap::new((input_shm_cap_id, input_shm_cap), (output_shm_cap_id, output_shm_cap)));
        Ok(())
    }

    pub fn publish_accessibility_tree_deferred(&mut self) {
        todo!()
    }

    pub fn destroy_accessibility_tree_cap(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId) -> Result<(), AccessibilityTreeSpaceError> {
        self.space.contains_key(&accessibility_tree_cap_id).then_some(()).ok_or_else(|| CapNotFoundSnafu { id: accessibility_tree_cap_id }.build())?;

        self.space.remove(&accessibility_tree_cap_id);
        self.id_pool.release(accessibility_tree_cap_id);

        Ok(())
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum AccessibilityTreeSpaceError {
    #[snafu(display("The new pool ID was already present in the space. This should never happen, and indicates a bug in Nushift's code."))]
    DuplicateId,
    #[snafu(display("The maximum amount of accessibility tree capabilities have been used for this app. Please destroy some capabilities."))]
    Exhausted,
    #[snafu(display("The accessibility tree cap with ID {id} was not found."))]
    CapNotFound { id: AccessibilityTreeCapId },
    #[snafu(display("The SHM cap with ID {id} was not found."))]
    ShmCapNotFound { id: ShmCapId },
    ShmExhausted,
    ShmCapacityNotAvailable,
    ShmSpaceInternalError { source: ShmSpaceError }, // Should never occur, indicates a bug in Nushift's code
    PublishInternalError, // Should never occur, indicates a bug in Nushift's code
}
