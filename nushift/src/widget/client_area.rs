use std::sync::Arc;

use druid::piet::{ImageFormat, InterpolationMode};
use druid::widget::{prelude::*, Image, FillStrat};
use druid::{SingleUse, WidgetPod, ImageBuf, Point};
use nushift_core::PresentBufferFormat;

use crate::model::RootData;
use crate::selector::{INITIAL_SCALE_AND_SIZE, SCALE_OR_SIZE_CHANGED};

pub struct ClientArea {
    image_widget: WidgetPod<RootData, Image>,
}

impl ClientArea {
    pub fn new() -> Self {
        // TODO: `FillStrat::ScaleDown` is a terrible workaround for not being able to draw a non-scaled image :(
        let image_widget = WidgetPod::new(Image::new(ImageBuf::empty())
            .fill_mode(FillStrat::ScaleDown)
            .interpolation_mode(InterpolationMode::NearestNeighbor));
        Self { image_widget }
    }

    fn update_image(&mut self, data: &RootData) {
        let img_buf = match data.scale_and_size {
            Some(ref scale_and_size) => {
                let gfx_output = scale_and_size.gfx_output();
                let (width, height) = (gfx_output.size_px()[0].try_into(), gfx_output.size_px()[1].try_into());

                match (width, height, data.currently_selected_tab_id.as_ref().and_then(|currently_selected_tab_id| data.get_tab_by_id(&currently_selected_tab_id))) {
                    (Ok(width), Ok(height), Some(tab_data)) => match tab_data.client_framebuffer {
                        // TODO: "Wrap" buffer? If not, then don't crash here
                        Some(ref client_framebuffer) => ImageBuf::from_raw(
                            Arc::clone(&client_framebuffer.framebuffer),
                            match client_framebuffer.present_buffer_format {
                                PresentBufferFormat::R8g8b8UintSrgb => ImageFormat::Rgb,
                            },
                            width,
                            height,
                        ),
                        None => ImageBuf::empty(),
                    },
                    _ => ImageBuf::empty(),
                }
            },
            _ => ImageBuf::empty(),
        };

        self.image_widget.widget_mut().set_image_data(img_buf);
    }
}

impl Widget<RootData> for ClientArea {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut RootData, env: &Env) {
        match event {
            // Handler for commands both from scale changes detected from the
            // root widget, and from initialisation and size changes from
            // `lifecycle`.
            Event::Command(cmd) => {
                let scale_and_size = match (cmd.get(INITIAL_SCALE_AND_SIZE), cmd.get(SCALE_OR_SIZE_CHANGED)) {
                    (Some(initial), _) => Some(initial),
                    (_, Some(changed)) => Some(changed),
                    _ => None,
                };

                if let Some(scale_and_size) = scale_and_size.and_then(SingleUse::take) {
                    // Update all existing tabs.
                    data.hypervisor.lock().unwrap().update_all_tab_gfx_outputs(scale_and_size.gfx_output());

                    // Update RootData (for new tabs).
                    data.scale_and_size = Some(scale_and_size);
                }
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
        let old_framebuffer_currently_selected_tab = data.currently_selected_tab_id.as_ref()
            .and_then(|tab_id| old_data.get_tab_by_id(&tab_id))
            .map(|tab_data| &tab_data.client_framebuffer);

        let new_framebuffer_currently_selected_tab = data.currently_selected_tab_id.as_ref()
            .and_then(|tab_id| data.get_tab_by_id(&tab_id))
            .map(|tab_data| &tab_data.client_framebuffer);

        let currently_selected_tab_framebuffer_same = match (old_framebuffer_currently_selected_tab, new_framebuffer_currently_selected_tab) {
            (Some(old_option_framebuffer), Some(new_option_framebuffer)) => old_option_framebuffer.same(new_option_framebuffer),
            (None, None) => true,
            _ => false,
        };

        // If the currently selected tab has changed, then update the client area.
        //
        // Else if, the client framebuffer for the currently selected tab has
        // changed, then update the client area. The bindings
        // `old_framebuffer_currently_selected_tab`,
        // `new_framebuffer_currently_selected_tab`,
        // `currently_selected_tab_framebuffer_same` are only for this else if
        // case.
        if old_data.currently_selected_tab_id != data.currently_selected_tab_id
            || !currently_selected_tab_framebuffer_same
        {
            self.update_image(data);
            ctx.request_layout();
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
