use std::collections::HashMap;
use std::sync::Arc;

use num_enum::TryFromPrimitive;

use crate::deferred_space::{self, DefaultDeferredSpace, DeferredSpace, DeferredSpaceError, DeferredSpaceGet, DefaultDeferredSpaceCapId, DeferredSpacePublish, DeferredError};
use crate::hypervisor::hypervisor_event::{UnboundHypervisorEvent, HypervisorEventError};
use crate::hypervisor::tab::Output;
use crate::hypervisor::tab_context::TabContext;
use crate::shm_space::{ShmCap, ShmCapId, ShmSpace};

pub type GfxCapId = u64;
pub type GfxCpuPresentBufferCapId = u64;
const GFX_CONTEXT: &str = "gfx";
const GFX_CPU_PRESENT_CONTEXT: &str = "gfx_cpu_present";

pub struct GfxSpace {
    root_deferred_space: DefaultDeferredSpace,
    cpu_present_buffer_deferred_space: DefaultDeferredSpace,
    get_outputs: GetOutputs,
    cpu_present: CpuPresent,
}

struct GetOutputs {
    tab_context: Arc<dyn TabContext>,
}

impl DeferredSpaceGet for GetOutputs {
    fn get(&mut self, output_shm_cap: &mut ShmCap) {
        // TODO: Need to serialise a unified structure that could contain an
        // error or the success result. Not this where you can't discriminate
        // between either.

        let outputs = self.tab_context.get_outputs();
        let outputs_dereferenced: Vec<&Output> = outputs.iter().map(|guard| &**guard).collect();
        // TODO: Serialise an error for the serialise buffer being full! When in
        // the future we are serialising a unified structure.
        let _ = postcard::to_slice(&outputs_dereferenced, output_shm_cap.backing_mut());
    }
}

impl GetOutputs {
    fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self { tab_context }
    }
}

#[derive(TryFromPrimitive, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum PresentBufferFormat {
    R8g8b8UintSrgb = 0,
}

struct CpuPresentBufferInfo {
    present_buffer_format: PresentBufferFormat,
    buffer_shm_cap_id: ShmCapId,
}

struct CpuPresent {
    tab_context: Arc<dyn TabContext>,
    space: HashMap<DefaultDeferredSpaceCapId, CpuPresentBufferInfo>,
}

impl DeferredSpacePublish for CpuPresent {
    type Payload<'de> = &'de [u8];

    fn publish_cap_payload(&mut self, payload: Self::Payload<'_>, output_shm_cap: &mut ShmCap, cap_id: u64) {
        let Some(cpu_present_buffer_info) = self.get_info(cap_id) else {
            let error_message = format!("Extra info no longer present. GfxCpuPresentBufferCapId: {cap_id}");
            tracing::debug!(error_message);
            deferred_space::print_error(output_shm_cap, DeferredError::ExtraInfoNoLongerPresent, &error_message);
            return;
        };

        self.tab_context.send_hypervisor_event(UnboundHypervisorEvent::GfxCpuPresent(cpu_present_buffer_info.present_buffer_format, payload.into()))
            .unwrap_or_else(|hypervisor_event_error| match hypervisor_event_error {
                HypervisorEventError::SubmitCommandError => {
                    tracing::debug!("Submit failed: {hypervisor_event_error}");
                    deferred_space::print_error(output_shm_cap, DeferredError::SubmitFailed, &hypervisor_event_error);
                },
            });
    }
}

impl CpuPresent {
    fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self { tab_context, space: HashMap::new() }
    }

    fn add_info(&mut self, cap_id: DefaultDeferredSpaceCapId, present_buffer_format: PresentBufferFormat, buffer_shm_cap_id: ShmCapId) {
        self.space.insert(cap_id, CpuPresentBufferInfo { present_buffer_format, buffer_shm_cap_id });
    }

    fn get_info(&self, cap_id: DefaultDeferredSpaceCapId) -> Option<&CpuPresentBufferInfo> {
        self.space.get(&cap_id)
    }

    fn remove_info(&mut self, cap_id: DefaultDeferredSpaceCapId) -> Option<CpuPresentBufferInfo> {
        self.space.remove(&cap_id)
    }
}

impl GfxSpace {
    pub(crate) fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self {
            root_deferred_space: DefaultDeferredSpace::new(),
            cpu_present_buffer_deferred_space: DefaultDeferredSpace::new(),
            get_outputs: GetOutputs::new(Arc::clone(&tab_context)),
            cpu_present: CpuPresent::new(Arc::clone(&tab_context)),
        }
    }

    pub fn new_gfx_cap(&mut self) -> Result<GfxCapId, DeferredSpaceError> {
        self.root_deferred_space.new_cap(GFX_CONTEXT)
    }

    pub fn get_outputs_blocking(&mut self, gfx_cap_id: GfxCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.root_deferred_space.get_blocking(GFX_CONTEXT, gfx_cap_id, output_shm_cap_id, shm_space)
    }

    pub fn get_outputs_deferred(&mut self, gfx_cap_id: GfxCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.root_deferred_space.get_deferred(&mut self.get_outputs, gfx_cap_id, shm_space)
    }

    pub fn new_gfx_cpu_present_buffer_cap(&mut self, gfx_cap_id: GfxCapId, present_buffer_format: PresentBufferFormat, buffer_shm_cap_id: ShmCapId) -> Result<GfxCpuPresentBufferCapId, DeferredSpaceError> {
        // Check that gfx_cap_id is a valid cap
        self.root_deferred_space.contains_key(gfx_cap_id).then_some(()).ok_or_else(|| DeferredSpaceError::CapNotFound { context: GFX_CONTEXT.into(), id: gfx_cap_id })?;

        // Create the cpu present cap
        let gfx_cpu_present_buffer_cap_id = self.cpu_present_buffer_deferred_space.new_cap(GFX_CPU_PRESENT_CONTEXT)?;
        let mut all_succeeded = drop_guard::guard(false, |all_succeeded| {
            if !all_succeeded {
                // CapNotFound is an internal error. We cannot change the error
                // being returned at this point anyway, so ignore.
                let _ = self.cpu_present_buffer_deferred_space.destroy_cap(GFX_CPU_PRESENT_CONTEXT, gfx_cpu_present_buffer_cap_id);
            }
        });

        // Store the additional info
        self.cpu_present.add_info(gfx_cpu_present_buffer_cap_id, present_buffer_format, buffer_shm_cap_id);

        *all_succeeded = true;
        Ok(gfx_cpu_present_buffer_cap_id)
    }

    pub fn cpu_present_blocking(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        let cpu_present_buffer_info = self.cpu_present.get_info(gfx_cpu_present_buffer_cap_id)
            .ok_or_else(|| DeferredSpaceError::CapNotFound { context: GFX_CPU_PRESENT_CONTEXT.into(), id: gfx_cpu_present_buffer_cap_id })?;

        self.cpu_present_buffer_deferred_space.publish_blocking(GFX_CPU_PRESENT_CONTEXT, gfx_cpu_present_buffer_cap_id, cpu_present_buffer_info.buffer_shm_cap_id, output_shm_cap_id, shm_space)
    }

    pub fn cpu_present_deferred(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.cpu_present_buffer_deferred_space.publish_deferred(&mut self.cpu_present, gfx_cpu_present_buffer_cap_id, shm_space)
    }

    pub fn destroy_gfx_cpu_present_buffer_cap(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId) -> Result<(), DeferredSpaceError> {
        let cpu_present_buffer_info = self.cpu_present.remove_info(gfx_cpu_present_buffer_cap_id).ok_or_else(|| DeferredSpaceError::CapNotFound { context: GFX_CPU_PRESENT_CONTEXT.into(), id: gfx_cpu_present_buffer_cap_id })?;
        let mut all_succeeded = drop_guard::guard(false, |all_succeeded| {
            if !all_succeeded {
                self.cpu_present.add_info(gfx_cpu_present_buffer_cap_id, cpu_present_buffer_info.present_buffer_format, cpu_present_buffer_info.buffer_shm_cap_id);
            }
        });

        self.cpu_present_buffer_deferred_space.destroy_cap(GFX_CPU_PRESENT_CONTEXT, gfx_cpu_present_buffer_cap_id)?;

        *all_succeeded = true;
        Ok(())
    }

    pub fn destroy_gfx_cap(&mut self, gfx_cap_id: GfxCapId) -> Result<(), DeferredSpaceError> {
        self.root_deferred_space.destroy_cap(GFX_CONTEXT, gfx_cap_id)
    }
}
