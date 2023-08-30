use druid::{widget::{Painter, SizedBox, Padding}, kurbo::BezPath, Point, RenderContext, Widget, WidgetExt};

use crate::model::RootAndTabData;
use crate::theme::THIN_STROKE_ICON_COLOR_KEY;
use crate::widget::click_inverse::ClickInverse;
use super::hover_background::{HoverParams, HoverBackground};

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

    HoverBackground::new(
        SizedBox::new(Padding::new(5.0, cross))
            .width(17.0)
            .height(17.0),
        HoverParams::default()
    )
        .controller(ClickInverse::new(|_, _, root_and_tab_data: &mut RootAndTabData, _| {
            let tab_data = root_and_tab_data.tab_data_cloned();
            root_and_tab_data.root_data_mut().close_tab(&tab_data.id);
        }))
}
