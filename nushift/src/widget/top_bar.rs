use druid::{
    Env, LocalizedString, Widget, WidgetExt,
    text::ArcStr,
    widget::{Flex, Label, CrossAxisAlignment, FlexParams}
};

use crate::theme::{TEXT_COLOR, THIN_STROKE_ICON_COLOR_KEY, THIN_STROKE_ICON_COLOR, THICK_STROKE_ICON_COLOR_KEY, THICK_STROKE_ICON_COLOR};
use crate::model::RootData;
use super::{value, tab_list, button};

pub fn top_bar() -> impl Widget<RootData> {
    let main_title = Label::new(|root_data: &RootData, env: &Env| {
        match root_data.currently_selected_tab_id {
            Some(ref id) => {
                root_data.get_tab_by_id(id)
                    .map(|tab_data| ArcStr::clone(&tab_data.title))
                    .unwrap_or_else(|| "".into())
            },
            None => {
                let mut no_tabs = LocalizedString::new("nushift-no-tabs");
                no_tabs.resolve(root_data, env);
                no_tabs.localized_str()
            },
        }
    })
        .with_text_size(value::TOP_BAR_TEXT_SIZE)
        .expand_width();

    let new_tab_button = button::new_tab_button();

    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::End)
        .with_flex_child(main_title, FlexParams::new(2.0, CrossAxisAlignment::Center))
        .with_child(new_tab_button)
        .with_spacer(2.5)
        .with_flex_child(tab_list::tab_list(), 3.0)
        .fix_height(value::TOP_BAR_HEIGHT)
        .padding((value::TOP_BAR_HORIZONTAL_PADDING, 0.))
        .background(value::TOP_BAR_BACKGROUND_COLOR)
        .env_scope(|env, _| {
            env.set(druid::theme::TEXT_COLOR, TEXT_COLOR);
            env.set(THIN_STROKE_ICON_COLOR_KEY, THIN_STROKE_ICON_COLOR);
            env.set(THICK_STROKE_ICON_COLOR_KEY, THICK_STROKE_ICON_COLOR);
        })
}
