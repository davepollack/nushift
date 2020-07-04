use druid::widget::Flex;
use druid::{AppLauncher, WindowDesc, Widget, LocalizedString, Color};
use std::sync::Arc;

mod theme;
mod widgets;
mod widget_data;

use theme::TEXT_COLOR;
use widget_data::{RootData, TabData};

fn main() {
    let main_window = WindowDesc::new(build_root_widget)
        .title(LocalizedString::new("nushift"));

    let initial_state = RootData {
        tabs: Arc::new(vec![
            TabData { tab_title: "Tab title 1".into() },
            TabData { tab_title: "Tab title 2".into() },
        ])
    };

    AppLauncher::with_window(main_window)
        .use_simple_logger()
        .configure_env(|env, _| {
            env.set(TEXT_COLOR, Color::grey8(0x00));
        })
        .launch(initial_state)
        .expect("Launch failed");
}

fn build_root_widget() -> impl Widget<RootData> {
    Flex::column()
        .with_child(widgets::build_top_bar())
}
