use serde::Deserialize;

use super::deferred_space::{self, DeferredSpace, DefaultDeferredSpace, DeferredSpaceSpecific, DeferredError, DeferredSpaceError};
use super::hypervisor_event::{BoundHypervisorEventHandler, UnboundHypervisorEvent, HypervisorEventError};
use super::shm_space::{ShmCapId, ShmCap, ShmSpace};

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
    bound_hypervisor_event_handler: BoundHypervisorEventHandler,
}

impl DeferredSpaceSpecific for TitleSpaceSpecific {
    fn process_cap_payload(&mut self, input: &[u8], output_shm_cap: &mut ShmCap) {
        let Ok(payload): Result<TitleSpacePayload<'_>, ()> = deferred_space::deserialize_general(input) else { return; };

        (self.bound_hypervisor_event_handler)(UnboundHypervisorEvent::TitleChange(payload.title.into()))
            .unwrap_or_else(|hypervisor_event_error| match hypervisor_event_error {
                HypervisorEventError::SubmitCommandError => {
                    tracing::debug!("Submit failed: {hypervisor_event_error}");
                    deferred_space::print_error(output_shm_cap, DeferredError::SubmitFailed, &hypervisor_event_error);
                }
            });
    }
}

impl TitleSpaceSpecific {
    fn new(bound_hypervisor_event_handler: BoundHypervisorEventHandler) -> Self {
        Self { bound_hypervisor_event_handler }
    }
}

impl TitleSpace {
    pub(crate) fn new(bound_hypervisor_event_handler: BoundHypervisorEventHandler) -> Self {
        Self {
            deferred_space: DefaultDeferredSpace::new(),
            title_space_specific: TitleSpaceSpecific::new(bound_hypervisor_event_handler),
        }
    }

    pub fn new_title_cap(&mut self) -> Result<TitleCapId, DeferredSpaceError> {
        self.deferred_space.new_cap(TITLE_CONTEXT)
    }

    pub fn publish_title_blocking(&mut self, title_cap_id: TitleCapId, input_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.deferred_space.publish_blocking(TITLE_CONTEXT, title_cap_id, input_shm_cap_id, shm_space)
    }

    pub fn publish_title_deferred(&mut self, title_cap_id: TitleCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.deferred_space.publish_deferred(&mut self.title_space_specific, title_cap_id, shm_space)
    }

    pub fn destroy_title_cap(&mut self, title_cap_id: TitleCapId) -> Result<(), DeferredSpaceError> {
        self.deferred_space.destroy_cap(TITLE_CONTEXT, title_cap_id)
    }
}
