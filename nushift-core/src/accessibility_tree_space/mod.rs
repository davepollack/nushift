// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

use self::accessibility_tree::AccessibilityTree;
use super::deferred_space::{self, DeferredSpace, DeferredSpacePublish, DefaultDeferredSpace, DeferredError, DeferredSpaceError};
use super::shm_space::{ShmSpace, ShmCapId, ShmCap};

mod accessibility_tree;

pub type AccessibilityTreeCapId = u64;
const A11Y_CONTEXT: &str = "accessibility tree";

pub struct AccessibilityTreeSpace {
    deferred_space: DefaultDeferredSpace,
    accessibility_tree_space_specific: AccessibilityTreeSpaceSpecific,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessibilityTreeSpaceRonPayload<'payload> {
    ron_accessibility_tree: &'payload str,
}

struct AccessibilityTreeSpaceSpecific {
    app_accessibility_tree: Option<AccessibilityTree>,
}

impl DeferredSpacePublish for AccessibilityTreeSpaceSpecific {
    // TODO: We need multiple payloads.
    type Payload<'de> = AccessibilityTreeSpaceRonPayload<'de>;

    fn publish_cap_payload(&mut self, payload: Self::Payload<'_>, output_shm_cap: &mut ShmCap, _cap_id: u64) {
        let accessibility_tree = match ron::from_str(payload.ron_accessibility_tree) {
            Ok(accessibility_tree) => accessibility_tree,
            Err(spanned_error) => {
                tracing::debug!("Deserialisation error: {spanned_error}");
                deferred_space::print_error(output_shm_cap, DeferredError::DeserializeRonError, &spanned_error);
                return;
            },
        };
        tracing::debug!("{accessibility_tree:?}");
        self.app_accessibility_tree = Some(accessibility_tree);
        deferred_space::print_success(output_shm_cap, ());
    }
}

impl AccessibilityTreeSpaceSpecific {
    fn new() -> Self {
        Self { app_accessibility_tree: None }
    }
}

impl AccessibilityTreeSpace {
    pub fn new() -> Self {
        Self {
            deferred_space: DefaultDeferredSpace::new(),
            accessibility_tree_space_specific: AccessibilityTreeSpaceSpecific::new(),
        }
    }

    // TODO: Should new and destroy also be part blocking, part deferred?

    pub fn new_accessibility_tree_cap(&mut self) -> Result<AccessibilityTreeCapId, DeferredSpaceError> {
        self.deferred_space.new_cap(A11Y_CONTEXT)
    }

    pub fn publish_accessibility_tree_ron_blocking(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId, input_shm_cap_id: ShmCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.deferred_space.publish_blocking(A11Y_CONTEXT, accessibility_tree_cap_id, input_shm_cap_id, output_shm_cap_id, shm_space)
    }

    pub fn publish_accessibility_tree_ron_deferred(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.deferred_space.publish_deferred(&mut self.accessibility_tree_space_specific, accessibility_tree_cap_id, shm_space)
    }

    pub fn destroy_accessibility_tree_cap(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId) -> Result<(), DeferredSpaceError> {
        self.deferred_space.destroy_cap(A11Y_CONTEXT, accessibility_tree_cap_id)
    }
}
