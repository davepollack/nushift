use druid::widget::{Align, Label};
use druid::{AppLauncher, WindowDesc, Widget};

fn main() {
    let main_window = WindowDesc::new(build_root_widget)
        .title("GUI demo");

    AppLauncher::with_window(main_window)
        .use_simple_logger()
        .launch(())
        .expect("Launch failed");
}

fn build_root_widget() -> impl Widget<()> {
    let label = Label::new("Hello");

    Align::centered(label)
}
