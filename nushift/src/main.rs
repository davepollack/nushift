use druid::widget::{Flex};
use druid::{AppLauncher, WindowDesc, Widget, LocalizedString, Data, Lens};

mod widgets {
    pub mod top_bar;
}

#[derive(Clone, Data, Lens)]
pub struct RootData {
    number_of_tabs: u32
}

fn main() {
    let main_window = WindowDesc::new(build_root_widget)
        .title(LocalizedString::new("nushift"));

    let initial_state = RootData {
        number_of_tabs: 0
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
