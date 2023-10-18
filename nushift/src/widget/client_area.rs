use std::sync::Arc;

use druid::piet::ImageFormat;
use druid::widget::{prelude::*, Image};
use druid::{SingleUse, WidgetPod, ImageBuf, Point};
use nushift_core::PresentBufferFormat;

use crate::model::RootData;
use crate::selector::{INITIAL_SCALE_AND_SIZE, SCALE_OR_SIZE_CHANGED};

pub struct ClientArea {
    image_widget: WidgetPod<RootData, Image>,
}

impl ClientArea {
    pub fn new() -> Self {
        let image_widget = WidgetPod::new(Image::new(ImageBuf::empty()));
        Self { image_widget }
    }

    fn update_image(&mut self, data: &RootData) {
        if let Some(ref scale_and_size) = data.scale_and_size {
            let output = scale_and_size.output();
            let (width, height) = (output.size_px()[0].try_into(), output.size_px()[1].try_into());

            if let (Ok(width), Ok(height)) = (width, height) {
                let img_buf = match data.client_framebuffer {
                    Some(ref client_framebuffer) => ImageBuf::from_raw(
                        Arc::clone(&client_framebuffer.framebuffer),
                        match client_framebuffer.present_buffer_format {
                            PresentBufferFormat::R8g8b8UintSrgb => ImageFormat::Rgb,
                        },
                        width,
                        height,
                    ),
                    None => ImageBuf::empty(),
                };

                self.image_widget.widget_mut().set_image_data(img_buf);
            }
        }
    }
}

impl Widget<RootData> for ClientArea {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut RootData, env: &Env) {
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
                    // Update all existing tabs.
                    data.hypervisor.lock().unwrap().update_all_tab_outputs(scale_and_size.output());

                    // Update RootData (for new tabs).
                    data.scale_and_size = Some(scale_and_size);
                }
            },

            // Detect and submit command for scale changes.
            //
            // TODO: This doesn't work :( This event is not received here, on Windows.
            Event::WindowScale(scale) => {
                tracing::debug!("Client area size: {:?}", ctx.size());
                tracing::debug!("Window scale: {:?}", scale);
                ctx.submit_command(SCALE_OR_SIZE_CHANGED.with(SingleUse::new((*scale, ctx.size()).into())));
            },

            _ => {},
        }

        self.image_widget.event(ctx, event, data, env)
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &RootData, env: &Env) {
        match event {
            LifeCycle::WidgetAdded => ctx.submit_command(INITIAL_SCALE_AND_SIZE.with(SingleUse::new((ctx.scale(), ctx.size()).into()))),
            LifeCycle::Size(size) => ctx.submit_command(SCALE_OR_SIZE_CHANGED.with(SingleUse::new((ctx.scale(), *size).into()))),
            _ => {},
        }

        self.image_widget.lifecycle(ctx, event, data, env)
    }

    fn update(&mut self, ctx: &mut UpdateCtx, old_data: &RootData, data: &RootData, env: &Env) {
        if !old_data.client_framebuffer.same(&data.client_framebuffer) {
            self.update_image(data);
            ctx.request_paint();
        }
        self.image_widget.update(ctx, data, env)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, data: &RootData, env: &Env) -> Size {
        self.image_widget.layout(ctx, &bc.loosen(), data, env);
        self.image_widget.set_origin(ctx, Point::ORIGIN);

        bc.max()
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &RootData, env: &Env) {
        self.image_widget.paint(ctx, data, env)
    }
}
