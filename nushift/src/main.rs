// On Windows, don't show a console when opening the app.
#![cfg_attr(not(test), windows_subsystem = "windows")]

use druid::{AppLauncher, WindowDesc, LocalizedString, Widget, Target};
use druid::im::vector;
use druid::{Color, widget::Flex};
use std::sync::{Mutex, Arc};
use nushift_core::{Hypervisor, HypervisorEventError};

mod model;
mod widget;
mod controller;
mod theme;
mod selector;
mod global_key_command_handler;

use self::model::RootData;
use self::selector::HYPERVISOR_EVENT;
use self::global_key_command_handler::GlobalKeyCommandHandler;

fn main() {
    let main_window = WindowDesc::new(build_root_widget())
        .title(LocalizedString::new("nushift"));

    let launcher = AppLauncher::with_window(main_window)
        .delegate(GlobalKeyCommandHandler::new())
        .log_to_console()
        .configure_env(|env, _| {
            env.set(druid::theme::WINDOW_BACKGROUND_COLOR, Color::grey(0.95));
        });

    let hypervisor_event_handler = {
        let event_sink = launcher.get_external_handle();
        move |hypervisor_event| {
            event_sink.submit_command(HYPERVISOR_EVENT, hypervisor_event, Target::Auto)
                .map_err(|_| HypervisorEventError::SubmitCommandError)
        }
    };

    let hypervisor = Arc::new(Mutex::new(Hypervisor::new(hypervisor_event_handler)));

    let root_data = RootData {
        tabs: vector![],
        currently_selected_tab_id: None,
        close_tab_requests: vector![],
        scale_and_size: None,
        client_framebuffer: None,
        hypervisor,
    };

    launcher
        .launch(root_data)
        .expect("Launch failed");
}

fn build_root_widget() -> impl Widget<RootData> {
    Flex::column()
        .with_child(widget::top_bar())
        .with_flex_child(widget::client_area::ClientArea::new(), 1.0)
}
