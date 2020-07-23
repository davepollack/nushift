use druid::widget::Flex;
use druid::{AppLauncher, WindowDesc, Widget, LocalizedString};
use std::sync::Arc;
use nushift_core::Hypervisor;

mod theme;
mod widget;
mod model;

use model::{RootData, TabData};

const INITIAL_TAB_TITLE: &str = "Tab title 1";

fn main() {
    let main_window = WindowDesc::new(build_root_widget)
        .title(LocalizedString::new("nushift"));

    let mut hypervisor = Hypervisor::new();
    let tab_id = hypervisor.add_new_tab(INITIAL_TAB_TITLE);

    let initial_state = RootData {
        tabs: Arc::new(vec![
            TabData { id: tab_id, title: INITIAL_TAB_TITLE.into() },
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
