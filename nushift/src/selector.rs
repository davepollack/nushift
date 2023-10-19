use std::sync::Mutex;

use druid::{Selector, SingleUse};
use nushift_core::HypervisorEvent;
use reusable_id_pool::ArcId;

use crate::model::scale_and_size::ScaleAndSize;

pub(crate) const HYPERVISOR_EVENT: Selector<InspectBeforeSingleUse<HypervisorEvent>> = Selector::new("hypervisor.event");

pub(crate) const INITIAL_SCALE_AND_SIZE: Selector<SingleUse<ScaleAndSize>> = Selector::new("client-area.initial-scale-and-size");
pub(crate) const SCALE_OR_SIZE_CHANGED: Selector<SingleUse<ScaleAndSize>> = Selector::new("client-area.size-or-scale-changed");

pub(crate) trait Inspectable {
    type InspectType;

    fn inspect(&self) -> Self::InspectType;
}

impl Inspectable for HypervisorEvent {
    type InspectType = Option<ArcId>;

    fn inspect(&self) -> Self::InspectType {
        self.tab_id()
    }
}

pub(crate) struct InspectBeforeSingleUse<T>(Mutex<Option<T>>);

impl<T: Inspectable> InspectBeforeSingleUse<T> {
    pub(crate) fn inspect(&self) -> Option<T::InspectType> {
        self.0.lock().unwrap().as_ref().map(T::inspect)
    }
}

impl<T> InspectBeforeSingleUse<T> {
    pub(crate) fn new(t: T) -> Self {
        Self(Mutex::new(Some(t)))
    }

    pub(crate) fn take(&self) -> Option<T> {
        self.0.lock().unwrap().take()
    }
}
