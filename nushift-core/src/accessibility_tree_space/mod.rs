// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use serde::Deserialize;

use self::accessibility_tree::AccessibilityTree;
use super::deferred_space::{self, DeferredSpace, DeferredSpacePublish, DefaultDeferredSpace, DeferredError, DeferredSpaceError};
use super::shm_space::{ShmSpace, ShmCapId, ShmCap};

mod accessibility_tree;

pub type AccessibilityTreeCapId = u64;
const A11Y_CONTEXT: &str = "accessibility tree";

pub struct AccessibilityTreeSpace {
    deferred_space: DefaultDeferredSpace,
    publish_ron: PublishRon,
    publish: Publish,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RonPayload<'payload> {
    ron_accessibility_tree: &'payload str,
}

struct PublishRon {
    app_accessibility_tree: Arc<Mutex<Option<AccessibilityTree>>>,
}

impl PublishRon {
    fn new(app_accessibility_tree: Arc<Mutex<Option<AccessibilityTree>>>) -> Self {
        Self { app_accessibility_tree }
    }
}

impl DeferredSpacePublish for PublishRon {
    type Payload<'de> = RonPayload<'de>;

    fn publish_cap_payload(&mut self, payload: Self::Payload<'_>, output_shm_cap: &mut ShmCap, _cap_id: u64) {
        let accessibility_tree = match ron::from_str(payload.ron_accessibility_tree) {
            Ok(accessibility_tree) => accessibility_tree,
            Err(spanned_error) => {
                tracing::debug!("Deserialisation error: {spanned_error}");
                deferred_space::print_error(output_shm_cap, DeferredError::DeserializeRonError, &spanned_error);
                return;
            }
        };
        tracing::debug!("{accessibility_tree:?}");
        *self.app_accessibility_tree.lock().unwrap() = Some(accessibility_tree);
        deferred_space::print_success(output_shm_cap, ());
    }
}

struct Publish {
    app_accessibility_tree: Arc<Mutex<Option<AccessibilityTree>>>,
}

impl Publish {
    fn new(app_accessibility_tree: Arc<Mutex<Option<AccessibilityTree>>>) -> Self {
        Self { app_accessibility_tree }
    }
}

impl DeferredSpacePublish for Publish {
    type Payload<'de> = AccessibilityTree;

    fn publish_cap_payload(&mut self, payload: Self::Payload<'_>, output_shm_cap: &mut ShmCap, _cap_id: u64) {
        tracing::debug!("{payload:?}");
        *self.app_accessibility_tree.lock().unwrap() = Some(payload);
        deferred_space::print_success(output_shm_cap, ());
    }
}

impl AccessibilityTreeSpace {
    pub fn new() -> Self {
        let app_accessibility_tree = Arc::new(Mutex::new(None));

        Self {
            deferred_space: DefaultDeferredSpace::new(),
            publish_ron: PublishRon::new(Arc::clone(&app_accessibility_tree)),
            publish: Publish::new(Arc::clone(&app_accessibility_tree)),
        }
    }

    // TODO: Should new and destroy also be part blocking, part deferred?

    pub fn new_accessibility_tree_cap(&mut self) -> Result<AccessibilityTreeCapId, DeferredSpaceError> {
        self.deferred_space.new_cap(A11Y_CONTEXT)
    }

    pub fn publish_accessibility_tree_blocking(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId, input_shm_cap_id: ShmCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.deferred_space.publish_blocking(A11Y_CONTEXT, accessibility_tree_cap_id, input_shm_cap_id, output_shm_cap_id, shm_space)
    }

    pub fn publish_accessibility_tree_ron_deferred(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.deferred_space.publish_deferred(&mut self.publish_ron, accessibility_tree_cap_id, shm_space)
    }

    pub fn publish_accessibility_tree_deferred(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.deferred_space.publish_deferred(&mut self.publish, accessibility_tree_cap_id, shm_space)
    }

    pub fn destroy_accessibility_tree_cap(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId) -> Result<(), DeferredSpaceError> {
        self.deferred_space.destroy_cap(A11Y_CONTEXT, accessibility_tree_cap_id)
    }
}
