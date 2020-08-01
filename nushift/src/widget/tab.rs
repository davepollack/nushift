use druid::widget::prelude::*;
use druid::{
    widget::{ListIter, Flex, Label, MainAxisAlignment, EnvScope, Container},
    WidgetPod, Widget, WidgetExt, Point, Rect, Color,
};
use std::cmp::Ordering;

use crate::model::{TabListAndSharedRootData, TabAndSharedRootData};
use super::{value, button};

const TAB_BACKGROUND_COLOR: Color = Color::rgb8(0xa1, 0xf0, 0xf0);
const TAB_MAX_WIDTH: f64 = 200.0;

type Tab = EnvScope<TabAndSharedRootData, Container<TabAndSharedRootData>>;

fn tab() -> Tab {
    let tab = Flex::row()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_child(Label::new(|(_root, tab_data): &TabAndSharedRootData, _env: &_| tab_data.title.to_owned()))
        .with_child(button::close_button())
        .padding((value::TAB_HORIZONTAL_PADDING, 0.0))
        .background(TAB_BACKGROUND_COLOR);

    tab.debug_paint_layout()
}

pub fn tab_list() -> TabList {
    TabList::new()
}

pub struct TabList {
    children: Vec<WidgetPod<TabAndSharedRootData, Tab>>,
}

/// Copy of druid::widget::List, but changed the `layout()` method.
impl TabList {
    fn new() -> Self {
        TabList { children: Vec::new() }
    }

    /// When the widget is created or the data changes, create or remove children as needed
    ///
    /// Returns `true` if children were added or removed.
    fn update_child_count<T>(&mut self, data: &impl ListIter<T>, _env: &Env) -> bool {
        let len = self.children.len();
        match len.cmp(&data.data_len()) {
            Ordering::Greater => self.children.truncate(data.data_len()),
            Ordering::Less => data.for_each(|_, i| {
                if i >= len {
                    self.children.push(WidgetPod::new(tab()));
                }
            }),
            Ordering::Equal => (),
        }
        len != data.data_len()
    }
}

/// Copy of druid::widget::List, but changed the `layout()` method.
impl Widget<TabListAndSharedRootData> for TabList {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut TabListAndSharedRootData, env: &Env) {
        let mut children = self.children.iter_mut();
        data.for_each_mut(|child_data, _| {
            if let Some(child) = children.next() {
                child.event(ctx, event, child_data, env);
            }
        });
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &TabListAndSharedRootData, env: &Env) {
        if let LifeCycle::WidgetAdded = event {
            if self.update_child_count(data, env) {
                ctx.children_changed();
            }
        }

        let mut children = self.children.iter_mut();
        data.for_each(|child_data, _| {
            if let Some(child) = children.next() {
                child.lifecycle(ctx, event, child_data, env);
            }
        });
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &TabListAndSharedRootData, data: &TabListAndSharedRootData, env: &Env) {
        let mut children = self.children.iter_mut();
        data.for_each(|child_data, _| {
            if let Some(child) = children.next() {
                child.update(ctx, child_data, env);
            }
        });

        if self.update_child_count(data, env) {
            ctx.children_changed();
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, data: &TabListAndSharedRootData, env: &Env) -> Size {
        let tab_width = if data.data_len() == 0 {
            0.0
        } else if TAB_MAX_WIDTH * (data.data_len() as f64) > bc.max().width {
            bc.max().width / (data.data_len() as f64)
        } else {
            TAB_MAX_WIDTH
        };
        let tab_height = value::TAB_HEIGHT.min(bc.max().height);

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

    fn paint(&mut self, ctx: &mut PaintCtx, data: &TabListAndSharedRootData, env: &Env) {
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

    #[test]
    fn tab_list_new_creates_widget_with_empty_vec() {
        let tab_list = TabList::new();
        assert!(tab_list.children.is_empty());
    }
}
