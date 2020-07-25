use druid::{Widget, WidgetExt, Color};
use druid::widget::{Flex, Label, CrossAxisAlignment, FlexParams};
use std::sync::Arc;

use crate::theme::{ICON_COLOR_KEY, TEXT_COLOR, ICON_COLOR};
use crate::model::{TabData, RootData};
use super::tab_list::TabList;

const TOP_BAR_HEIGHT: f64 = 30.0;
const TOP_BAR_HORIZONTAL_PADDING: f64 = 10.0;

const TOP_BAR_BACKGROUND_COLOR: Color = Color::rgb8(0x82, 0xe0, 0xe0);

pub fn top_bar() -> impl Widget<RootData> {

    let tab_title = Label::new(|root_data: &RootData, _env: &_| {
        let id_comparator = |tab_data: &&TabData| {
            Arc::ptr_eq(&tab_data.id, &root_data.currently_selected_tab_id)
        };
        match root_data.tabs.iter().find(id_comparator) {
            Some(tab_data) => tab_data.title.to_owned(),
            None => String::new(), // *Shrug* Invalid state?
        }
    }).expand_width();

    let tab_list = TabList::new()
        .lens(RootData::tabs)
        .expand_width();

    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .with_flex_child(tab_title, 2.0)
        .with_flex_child(tab_list, FlexParams::new(3.0, CrossAxisAlignment::End))
        .fix_height(TOP_BAR_HEIGHT)
        .padding((TOP_BAR_HORIZONTAL_PADDING, 0.))
        .background(TOP_BAR_BACKGROUND_COLOR)
        .env_scope(|env, _| {
            env.set(druid::theme::LABEL_COLOR, TEXT_COLOR);
            env.set(ICON_COLOR_KEY, ICON_COLOR);
        })
}
