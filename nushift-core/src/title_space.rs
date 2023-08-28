use super::deferred_space::{DeferredSpace, DefaultDeferredSpace, DeferredSpaceSpecific, DeferredSpaceError};
use super::shm_space::{ShmCapId, ShmCap, ShmSpace};

pub type TitleCapId = u64;
const TITLE_CONTEXT: &str = "title";

pub struct TitleSpace {
    deferred_space: DefaultDeferredSpace,
    title_space_specific: TitleSpaceSpecific,
}

struct TitleSpaceSpecific {
    title: Option<String>,
}

impl DeferredSpaceSpecific for TitleSpaceSpecific {
    fn process_cap_str(&mut self, str: &str, _output_shm_cap: &mut ShmCap) {
        self.title = Some(str.into())
    }
}

impl TitleSpaceSpecific {
    fn new() -> Self {
        Self { title: None }
    }
}

impl TitleSpace {
    pub fn new() -> Self {
        Self {
            deferred_space: DefaultDeferredSpace::new(),
            title_space_specific: TitleSpaceSpecific::new(),
        }
    }

    pub fn title(&self) -> Option<&str> {
        self.title_space_specific.title.as_deref()
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
