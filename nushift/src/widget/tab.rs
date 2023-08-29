use druid::{
    text::ArcStr,
    widget::{Container, ControllerHost, Painter, Flex, MainAxisAlignment, Label},
    Color, RenderContext, WidgetExt, MouseButton,
};

use crate::model::RootAndTabData;
use super::{button, value, click_inverse::ClickInverse};

const TAB_BACKGROUND_COLOR: Color = Color::rgb8(0xa1, 0xf0, 0xf0);
const TAB_HOVER_BACKGROUND_COLOR: Color = Color::rgb8(0xbd, 0xf5, 0xf5);
const TAB_SELECTED_BACKGROUND_COLOR: Color = Color::rgb8(0xe9, 0xfc, 0xfc);

pub type Tab = ControllerHost<Container<RootAndTabData>, ClickInverse<RootAndTabData>>;

pub fn tab() -> Tab {
    let selected_or_non_selected_background = Painter::new(|ctx, data: &RootAndTabData, _| {
        let bounds = ctx.size().to_rect();

        match data.0.currently_selected_tab_id {
            Some(ref id) if *id == data.1.id => {
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
        .with_child(Label::new(|(_root, tab_data): &RootAndTabData, _env: &_| ArcStr::clone(&tab_data.title)).with_text_size(value::TAB_TEXT_SIZE))
        .with_child(button::close_button())
        .padding((value::TAB_HORIZONTAL_PADDING, 0.0))
        .background(selected_or_non_selected_background)
        .controller(ClickInverse::new(|_, mouse_event, (root_data, tab_data): &mut RootAndTabData, _| {
            match mouse_event.button {
                MouseButton::Left => {
                    root_data.select_tab(&tab_data.id);
                },
                MouseButton::Middle => {
                    root_data.close_tab(&tab_data.id);
                },
                _ => {},
            };
        }));

    tab
}
