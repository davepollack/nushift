use std::sync::{MutexGuard, Arc, Mutex};

use reusable_id_pool::ArcId;

use super::hypervisor_event::{HypervisorEvent, HypervisorEventHandler, UnboundHypervisorEvent, HypervisorEventError};
use super::tab::Output;

pub(crate) trait TabContext: Send + Sync {
    fn send_hypervisor_event(&self, unbound_hypervisor_event: UnboundHypervisorEvent) -> Result<(), HypervisorEventError>;
    fn get_outputs(&self) -> Vec<MutexGuard<'_, Output>>;
}

pub(crate) struct DefaultTabContext {
    tab_id: ArcId,
    hypervisor_event_handler: HypervisorEventHandler,
    output: Arc<Mutex<Output>>,
}

impl DefaultTabContext {
    pub(crate) fn new(tab_id: ArcId, hypervisor_event_handler: HypervisorEventHandler, output: Arc<Mutex<Output>>) -> Self {
        Self { tab_id, hypervisor_event_handler, output }
    }
}

impl TabContext for DefaultTabContext {
    fn send_hypervisor_event(&self, unbound_hypervisor_event: UnboundHypervisorEvent) -> Result<(), HypervisorEventError> {
        (self.hypervisor_event_handler)(HypervisorEvent::from(ArcId::clone(&self.tab_id), unbound_hypervisor_event))
    }

    fn get_outputs(&self) -> Vec<MutexGuard<'_, Output>> {
        vec![self.output.lock().unwrap()]
    }
}
