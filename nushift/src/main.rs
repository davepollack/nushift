// On Windows, don't show a console when opening the app.
#![cfg_attr(not(test), windows_subsystem = "windows")]

use druid::{AppLauncher, WindowDesc, LocalizedString, Widget, Target};
use druid::im::vector;
use druid::{Color, widget::Flex};
use std::sync::{Mutex, Arc};
use nushift_core::Hypervisor;

mod theme;
mod widget;
mod model;
mod selectors;

use self::model::RootData;
use self::selectors::HYPERVISOR_EVENT;

fn main() {
    let main_window = WindowDesc::new(build_root_widget())
        .title(LocalizedString::new("nushift"));

    let launcher = AppLauncher::with_window(main_window)
        .log_to_console()
        .configure_env(|env, _| {
            env.set(druid::theme::WINDOW_BACKGROUND_COLOR, Color::grey(0.95));
        });

    let hypervisor_event_handler = {
        let event_sink = launcher.get_external_handle();
        move |hypervisor_event| {
            // TODO: Use error
            event_sink.submit_command(HYPERVISOR_EVENT, hypervisor_event, Target::Auto);
        }
    };

    let hypervisor = Arc::new(Mutex::new(Hypervisor::new(hypervisor_event_handler)));

    let root_data = RootData {
        tabs: vector![],
        currently_selected_tab_id: None,
        close_tab_requests: vector![],
        hypervisor,
    };

    launcher
        .launch(root_data)
        .expect("Launch failed");
}

fn build_root_widget() -> impl Widget<RootData> {
    Flex::column().with_child(widget::top_bar())
}
