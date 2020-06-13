use druid::{Widget, WidgetExt, LocalizedString, Color};
use druid::widget::{Flex, Label};

use crate::RootData;

const TOP_BAR_HEIGHT: f64 = 30.0;
const TOP_BAR_HORIZONTAL_PADDING: f64 = 10.0;

pub fn build_top_bar() -> impl Widget<RootData> {

    let tab_title = Label::new(LocalizedString::new("new-tab"))
        .with_text_color(Color::grey8(0x00));

    let tab_bar = Flex::row();

    Flex::row()
        .with_flex_child(tab_title, 2.0)
        .with_flex_child(tab_bar, 3.0)
        .fix_height(TOP_BAR_HEIGHT)
        .padding((TOP_BAR_HORIZONTAL_PADDING, 0.))
        .background(Color::rgb8(0x82, 0xe0, 0xe0))
}
