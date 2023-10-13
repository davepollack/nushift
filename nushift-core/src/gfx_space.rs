use std::sync::Arc;

use num_enum::TryFromPrimitive;

use crate::deferred_space::{DefaultDeferredSpace, DeferredSpace, DeferredSpaceError, DeferredSpaceSpecificGet};
use crate::hypervisor::tab::Output;
use crate::hypervisor::tab_context::TabContext;
use crate::shm_space::{ShmCap, ShmCapId, ShmSpace};

pub type GfxCapId = u64;
pub type GfxCpuPresentBufferCapId = u64;
const GFX_CONTEXT: &str = "gfx";

pub struct GfxSpace {
    root_deferred_space: DefaultDeferredSpace,
    cpu_present_buffer_deferred_space: DefaultDeferredSpace,
    gfx_get_outputs: GfxGetOutputs,
}

#[derive(TryFromPrimitive, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum PresentBufferFormat {
    R8g8b8UintSrgb = 0,
}

struct GfxGetOutputs {
    tab_context: Arc<dyn TabContext>,
}

impl DeferredSpaceSpecificGet for GfxGetOutputs {
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

impl GfxGetOutputs {
    fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self { tab_context }
    }
}

impl GfxSpace {
    pub(crate) fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self {
            root_deferred_space: DefaultDeferredSpace::new(),
            cpu_present_buffer_deferred_space: DefaultDeferredSpace::new(),
            gfx_get_outputs: GfxGetOutputs::new(tab_context),
        }
    }

    pub fn new_gfx_cap(&mut self) -> Result<GfxCapId, DeferredSpaceError> {
        self.root_deferred_space.new_cap(GFX_CONTEXT)
    }

    pub fn get_outputs_blocking(&mut self, gfx_cap_id: GfxCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.root_deferred_space.get_blocking(GFX_CONTEXT, gfx_cap_id, output_shm_cap_id, shm_space)
    }

    pub fn get_outputs_deferred(&mut self, gfx_cap_id: GfxCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.root_deferred_space.get_deferred(&mut self.gfx_get_outputs, gfx_cap_id, shm_space)
    }

    pub fn new_gfx_cpu_present_buffer_cap(&mut self, gfx_cap_id: GfxCapId, present_buffer_format: PresentBufferFormat, buffer_shm_cap_id: ShmCapId) -> Result<GfxCpuPresentBufferCapId, DeferredSpaceError> {
        todo!()
    }

    pub fn cpu_present_blocking(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        todo!()
    }

    pub fn cpu_present_deferred(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        todo!()
    }

    pub fn destroy_gfx_cpu_present_buffer_cap(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId) -> Result<(), DeferredSpaceError> {
        todo!()
    }

    pub fn destroy_gfx_cap(&mut self, gfx_cap_id: GfxCapId) -> Result<(), DeferredSpaceError> {
        self.root_deferred_space.destroy_cap(GFX_CONTEXT, gfx_cap_id)
    }
}
