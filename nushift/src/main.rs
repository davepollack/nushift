use druid::widget::{Flex};
use druid::{AppLauncher, WindowDesc, Widget, LocalizedString, Data, Lens};
use std::sync::Arc;

mod widgets {
    pub mod top_bar;
}

#[derive(Clone, Data, Lens)]
pub struct RootData {
    tabs: Arc<Vec<TabData>>
}

#[derive(Clone, Data, Lens)]
pub struct TabData {
    tab_title: String
}

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
        .with_child(widgets::top_bar::build_top_bar())
}
