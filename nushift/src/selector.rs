use druid::{Selector, SingleUse};
use nushift_core::HypervisorEvent;

use crate::model::scale_and_size::ScaleAndSize;

pub(crate) const HYPERVISOR_EVENT: Selector<HypervisorEvent> = Selector::new("hypervisor.event");

pub(crate) const INITIAL_SCALE_AND_SIZE: Selector<SingleUse<ScaleAndSize>> = Selector::new("client-area.initial-scale-and-size");
pub(crate) const SCALE_OR_SIZE_CHANGED: Selector<SingleUse<ScaleAndSize>> = Selector::new("client-area.size-or-scale-changed");
