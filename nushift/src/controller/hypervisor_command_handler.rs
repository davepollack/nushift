use druid::{Env, EventCtx, Data, Widget, widget::Controller, Event};
use nushift_core::HypervisorEvent;

use crate::selector::HYPERVISOR_EVENT;

pub struct HypervisorCommandHandler<T> {
    action: Box<dyn Fn(&HypervisorEvent, &mut T)>,
}

impl<T> HypervisorCommandHandler<T> {
    pub fn new(action: impl Fn(&HypervisorEvent, &mut T) + 'static) -> Self {
        Self { action: Box::new(action) }
    }
}

impl<T: Data, W: Widget<T>> Controller<T, W> for HypervisorCommandHandler<T> {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        match event {
            Event::Command(cmd) => {
                if let Some(hypervisor_event) = cmd.get(HYPERVISOR_EVENT) {
                    (self.action)(hypervisor_event, data);
                }
            },
            _ => child.event(ctx, event, data, env),
        }
    }
}
