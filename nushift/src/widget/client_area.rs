use druid::widget::prelude::*;
use druid::{Color, SingleUse};

use crate::model::RootData;
use crate::selector::{INITIAL_SCALE_AND_SIZE, SCALE_OR_SIZE_CHANGED};

pub struct ClientArea {
    color: Color,
}

impl ClientArea {
    pub fn new(color: Color) -> Self {
        ClientArea { color }
    }
}

impl Widget<RootData> for ClientArea {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut RootData, _env: &Env) {
        match event {
            // Handler for commands both from scale changes in this same `event`
            // method, and from initialisation and size changes from
            // `lifecycle`.
            Event::Command(cmd) => {
                // Coalesce both cases into the same handling logic.
                let scale_and_size = match (cmd.get(INITIAL_SCALE_AND_SIZE), cmd.get(SCALE_OR_SIZE_CHANGED)) {
                    (Some(initial), _) => Some(initial),
                    (_, Some(changed)) => Some(changed),
                    _ => None,
                };

                // Unwrap both that the command matched and that the SingleUse container contains a value.
                if let Some(scale_and_size) = scale_and_size.and_then(SingleUse::take) {
                    // Update RootData (for new tabs).
                    data.scale_and_size = scale_and_size;

                    // Update all existing tabs.
                    data.hypervisor.lock().unwrap().update_all_tab_outputs(data.scale_and_size.output());
                }
            },

            // Detect and submit command for scale changes.
            //
            // TODO:
            // Check that this Event::WindowScale(scale) is actually fired on
            // this widget when you drag the window to a different display that
            // has a different scale.
            Event::WindowScale(scale) => {
                tracing::debug!("Client area size: {:?}", ctx.size());
                tracing::debug!("Window scale: {:?}", scale);
                ctx.submit_command(SCALE_OR_SIZE_CHANGED.with(SingleUse::new((*scale, ctx.size()).into())));
            },

            _ => {},
        }
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, _data: &RootData, _env: &Env) {
        match event {
            LifeCycle::WidgetAdded => ctx.submit_command(INITIAL_SCALE_AND_SIZE.with(SingleUse::new((ctx.scale(), ctx.size()).into()))),
            LifeCycle::Size(size) => ctx.submit_command(SCALE_OR_SIZE_CHANGED.with(SingleUse::new((ctx.scale(), *size).into()))),
            _ => {},
        }
    }

    fn update(&mut self, _ctx: &mut UpdateCtx, _old_data: &RootData, _data: &RootData, _env: &Env) {}

    fn layout(&mut self, _ctx: &mut LayoutCtx, bc: &BoxConstraints, _data: &RootData, _env: &Env) -> Size {
        bc.max()
    }

    fn paint(&mut self, ctx: &mut PaintCtx, _data: &RootData, _env: &Env) {
        let size = ctx.size();
        let rect = size.to_rect();
        ctx.fill(rect, &self.color);
    }
}
