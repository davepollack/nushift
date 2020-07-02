use druid::{
    widget::{ListIter, Flex, Label, MainAxisAlignment},
    // TODO import widget prelude instead of these imports?
    WidgetPod, Widget, WidgetExt, Env, EventCtx, Event, LifeCycleCtx,
    LifeCycle, UpdateCtx, LayoutCtx, BoxConstraints, Size, Rect, Point, PaintCtx,
};

use crate::widget_data::TabData;

const TAB_HEIGHT: f64 = 20.0;
const TAB_MAX_WIDTH: f64 = 200.0;

pub struct TabList {
    children: Vec<WidgetPod<TabData, Box<dyn Widget<TabData>>>>,
}

impl TabList {
    pub fn new() -> Self {
        TabList { children: Vec::new() }
    }

    /// This recreates all children, which is not the greatest, but doing it for
    /// now until we have child tracking.
    fn recreate_children(&mut self, data: &impl ListIter<TabData>, _env: &Env) {
        self.children.clear();
        data.for_each(|_, _| {
            self.children.push(WidgetPod::new(Box::new(build_tab())));
        });
    }
}

fn build_tab() -> impl Widget<TabData> {
    Flex::row()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_child(Label::new(|tab_data: &TabData, _env: &_| {
            tab_data.tab_title.to_owned()
        }))
        .with_child(Label::new("x"))
}


impl<T: ListIter<TabData>> Widget<T> for TabList {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        let mut children = self.children.iter_mut();
        data.for_each_mut(|child_data, _| {
            if let Some(child) = children.next() {
                child.event(ctx, event, child_data, env);
            }
        });
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &T, env: &Env) {
        if let LifeCycle::WidgetAdded = event {
            self.recreate_children(data, env);
            ctx.children_changed();
        }

        let mut children = self.children.iter_mut();
        data.for_each(|child_data, _| {
            if let Some(child) = children.next() {
                child.lifecycle(ctx, event, child_data, env);
            }
        });
    }

    fn update(&mut self, ctx: &mut UpdateCtx, old_data: &T, data: &T, env: &Env) {
        // Recreate everything. This is not good (FIXME), we should actually send
        // `update` to the children like in widget::List, but doing this for now
        // until we have child tracking.
        self.recreate_children(data, env);
        ctx.children_changed();
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, data: &T, env: &Env) -> Size {
        let tab_width = if data.data_len() == 0 {
            0.0
        } else if TAB_MAX_WIDTH * (data.data_len() as f64) > bc.max().width {
            bc.max().width / (data.data_len() as f64)
        } else {
            TAB_MAX_WIDTH
        };
        let tab_height = TAB_HEIGHT.min(bc.max().height);

        let mut children = self.children.iter_mut();
        let mut max_height_seen = bc.min().height;
        data.for_each(|child_data, i| {
            let child = match children.next() {
                Some(child) => child,
                None => {
                    return;
                },
            };

            let child_bc = BoxConstraints::new(
                Size::new(tab_width, bc.min().height),
                Size::new(tab_width, tab_height),
            );

            let child_size = child.layout(ctx, &child_bc, child_data, env);
            let bottom_aligned_origin = Point::new((i as f64) * tab_width, bc.max().height - tab_height);
            dbg!(bottom_aligned_origin, tab_height, bc.min(), bc.max(), child_size);
            let rect = Rect::from_origin_size(bottom_aligned_origin, child_size);
            child.set_layout_rect(ctx, child_data, env, rect);
            max_height_seen = max_height_seen.max(child_size.height);
        });

        let my_size = Size::new((data.data_len() as f64) * tab_width, max_height_seen);
        my_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &T, env: &Env) {
        let mut children = self.children.iter_mut();
        data.for_each(|child_data, _| {
            if let Some(child) = children.next() {
                child.paint(ctx, child_data, env);
            }
        });
    }
}
