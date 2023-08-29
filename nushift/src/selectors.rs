use druid::Selector;
use reusable_id_pool::ArcId;

pub(crate) struct HypervisorTitleChangePayload {
    pub(crate) tab_id: ArcId,
    pub(crate) new_title: String,
}
pub(crate) const HYPERVISOR_TITLE_CHANGE: Selector<HypervisorTitleChangePayload> = Selector::new("hypervisor.title-change");
