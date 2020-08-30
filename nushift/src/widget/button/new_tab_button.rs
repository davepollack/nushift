use druid::{widget::{Painter, SizedBox, Padding}, kurbo::BezPath, Color, RenderContext, Widget};

use crate::model::RootData;
use crate::theme::THICK_STROKE_ICON_COLOR_KEY;
use crate::widget::value::TAB_HEIGHT;
use super::hover_transition::HoverBackground;

pub fn new_tab_button() -> impl Widget<RootData> {
    let plus = Painter::new(|ctx, _: &RootData, env| {
        let size = ctx.size();

        let mut path = BezPath::new();
        path.move_to((size.width / 2.0, 0.0));
        path.line_to((size.width / 2.0, size.height));
        path.move_to((0.0, size.height / 2.0));
        path.line_to((size.width, size.height / 2.0));

        ctx.stroke(path, &env.get(THICK_STROKE_ICON_COLOR_KEY), 2.0);
    });

    // TODO remove when click/active is working
    // SizedBox::new(Padding::new((8.0, 5.5), plus))
    //     .width(TAB_HEIGHT + 5.0)
    //     .height(TAB_HEIGHT)
    //     .background(hover_background())
    //     .on_click(|_, root_data, _| {
    //         root_data.add_new_tab();
    //     })
    HoverBackground::new(
        Color::grey(0.0), 0.0, 0.1,
        super::hover_transition::default_easing_function(),
        super::hover_transition::default_easing_function(),
        0.07,
        SizedBox::new(Padding::new((8.0, 5.5), plus))
            .width(TAB_HEIGHT + 5.0)
            .height(TAB_HEIGHT)
    )
}