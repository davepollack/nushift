use druid::{
    widget::{Container, ControllerHost, Click, Painter, Flex, MainAxisAlignment, Label},
    Color, RenderContext, WidgetExt,
};
use nushift_core::IdEq;

use crate::model::RootAndTabData;
use super::{button, value};

const TAB_BACKGROUND_COLOR: Color = Color::rgb8(0xa1, 0xf0, 0xf0);
const TAB_HOVER_BACKGROUND_COLOR: Color = Color::rgb8(0xbd, 0xf5, 0xf5);
const TAB_SELECTED_BACKGROUND_COLOR: Color = Color::rgb8(0xe9, 0xfc, 0xfc);

pub type Tab = ControllerHost<Container<RootAndTabData>, Click<RootAndTabData>>;

pub fn tab() -> Tab {
    let selected_or_non_selected_background = Painter::new(|ctx, data: &RootAndTabData, _| {
        let bounds = ctx.size().to_rect();
        match &data.0.currently_selected_tab_id {
            Some(id) if id.id_eq(&data.1.id) => {
                ctx.fill(bounds, &TAB_SELECTED_BACKGROUND_COLOR);
            },
            _ => {
                if ctx.is_hot() {
                    ctx.fill(bounds, &TAB_HOVER_BACKGROUND_COLOR);
                } else {
                    ctx.fill(bounds, &TAB_BACKGROUND_COLOR);
                }
            },
        }
    });

    let tab = Flex::row()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_child(Label::new(|(_root, tab_data): &RootAndTabData, _env: &_| tab_data.title.to_owned()))
        .with_child(button::close_button())
        .padding((value::TAB_HORIZONTAL_PADDING, 0.0))
        .background(selected_or_non_selected_background)
        .on_click(|_ctx, _data, _env| {
            // Attach `Click` widget to get "hot" tracking and other useful
            // mouse handling, but don't actually use it for the select handler,
            // we're going to do that ourselves.
        });

    tab
}
