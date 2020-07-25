use druid::{AppLauncher, WindowDesc, Widget, LocalizedString};
use druid::im::vector;
use druid::widget::Flex;
use std::sync::{Mutex, Arc};
use nushift_core::Hypervisor;

mod theme;
mod widget;
mod model;

use model::RootData;

fn main() {
    let main_window = WindowDesc::new(build_root_widget)
        .title(LocalizedString::new("nushift"));

    let hypervisor = Arc::new(Mutex::new(Hypervisor::new()));

    let mut initial_state = RootData {
        tabs: vector![],
        currently_selected_tab_id: None,
        hypervisor,
    };
    initial_state.add_new_tab();

    AppLauncher::with_window(main_window)
        .use_simple_logger()
        .launch(initial_state)
        .expect("Launch failed");
}

fn build_root_widget() -> impl Widget<RootData> {
    Flex::column()
        .with_child(widget::top_bar())
}
