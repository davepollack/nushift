use druid::{Widget, WidgetExt, Color};
use druid::widget::{Flex, Label, CrossAxisAlignment, FlexParams, SizedBox};
use std::sync::Arc;

use crate::theme::{TEXT_COLOR, THIN_STROKE_ICON_COLOR_KEY, THIN_STROKE_ICON_COLOR, THICK_STROKE_ICON_COLOR_KEY, THICK_STROKE_ICON_COLOR};
use crate::model::{TabData, RootData};
use super::{value, tab, button};

const TOP_BAR_BACKGROUND_COLOR: Color = Color::rgb8(0x82, 0xe0, 0xe0);

pub fn top_bar() -> impl Widget<RootData> {

    let main_title = Label::new(|root_data: &RootData, _env: &_| {
        let id_comparator = |tab_data: &&TabData| {
            Arc::ptr_eq(&tab_data.id, &root_data.currently_selected_tab_id)
        };
        match root_data.tabs.iter().find(id_comparator) {
            Some(tab_data) => tab_data.title.to_owned(),
            None => String::new(), // *Shrug* Invalid state?
        }
    }).expand_width();

    let new_tab_button = button::new_tab_button();

    let spacer = SizedBox::empty().width(2.5);

    let tab_list = tab::tab_list()
        .lens(RootData::tabs)
        .expand_width();

    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .with_flex_child(main_title, 2.0)
        .with_flex_child(new_tab_button, FlexParams::new(0.0, CrossAxisAlignment::End)) // Non-flex, but we want to align it
        .with_child(spacer)
        .with_flex_child(tab_list, FlexParams::new(3.0, CrossAxisAlignment::End))
        .fix_height(value::TOP_BAR_HEIGHT)
        .padding((value::TOP_BAR_HORIZONTAL_PADDING, 0.))
        .background(TOP_BAR_BACKGROUND_COLOR)
        .env_scope(|env, _| {
            env.set(druid::theme::LABEL_COLOR, TEXT_COLOR);
            env.set(THIN_STROKE_ICON_COLOR_KEY, THIN_STROKE_ICON_COLOR);
            env.set(THICK_STROKE_ICON_COLOR_KEY, THICK_STROKE_ICON_COLOR);
        })
}
