use crate::deferred_space::{DefaultDeferredSpace, DeferredSpace, DeferredSpaceError, DeferredSpaceSpecificGet};
use crate::shm_space::{ShmCap, ShmCapId, ShmSpace};

pub type GfxCapId = u64;
const GFX_CONTEXT: &str = "gfx";

pub struct GfxSpace {
    deferred_space: DefaultDeferredSpace,
    gfx_get_outputs: GfxGetOutputs,
}

struct GfxGetOutputs;

impl DeferredSpaceSpecificGet for GfxGetOutputs {
    fn get_specific(&mut self, output_shm_cap: &mut ShmCap) {
        todo!()
    }
}

impl GfxGetOutputs {
    fn new() -> Self {
        Self
    }
}

impl GfxSpace {
    pub(crate) fn new() -> Self {
        Self {
            deferred_space: DefaultDeferredSpace::new(),
            gfx_get_outputs: GfxGetOutputs::new(),
        }
    }

    pub fn new_gfx_cap(&mut self) -> Result<GfxCapId, DeferredSpaceError> {
        self.deferred_space.new_cap(GFX_CONTEXT)
    }

    pub fn get_outputs_blocking(&mut self, gfx_cap_id: GfxCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.deferred_space.get_blocking(GFX_CONTEXT, gfx_cap_id, output_shm_cap_id, shm_space)
    }

    pub fn get_outputs_deferred(&mut self, gfx_cap_id: GfxCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.deferred_space.get_deferred(&mut self.gfx_get_outputs, gfx_cap_id, shm_space)
    }

    pub fn destroy_gfx_cap(&mut self, gfx_cap_id: GfxCapId) -> Result<(), DeferredSpaceError> {
        self.deferred_space.destroy_cap(GFX_CONTEXT, gfx_cap_id)
    }
}
