// On Windows, don't show a console when opening the app.
#![cfg_attr(not(test), windows_subsystem = "windows")]

use druid::{AppLauncher, WindowDesc, LocalizedString, Widget, Target, ExtEventSink};
use druid::im::vector;
use druid::{Color, widget::Flex};
use selectors::HypervisorTitleChangePayload;
use std::sync::{Mutex, Arc};
use nushift_core::{Hypervisor, HypervisorEvent};

mod theme;
mod widget;
mod model;
mod selectors;

use self::model::RootData;
use self::selectors::HYPERVISOR_TITLE_CHANGE;

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
            hypervisor_event_handler_with_event_sink(&event_sink, hypervisor_event);
        }
    };

    let hypervisor = Arc::new(Mutex::new(Hypervisor::new(hypervisor_event_handler)));

    let initial_state = RootData {
        tabs: vector![],
        currently_selected_tab_id: None,
        hypervisor,
    };

    launcher
        .launch(initial_state)
        .expect("Launch failed");
}

fn build_root_widget() -> impl Widget<RootData> {
    Flex::column().with_child(widget::top_bar())
}

fn hypervisor_event_handler_with_event_sink(event_sink: &ExtEventSink, hypervisor_event: HypervisorEvent) {
    let (selector, payload) = match hypervisor_event {
        HypervisorEvent::TitleChange(tab_id, new_title) => (HYPERVISOR_TITLE_CHANGE, HypervisorTitleChangePayload { tab_id, new_title }),
    };
    // TODO: Use error
    event_sink.submit_command(selector, payload, Target::Auto);
    // TODO: Observe command in tab title (label?) widget
}
