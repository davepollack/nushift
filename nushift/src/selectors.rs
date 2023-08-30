use druid::Selector;
use nushift_core::HypervisorEvent;

pub(crate) const HYPERVISOR_EVENT: Selector<HypervisorEvent> = Selector::new("hypervisor.event");
