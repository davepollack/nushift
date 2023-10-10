use reusable_id_pool::ArcId;

use super::hypervisor_event::{HypervisorEvent, HypervisorEventHandler, UnboundHypervisorEvent, HypervisorEventError};

pub(crate) trait TabContext: Send + Sync {
    fn send_hypervisor_event(&self, unbound_hypervisor_event: UnboundHypervisorEvent) -> Result<(), HypervisorEventError>;
}

pub(crate) struct DefaultTabContext {
    tab_id: ArcId,
    hypervisor_event_handler: HypervisorEventHandler,
}

impl TabContext for DefaultTabContext {
    fn send_hypervisor_event(&self, unbound_hypervisor_event: UnboundHypervisorEvent) -> Result<(), HypervisorEventError> {
        (self.hypervisor_event_handler)(HypervisorEvent::from(ArcId::clone(&self.tab_id), unbound_hypervisor_event))
    }
}

impl DefaultTabContext {
    pub(crate) fn new(tab_id: ArcId, hypervisor_event_handler: HypervisorEventHandler) -> Self {
        Self { tab_id, hypervisor_event_handler }
    }
}
