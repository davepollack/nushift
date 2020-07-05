use druid::widget::prelude::*;
use druid::{
    widget::{ListIter, Flex, Label, MainAxisAlignment, Container},
    WidgetPod, Widget, WidgetExt, Point, Rect, Color
};

use crate::widget_data::TabData;
use super::close_button;

const TAB_BACKGROUND_COLOR: Color = Color::rgb8(0xa1, 0xf0, 0xf0);
const TAB_HEIGHT: f64 = 25.0;
const TAB_MAX_WIDTH: f64 = 200.0;

type Tab = Container<TabData>;

fn build_tab() -> Tab {
    Flex::row()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_child(
            Label::new(|tab_data: &TabData, _env: &_| {
                tab_data.tab_title.to_owned()
            })
        )
        .with_child(close_button::close_button())
        .padding((3.0, 0.0))
        .background(TAB_BACKGROUND_COLOR)
}

pub struct TabList {
    children: Vec<WidgetPod<TabData, Tab>>,
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
            self.children.push(WidgetPod::new(build_tab()));
        });
    }
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

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &T, data: &T, env: &Env) {
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
                Size::new(tab_width, tab_height),
                Size::new(tab_width, tab_height),
            );

            let child_size = child.layout(ctx, &child_bc, child_data, env);
            let origin = Point::new((i as f64) * tab_width, 0.0);
            let rect = Rect::from_origin_size(origin, child_size);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn tab_list_new_creates_widget_with_empty_vec() {
        let tab_list = TabList::new();
        assert!(tab_list.children.is_empty());
    }

    #[test]
    fn tab_list_recreate_children_clears_the_vec_and_adds_children() {
        let mut tab_list = TabList::new();
        for _ in 0..5 {
            tab_list.children.push(WidgetPod::new(build_tab()));
        }
        let child_datas = Arc::new(
            vec![TabData { tab_title: "Title 1".into() }, TabData { tab_title: "Title 2".into() }]
        );
        tab_list.recreate_children(&child_datas, &Env::default());

        // Data length is 2, so it should clear the 5 widgets we added and only add back 2.
        assert_eq!(2, tab_list.children.len());
    }
}
