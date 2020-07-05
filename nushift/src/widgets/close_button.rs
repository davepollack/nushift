use druid::widget::prelude::*;
use druid::{widget::{Painter, SizedBox, Padding}, kurbo::BezPath, Color, Point, WidgetExt, Data};
use crate::theme::ICON_COLOR_KEY;

pub fn close_button<T: Data>() -> impl Widget<T> {
    let hover_background = Painter::new(|ctx, _, _| {
        let bounds = ctx.size().to_rect();

        if ctx.is_hot() {
            ctx.fill(bounds, &Color::rgba(0., 0., 0., 0.1));
        }

        if ctx.is_active() {
            ctx.fill(bounds, &Color::rgba(0., 0., 0., 0.16));
        }
    });

    let cross = Painter::new(|ctx, _, env| {
        let size = ctx.size();

        let mut path = BezPath::new();
        path.move_to(Point::ORIGIN);
        path.line_to((size.width, size.height));
        path.move_to((size.width, 0.0));
        path.line_to((0.0, size.height));

        ctx.stroke(path, &env.get(ICON_COLOR_KEY), 1.5);
    });

    SizedBox::new(
        Padding::new(5.0, cross)
    )
        .width(17.0)
        .height(17.0)
        .background(hover_background)
        .on_click(|_, _, _| { /* TODO */ })
}
