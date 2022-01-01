use druid::{Widget, WidgetExt};
use druid::{lens, Env, LensExt, LocalizedString, widget::{Flex, Label, CrossAxisAlignment, FlexParams}};
use reusable_id_pool::IdEq;

use crate::theme::{TEXT_COLOR, THIN_STROKE_ICON_COLOR_KEY, THIN_STROKE_ICON_COLOR, THICK_STROKE_ICON_COLOR_KEY, THICK_STROKE_ICON_COLOR};
use crate::model::{RootAndVectorTabData, RootData};
use super::{value, tab_list, button};

pub fn top_bar() -> impl Widget<RootData> {

    let main_title = Label::new(|root_data: &RootData, env: &Env| {
        match &root_data.currently_selected_tab_id {
            Some(id) => match root_data.tabs.iter().find(|tab_data| tab_data.id.id_eq(&id)) {
                Some(tab_data) => tab_data.title.to_string(),
                None => String::new(),
            },
            None => {
                let mut no_tabs = LocalizedString::new("nushift-no-tabs");
                no_tabs.resolve(root_data, env);
                no_tabs.localized_str().to_string()
            },
        }
    })
        .with_text_size(value::TOP_BAR_TEXT_SIZE)
        .expand_width();

    let new_tab_button = button::new_tab_button();

    let tab_list = tab_list::tab_list()
        .lens(lens::Identity.map(
            // Add root data as shared data, so tabs can call `close_tab()` on the root data
            |root_data: &RootData| (root_data.clone(), root_data.tabs.clone()),
            |root_data: &mut RootData, new_data: RootAndVectorTabData| {
                *root_data = new_data.0;
            }
        ))
        .expand_width();

    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .with_flex_child(main_title, 2.0)
        .with_flex_child(new_tab_button, FlexParams::new(0.0, CrossAxisAlignment::End)) // Non-flex, but we want to align it
        .with_spacer(2.5)
        .with_flex_child(tab_list, FlexParams::new(3.0, CrossAxisAlignment::End))
        .fix_height(value::TOP_BAR_HEIGHT)
        .padding((value::TOP_BAR_HORIZONTAL_PADDING, 0.))
        .background(value::TOP_BAR_BACKGROUND_COLOR)
        .env_scope(|env, _| {
            env.set(druid::theme::LABEL_COLOR, TEXT_COLOR);
            env.set(THIN_STROKE_ICON_COLOR_KEY, THIN_STROKE_ICON_COLOR);
            env.set(THICK_STROKE_ICON_COLOR_KEY, THICK_STROKE_ICON_COLOR);
        })
}
