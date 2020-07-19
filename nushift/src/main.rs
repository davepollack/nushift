use druid::widget::Flex;
use druid::{AppLauncher, WindowDesc, Widget, LocalizedString};
use std::sync::Arc;

mod theme;
mod widget;
mod model;

use model::{RootData, TabData};

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
        .launch(initial_state)
        .expect("Launch failed");
}

fn build_root_widget() -> impl Widget<RootData> {
    Flex::column()
        .with_child(widget::top_bar())
}
