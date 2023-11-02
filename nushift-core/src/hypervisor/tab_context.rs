use std::sync::{MutexGuard, Arc, Mutex};

use reusable_id_pool::ArcId;

use crate::gfx_space::GfxOutput;
use super::hypervisor_event::{HypervisorEvent, HypervisorEventHandler, UnboundHypervisorEvent, HypervisorEventError};

pub(crate) trait TabContext: Send + Sync {
    fn send_hypervisor_event(&self, unbound_hypervisor_event: UnboundHypervisorEvent) -> Result<(), HypervisorEventError>;
    fn get_gfx_outputs(&self) -> Vec<MutexGuard<'_, GfxOutput>>;
}

pub(crate) struct DefaultTabContext {
    tab_id: ArcId,
    hypervisor_event_handler: HypervisorEventHandler,
    gfx_output: Arc<Mutex<GfxOutput>>,
}

impl DefaultTabContext {
    pub(crate) fn new(tab_id: ArcId, hypervisor_event_handler: HypervisorEventHandler, gfx_output: Arc<Mutex<GfxOutput>>) -> Self {
        Self { tab_id, hypervisor_event_handler, gfx_output }
    }
}

impl TabContext for DefaultTabContext {
    fn send_hypervisor_event(&self, unbound_hypervisor_event: UnboundHypervisorEvent) -> Result<(), HypervisorEventError> {
        (self.hypervisor_event_handler)(HypervisorEvent::from(ArcId::clone(&self.tab_id), unbound_hypervisor_event))
    }

    fn get_gfx_outputs(&self) -> Vec<MutexGuard<'_, GfxOutput>> {
        vec![self.gfx_output.lock().unwrap()]
    }
}
