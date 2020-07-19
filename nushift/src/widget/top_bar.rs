use druid::{Widget, WidgetExt, LocalizedString, Color};
use druid::widget::{Flex, Label, CrossAxisAlignment, FlexParams};

use crate::theme::{ICON_COLOR_KEY, TEXT_COLOR, ICON_COLOR};
use crate::model::RootData;
use super::tab_list::TabList;

const TOP_BAR_HEIGHT: f64 = 30.0;
const TOP_BAR_HORIZONTAL_PADDING: f64 = 10.0;

const TOP_BAR_BACKGROUND_COLOR: Color = Color::rgb8(0x82, 0xe0, 0xe0);

#[cfg(test)]
use mockall::{automock, predicate::*};
#[cfg_attr(test, automock)]
trait FlexTrait {
    fn row() -> Flex<RootData>;
}

impl FlexTrait for Flex<RootData> {
    fn row() -> Flex<RootData> { Flex::row() }
}

pub fn top_bar() -> impl Widget<RootData> {
    build_top_bar_internal::<Flex<RootData>>()
}

fn build_top_bar_internal<FlexI: FlexTrait>() -> impl Widget<RootData> {

    let tab_title = Label::new(LocalizedString::new("new-tab"))
        .expand_width();

    let tab_list = TabList::new()
        .lens(RootData::tabs)
        .expand_width();

    FlexI::row()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_bar_creates_one_flex_row() {
        let ctx = MockFlexTrait::row_context();
        ctx.expect()
            .times(1)
            .returning(|| Flex::row());
        build_top_bar_internal::<MockFlexTrait>();
    }
}
