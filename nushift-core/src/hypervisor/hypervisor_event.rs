// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use reusable_id_pool::ArcId;
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use crate::gfx_space::PresentBufferFormat;

/// For now, do Fn not FnMut, because we actually don't need mutability for
/// ExtEventSink::submit_command because it uses a lock. We can always expand to
/// FnMut later.
pub trait HypervisorEventHandlerFn: Fn(HypervisorEvent) -> Result<(), HypervisorEventError> + Send + Sync + 'static {}
impl<F> HypervisorEventHandlerFn for F where F: Fn(HypervisorEvent) -> Result<(), HypervisorEventError> + Send + Sync + 'static {}
pub type HypervisorEventHandler = Arc<dyn HypervisorEventHandlerFn>;

pub enum HypervisorEvent {
    TitleChange(ArcId, String),
    GfxCpuPresent(ArcId, PresentBufferFormat, Vec<u64>, Arc<[u8]>),
}

pub(crate) enum UnboundHypervisorEvent {
    TitleChange(String),
    GfxCpuPresent(PresentBufferFormat, Vec<u64>, Arc<[u8]>),
}

impl HypervisorEvent {
    pub(crate) fn from(tab_id: ArcId, unbound_hyp_event: UnboundHypervisorEvent) -> Self {
        match unbound_hyp_event {
            UnboundHypervisorEvent::TitleChange(new_title) => HypervisorEvent::TitleChange(tab_id, new_title),
            UnboundHypervisorEvent::GfxCpuPresent(present_buffer_format, size_px, buffer) => HypervisorEvent::GfxCpuPresent(tab_id, present_buffer_format, size_px, buffer),
        }
    }

    pub fn tab_id(&self) -> Option<ArcId> {
        match self {
            Self::TitleChange(tab_id, ..) => Some(ArcId::clone(tab_id)),
            Self::GfxCpuPresent(tab_id, ..) => Some(ArcId::clone(tab_id)),
        }
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum HypervisorEventError {
    #[snafu(display("Submitting command to the Nushift shell failed. This probably means that the Nushift shell has gone away."))]
    SubmitCommandError,
}
