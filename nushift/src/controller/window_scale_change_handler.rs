use druid::widget::{prelude::*, Controller};
use druid::SingleUse;

use crate::selector::SCALE_OR_SIZE_CHANGED;

pub struct WindowScaleChangeHandler;

impl WindowScaleChangeHandler {
    pub fn new() -> Self {
        Self
    }
}

impl<T: Data, W: Widget<T>> Controller<T, W> for WindowScaleChangeHandler {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        match event {
            // Detect and submit command for scale changes.
            //
            // TODO: I am not currently on a machine where I can test if this
            // works. When I am, test it.
            Event::WindowScale(scale) => {
                tracing::debug!("Client area size: {:?}", ctx.size());
                tracing::debug!("Window scale: {:?}", scale);
                ctx.submit_command(SCALE_OR_SIZE_CHANGED.with(SingleUse::new((*scale, ctx.size()).into())));
            },
            _ => child.event(ctx, event, data, env),
        }
    }
}
