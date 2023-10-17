use std::sync::Arc;

use serde::Deserialize;

use crate::deferred_space::{self, DeferredSpace, DefaultDeferredSpace, DeferredSpacePublish, DeferredError, DeferredSpaceError};
use crate::hypervisor::hypervisor_event::{UnboundHypervisorEvent, HypervisorEventError};
use crate::hypervisor::tab_context::TabContext;
use crate::shm_space::{ShmCapId, ShmCap, ShmSpace};

pub type TitleCapId = u64;
const TITLE_CONTEXT: &str = "title";

pub struct TitleSpace {
    deferred_space: DefaultDeferredSpace,
    title_space_specific: TitleSpaceSpecific,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TitleSpacePayload<'payload> {
    /// I would like the wire encoding of the length to be a u64, not usize as
    /// is entailed by the &str type and Postcard. I don't know if that's
    /// possible though.
    title: &'payload str,
}

struct TitleSpaceSpecific {
    tab_context: Arc<dyn TabContext>,
}

impl DeferredSpacePublish for TitleSpaceSpecific {
    type Payload<'de> = TitleSpacePayload<'de>;

    fn publish_cap_payload(&mut self, payload: Self::Payload<'_>, output_shm_cap: &mut ShmCap, _cap_id: u64) {
        self.tab_context.send_hypervisor_event(UnboundHypervisorEvent::TitleChange(payload.title.into()))
            .unwrap_or_else(|hypervisor_event_error| match hypervisor_event_error {
                HypervisorEventError::SubmitCommandError => {
                    tracing::debug!("Submit failed: {hypervisor_event_error}");
                    deferred_space::print_error(output_shm_cap, DeferredError::SubmitFailed, &hypervisor_event_error);
                },
            });
    }
}

impl TitleSpaceSpecific {
    fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self { tab_context }
    }
}

impl TitleSpace {
    pub(crate) fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self {
            deferred_space: DefaultDeferredSpace::new(),
            title_space_specific: TitleSpaceSpecific::new(tab_context),
        }
    }

    pub fn new_title_cap(&mut self) -> Result<TitleCapId, DeferredSpaceError> {
        self.deferred_space.new_cap(TITLE_CONTEXT)
    }

    pub fn publish_title_blocking(&mut self, title_cap_id: TitleCapId, input_shm_cap_id: ShmCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.deferred_space.publish_blocking(TITLE_CONTEXT, title_cap_id, input_shm_cap_id, output_shm_cap_id, shm_space)
    }

    pub fn publish_title_deferred(&mut self, title_cap_id: TitleCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.deferred_space.publish_deferred(&mut self.title_space_specific, title_cap_id, shm_space)
    }

    pub fn destroy_title_cap(&mut self, title_cap_id: TitleCapId) -> Result<(), DeferredSpaceError> {
        self.deferred_space.destroy_cap(TITLE_CONTEXT, title_cap_id)
    }
}
