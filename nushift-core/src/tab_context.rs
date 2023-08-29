use reusable_id_pool::ArcId;

use super::hypervisor_event::{HypervisorEvent, HypervisorEventHandler, UnboundHypervisorEvent};

pub(crate) trait TabContext: Send + Sync {
    fn send_hypervisor_event(&self, unbound_hypervisor_event: UnboundHypervisorEvent);
}

pub(crate) struct DefaultTabContext {
    tab_id: ArcId,
    hypervisor_event_creator: HypervisorEventCreator,
}

impl TabContext for DefaultTabContext {
    fn send_hypervisor_event(&self, unbound_hypervisor_event: UnboundHypervisorEvent) {
        self.hypervisor_event_creator.send_hypervisor_event(ArcId::clone(&self.tab_id), unbound_hypervisor_event);
    }
}

impl DefaultTabContext {
    pub(crate) fn new(tab_id: ArcId, hypervisor_event_handler: HypervisorEventHandler) -> Self {
        Self { tab_id, hypervisor_event_creator: HypervisorEventCreator::new(hypervisor_event_handler) }
    }
}

struct HypervisorEventCreator {
    hypervisor_event_handler: HypervisorEventHandler,
}

impl HypervisorEventCreator {
    fn new(hypervisor_event_handler: HypervisorEventHandler) -> Self {
        Self { hypervisor_event_handler }
    }

    fn send_hypervisor_event(&self, tab_id: ArcId, unbound_hypervisor_event: UnboundHypervisorEvent) {
        (self.hypervisor_event_handler)(HypervisorEvent::from(tab_id, unbound_hypervisor_event));
    }
}
