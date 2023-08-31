use druid::widget::prelude::*;
use druid::{widget::ListIter, WidgetPod, Widget, Point};
use reusable_id_pool::ArcId;
use std::collections::{HashSet, HashMap};

use crate::model::{RootData, RootAndTabData};
use super::{tab, value};

const TAB_NORMAL_WIDTH: f64 = 200.0;

pub fn tab_list() -> TabList {
    TabList::new()
}

/// Based on druid::widget::List, but has more child tracking, and custom
/// `layout()` method
pub struct TabList {
    widget_children: HashMap<ArcId, WidgetPod<RootAndTabData, Box<dyn Widget<RootAndTabData>>>>,
}

impl TabList {
    fn new() -> Self {
        TabList { widget_children: HashMap::new() }
    }

    /// When the widget is created or the data changes, create or remove children
    /// as needed.
    ///
    /// Returns `true` if children were added or removed.
    fn create_and_remove_widget_children(&mut self, root_data: &RootData, _env: &Env) -> bool {
        let mut is_changed = false;
        let original_widget_children_len = self.widget_children.len();

        let mut data_ids_set = HashSet::with_capacity(root_data.data_len());
        root_data.for_each(|root_and_tab_data, _| {
            data_ids_set.insert(ArcId::clone(&root_and_tab_data.tab_data().id));
        });

        // Wipe all widget children that are no longer in the data
        self.widget_children.retain(|tab_key: &ArcId, _| data_ids_set.contains(tab_key));

        if self.widget_children.len() != original_widget_children_len {
            is_changed = true;
        }

        // Add new widget children corresponding to new IDs
        for tab_key in data_ids_set {
            if !self.widget_children.contains_key(&tab_key) {
                self.widget_children.insert(tab_key, WidgetPod::new(Box::new(tab::tab())));
                is_changed = true;
            }
        }

        is_changed
    }
}

impl Widget<RootData> for TabList {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, root_data: &mut RootData, env: &Env) {
        root_data.for_each_mut(|root_and_tab_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&root_and_tab_data.tab_data().id) {
                widget_child.event(ctx, event, root_and_tab_data, env);
            }
        });
        root_data.process_close_requests();
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, root_data: &RootData, env: &Env) {
        if let LifeCycle::WidgetAdded = event {
            let widgets_were_added_or_removed = self.create_and_remove_widget_children(root_data, env);
            if widgets_were_added_or_removed {
                ctx.children_changed();
            }
        }

        root_data.for_each(|root_and_tab_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&root_and_tab_data.tab_data().id) {
                widget_child.lifecycle(ctx, event, root_and_tab_data, env);
            }
        });
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &RootData, root_data: &RootData, env: &Env) {
        // we send update to children first, before adding or removing children;
        // this way we avoid sending update to newly added children, at the cost
        // of potentially updating children that are going to be removed.
        root_data.for_each(|root_and_tab_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&root_and_tab_data.tab_data().id) {
                widget_child.update(ctx, root_and_tab_data, env);
            }
        });

        let widgets_were_added_or_removed = self.create_and_remove_widget_children(root_data, env);
        if widgets_were_added_or_removed {
            ctx.children_changed();
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, root_data: &RootData, env: &Env) -> Size {
        let number_of_tabs = root_data.data_len();

        let tab_width = if number_of_tabs == 0 {
            0.0
        } else if TAB_NORMAL_WIDTH * (number_of_tabs as f64) > bc.max().width { // If too many tabs, squash them
            bc.max().width / (number_of_tabs as f64)
        } else { // Else, the normal width
            TAB_NORMAL_WIDTH
        };
        let tab_height = value::TAB_HEIGHT.min(bc.max().height);

        let mut max_height_seen = bc.min().height;
        root_data.for_each(|root_and_tab_data, i| {
            let widget_child = match self.widget_children.get_mut(&root_and_tab_data.tab_data().id) {
                Some(widget_child) => widget_child,
                None => {
                    return;
                },
            };

            let child_bc = BoxConstraints::new(
                Size::new(tab_width, tab_height),
                Size::new(tab_width, tab_height),
            );

            let widget_child_size = widget_child.layout(ctx, &child_bc, root_and_tab_data, env);
            // Tabs should be rendered right-to-left
            let origin = Point::new(((number_of_tabs - 1 - i) as f64) * tab_width, 0.0);
            widget_child.set_origin(ctx, origin);
            max_height_seen = max_height_seen.max(widget_child_size.height);
        });

        let my_size = Size::new((root_data.data_len() as f64) * tab_width, max_height_seen);
        my_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, root_data: &RootData, env: &Env) {
        root_data.for_each(|root_and_tab_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&root_and_tab_data.tab_data().id) {
                widget_child.paint(ctx, root_and_tab_data, env);
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
        assert!(tab_list.widget_children.is_empty());
    }

    #[test]
    fn tab_list_create_and_remove_widget_children() {
        // Set up RootData with 3 tabs.
        let mut mock_root_data = crate::model::root_data::tests::mock();
        let mock_tab_data_1 = crate::model::tab_data::tests::mock();
        let mock_tab_data_2 = crate::model::tab_data::tests::mock();
        let mock_tab_data_3 = crate::model::tab_data::tests::mock();
        mock_root_data.tabs.push_back(mock_tab_data_1);
        mock_root_data.tabs.push_back(mock_tab_data_2);
        mock_root_data.tabs.push_back(mock_tab_data_3);
        let env = Env::empty();

        // Call create_and_remove_widget_children.
        let mut tab_list = TabList::new();
        let mut is_changed = tab_list.create_and_remove_widget_children(&mock_root_data, &env);

        // It should add three widgets, and report it was changed.
        assert!(is_changed);
        assert_eq!(3, tab_list.widget_children.len());

        // Call it again. It should report NOT changed.
        is_changed = tab_list.create_and_remove_widget_children(&mock_root_data, &env);
        assert!(!is_changed);
        assert_eq!(3, tab_list.widget_children.len());

        // Remove one data element, the corresponding widget should be removed.
        let removed_tab = mock_root_data.tabs.remove(1);
        is_changed = tab_list.create_and_remove_widget_children(&mock_root_data, &env);
        assert!(is_changed);
        assert_eq!(2, tab_list.widget_children.len());
        assert!(!tab_list.widget_children.contains_key(&removed_tab.id));

        // Remove and add a different data element, the length should be the same, BUT it should report it has changed.
        mock_root_data.tabs.remove(1);
        mock_root_data.tabs.push_back(crate::model::tab_data::tests::mock());
        is_changed = tab_list.create_and_remove_widget_children(&mock_root_data, &env);
        assert!(is_changed);
        assert_eq!(2, tab_list.widget_children.len());
    }
}
