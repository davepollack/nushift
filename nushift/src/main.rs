use druid::widget::{Align, Label};
use druid::{AppLauncher, WindowDesc, Widget, LocalizedString};

type RootType = ();

fn main() {
    let title = LocalizedString::<RootType>::new("nushift");

    let main_window = WindowDesc::new(build_root_widget)
        .title(title);

    AppLauncher::with_window(main_window)
        .use_simple_logger()
        .launch(())
        .expect("Launch failed");
}

fn build_root_widget() -> impl Widget<RootType> {
    let label = Label::new(LocalizedString::new("demo-hello"));

    Align::centered(label)
}
