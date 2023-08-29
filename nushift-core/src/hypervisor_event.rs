use std::sync::Arc;

use reusable_id_pool::ArcId;

/// For now, do Fn not FnMut, because we actually don't need mutability for
/// ExtEventSink::submit_command because it uses a lock. We can always expand to
/// FnMut later.
pub type HypervisorEventHandler = Arc<dyn Fn(HypervisorEvent) + Send + Sync + 'static>;
pub(crate) type BoundHypervisorEventHandler = Arc<dyn Fn(UnboundHypervisorEvent) + Send + Sync + 'static>;

pub enum HypervisorEvent {
    TitleChange(ArcId, String),
}

pub(crate) enum UnboundHypervisorEvent {
    TitleChange(String),
}

impl HypervisorEvent {
    pub(crate) fn from(tab_id: ArcId, unbound_hyp_event: UnboundHypervisorEvent) -> Self {
        match unbound_hyp_event {
            UnboundHypervisorEvent::TitleChange(new_title) => HypervisorEvent::TitleChange(tab_id, new_title),
        }
    }
}
