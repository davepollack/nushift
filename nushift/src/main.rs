// On Windows, don't show a console when opening the app.
#![cfg_attr(not(test), windows_subsystem = "windows")]

use druid::{AppLauncher, WindowDesc, LocalizedString};
use druid::im::vector;
use druid::{Color, widget::Flex};
use std::sync::{Mutex, Arc};
use nushift_core::Hypervisor;

mod theme;
mod widget;
mod model;

use self::model::RootData;

fn main() {
    let main_window = WindowDesc::new(
        Flex::column().with_child(widget::top_bar())
    )
        .title(LocalizedString::new("nushift"));

    let hypervisor = Arc::new(Mutex::new(Hypervisor::new()));

    let initial_state = RootData {
        tabs: vector![],
        currently_selected_tab_id: None,
        hypervisor,
    };

    AppLauncher::with_window(main_window)
        .log_to_console()
        .configure_env(|env, _| {
            env.set(druid::theme::WINDOW_BACKGROUND_COLOR, Color::grey(0.95));
        })
        .launch(initial_state)
        .expect("Launch failed");
}
