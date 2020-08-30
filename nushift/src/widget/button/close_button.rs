use druid::{widget::{Painter, SizedBox, Padding}, kurbo::BezPath, Color, Point, RenderContext, Widget, WidgetExt};

use crate::model::RootAndTabData;
use crate::theme::THIN_STROKE_ICON_COLOR_KEY;
use crate::widget::click_inverse::ClickInverse;

// TODO delete
fn hover_background<T>() -> Painter<T> {
    Painter::new(|ctx, _, _| {
        let bounds = ctx.size().to_rect();

        if ctx.is_hot() {
            ctx.fill(bounds, &Color::rgba(0., 0., 0., 0.1));
        }

        if ctx.is_active() {
            ctx.fill(bounds, &Color::rgba(0., 0., 0., 0.16));
        }
    })
}

pub fn close_button() -> impl Widget<RootAndTabData> {
    let cross = Painter::new(|ctx, _: &RootAndTabData, env| {
        let size = ctx.size();

        let mut path = BezPath::new();
        path.move_to(Point::ORIGIN);
        path.line_to((size.width, size.height));
        path.move_to((size.width, 0.0));
        path.line_to((0.0, size.height));

        ctx.stroke(path, &env.get(THIN_STROKE_ICON_COLOR_KEY), 1.5);
    });

    SizedBox::new(Padding::new(5.0, cross))
        .width(17.0)
        .height(17.0)
        .background(hover_background())
        .controller(ClickInverse::new(|_, _, (root_data, tab_data): &mut RootAndTabData, _| {
            root_data.close_tab(&tab_data.id);
        }))
}