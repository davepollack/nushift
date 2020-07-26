use druid::widget::prelude::*;
use druid::{widget::{Painter, SizedBox, Padding}, kurbo::BezPath, Color, Point, WidgetExt, Data};
use crate::{model::RootData, theme::{THICK_STROKE_ICON_COLOR_KEY, THIN_STROKE_ICON_COLOR_KEY}};
use super::value::TAB_HEIGHT;

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

pub fn new_tab_button() -> impl Widget<RootData> {
    let plus = Painter::<RootData>::new(|ctx, _, env| {
        let size = ctx.size();

        let mut path = BezPath::new();
        path.move_to((size.width / 2.0, 0.0));
        path.line_to((size.width / 2.0, size.height));
        path.move_to((0.0, size.height / 2.0));
        path.line_to((size.width, size.height / 2.0));

        ctx.stroke(path, &env.get(THICK_STROKE_ICON_COLOR_KEY), 2.0);
    });

    SizedBox::new(Padding::new((8.0, 5.5), plus))
        .width(TAB_HEIGHT + 5.0)
        .height(TAB_HEIGHT)
        .background(hover_background())
        .on_click(|_, data, _| {
            data.add_new_tab();
        })
}

pub fn close_button<T: Data>() -> impl Widget<T> {
    let cross = Painter::new(|ctx, _, env| {
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
        .on_click(|_, _, _| { /* TODO */ })
}
