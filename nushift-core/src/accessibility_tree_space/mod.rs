use std::collections::HashSet;

use reusable_id_pool::{ReusableIdPoolManual, ReusableIdPoolError};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

pub type AccessibilityTreeCapId = u64;

pub struct AccessibilityTreeSpace {
    id_pool: ReusableIdPoolManual,
    space: HashSet<AccessibilityTreeCapId>,
}

impl AccessibilityTreeSpace {
    pub fn new() -> Self {
        AccessibilityTreeSpace { id_pool: ReusableIdPoolManual::new(), space: HashSet::new() }
    }

    pub fn new_accessibility_tree_cap(&mut self) -> Result<AccessibilityTreeCapId, AccessibilityTreeSpaceError> {
        let id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyLiveIDs => ExhaustedSnafu.build() })?;

        self.space.insert(id);

        Ok(id)
    }

    pub fn destroy_accessibility_tree_cap(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId) -> Result<(), AccessibilityTreeSpaceError> {
        self.space.contains(&accessibility_tree_cap_id).then_some(()).ok_or_else(|| CapNotFoundSnafu.build())?;

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
    #[snafu(display("A cap with the requested cap ID was not found."))]
    CapNotFound,
}
