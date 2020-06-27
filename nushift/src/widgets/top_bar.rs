use druid::{Widget, WidgetExt, LocalizedString, Color};
use druid::widget::{Flex, Label, CrossAxisAlignment, MainAxisAlignment};

use crate::widget_data::RootData;

const TOP_BAR_HEIGHT: f64 = 30.0;
const TOP_BAR_HORIZONTAL_PADDING: f64 = 10.0;

const TOP_BAR_BACKGROUND_COLOR: Color = Color::rgb8(0x82, 0xe0, 0xe0);
const TOP_BAR_TEXT_COLOR: Color = Color::grey8(0x00);

const TAB_BACKGROUND_COLOR: Color = Color::rgb8(0xa1, 0xf0, 0xf0);
const TAB_HEIGHT: f64 = 20.0;
const TAB_MAX_WIDTH: f64 = 200.0;

#[cfg(test)]
use mockall::{automock, predicate::*};
#[cfg_attr(test, automock)]
trait FlexTrait {
    fn row() -> Flex<RootData>;
}

impl FlexTrait for Flex<RootData> {
    fn row() -> Flex<RootData> { Flex::row() }
}

pub fn build_top_bar() -> impl Widget<RootData> {
    build_top_bar_internal::<Flex<RootData>>()
}

fn build_top_bar_internal<FlexI: FlexTrait>() -> impl Widget<RootData> {

    let tab_title = Label::new(LocalizedString::new("new-tab"))
        .with_text_color(TOP_BAR_TEXT_COLOR);

    let mut tab_bar = FlexI::row()
        .cross_axis_alignment(CrossAxisAlignment::End);

    // TODO how to use the data?
    tab_bar.add_child(build_tab());
    tab_bar.add_child(build_tab());

    FlexI::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .with_flex_child(tab_title, 2.0)
        .with_flex_child(tab_bar, 3.0)
        .fix_height(TOP_BAR_HEIGHT)
        .padding((TOP_BAR_HORIZONTAL_PADDING, 0.))
        .background(TOP_BAR_BACKGROUND_COLOR)
}

fn build_tab() -> impl Widget<RootData> {
    Flex::row()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_child(Label::new("Tab title 1"))
        .with_child(Label::new("x"))
        .fix_width(TAB_MAX_WIDTH)
        .fix_height(TAB_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_top_bar_creates_two_flex_rows() {
        let ctx = MockFlexTrait::row_context();
        ctx.expect()
            .times(2)
            .returning(|| Flex::row());
        build_top_bar_internal::<MockFlexTrait>();
    }
}
