// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use druid::{AppDelegate, DelegateCtx, WindowId, Event, Env, HotKey, SysMods};

use crate::model::RootData;

pub struct GlobalKeyCommandHandler;

impl GlobalKeyCommandHandler {
    pub fn new() -> Self {
        Self
    }
}

impl AppDelegate<RootData> for GlobalKeyCommandHandler {
    fn event(&mut self, _ctx: &mut DelegateCtx, _window_id: WindowId, event: Event, root_data: &mut RootData, env: &Env) -> Option<Event> {
        match event {
            Event::KeyDown(ref key_event) if HotKey::new(SysMods::Cmd, "w").matches(key_event) => {
                root_data.close_selected_tab();
            }
            Event::KeyDown(ref key_event) if HotKey::new(SysMods::Cmd, "t").matches(key_event) => {
                root_data.add_new_tab(env);
            }
            _ => {}
        }

        // Should we switch to a focus-based approach? How are keyboard events
        // going to be passed down to the app? Focus-based approach and this
        // AppDelegate (so the passing down of the event should still happen)?
        // Purely focus-based approach (no AppDelegate)? `Menu` for these key
        // commands, no AppDelegate, and focus-based approach?
        Some(event)
    }
}
