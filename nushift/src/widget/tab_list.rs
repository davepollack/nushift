use druid::widget::prelude::*;
use druid::{widget::ListIter, WidgetPod, Widget, Point, Rect};
use std::{sync::Arc, hash::Hash, collections::{HashSet, HashMap}};
use nushift_core::{Id, IdEq};

use crate::model::{RootAndVectorTabData, RootAndTabData};
use super::{tab, value};

const TAB_NORMAL_WIDTH: f64 = 200.0;

#[derive(Debug, Clone)]
struct TabKey(Arc<Id>);

impl TabKey {
    fn new(id: &Arc<Id>) -> Self {
        TabKey(Arc::clone(id))
    }
}
impl PartialEq for TabKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.id_eq(&other.0)
    }
}
impl Eq for TabKey {}
impl Hash for TabKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

pub fn tab_list() -> TabList {
    TabList::new()
}

/// Based on druid::widget::List, but has more child tracking, and custom
/// `layout()` method
pub struct TabList {
    widget_children: HashMap<TabKey, WidgetPod<RootAndTabData, tab::Tab>>,
}

impl TabList {
    fn new() -> Self {
        TabList { widget_children: HashMap::new() }
    }

    /// When the widget is created or the data changes, create or remove children
    /// as needed.
    ///
    /// Returns `true` if children were added or removed.
    fn create_and_remove_widget_children(&mut self, root_and_vector_tab_data: &RootAndVectorTabData, _env: &Env) -> bool {
        let mut is_changed = false;
        let original_widget_children_len = self.widget_children.len();

        let mut data_ids_set = HashSet::with_capacity(root_and_vector_tab_data.data_len());
        root_and_vector_tab_data.for_each(|root_and_tab_data, _| {
            data_ids_set.insert(TabKey::new(&root_and_tab_data.1.id));
        });

        // Wipe all widget children that are no longer in the data
        self.widget_children.retain(|tab_key: &TabKey, _| data_ids_set.contains(tab_key));

        if self.widget_children.len() != original_widget_children_len {
            is_changed = true;
        }

        // Add new widget children corresponding to new IDs
        for tab_key in data_ids_set {
            if !self.widget_children.contains_key(&tab_key) {
                self.widget_children.insert(tab_key, WidgetPod::new(tab::tab()));
                is_changed = true;
            }
        }

        is_changed
    }
}

impl Widget<RootAndVectorTabData> for TabList {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, root_and_vector_tab_data: &mut RootAndVectorTabData, env: &Env) {
        root_and_vector_tab_data.for_each_mut(|root_and_tab_data: &mut RootAndTabData, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&TabKey::new(&root_and_tab_data.1.id)) {
                widget_child.event(ctx, event, root_and_tab_data, env);
            }
        });
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, root_and_vector_tab_data: &RootAndVectorTabData, env: &Env) {
        if let LifeCycle::WidgetAdded = event {
            let widgets_were_added_or_removed = self.create_and_remove_widget_children(root_and_vector_tab_data, env);
            if widgets_were_added_or_removed {
                ctx.children_changed();
            }
        }

        root_and_vector_tab_data.for_each(|root_and_tab_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&TabKey::new(&root_and_tab_data.1.id)) {
                widget_child.lifecycle(ctx, event, root_and_tab_data, env);
            }
        });
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &RootAndVectorTabData, root_and_vector_tab_data: &RootAndVectorTabData, env: &Env) {
        // we send update to children first, before adding or removing children;
        // this way we avoid sending update to newly added children, at the cost
        // of potentially updating children that are going to be removed.
        root_and_vector_tab_data.for_each(|root_and_tab_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&TabKey::new(&root_and_tab_data.1.id)) {
                widget_child.update(ctx, root_and_tab_data, env);
            }
        });

        let widgets_were_added_or_removed = self.create_and_remove_widget_children(root_and_vector_tab_data, env);
        if widgets_were_added_or_removed {
            ctx.children_changed();
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, root_and_vector_tab_data: &RootAndVectorTabData, env: &Env) -> Size {
        let number_of_tabs = root_and_vector_tab_data.data_len();

        let tab_width = if number_of_tabs == 0 {
            0.0
        } else if TAB_NORMAL_WIDTH * (number_of_tabs as f64) > bc.max().width { // If too many tabs, squash them
            bc.max().width / (number_of_tabs as f64)
        } else { // Else, the normal width
            TAB_NORMAL_WIDTH
        };
        let tab_height = value::TAB_HEIGHT.min(bc.max().height);

        let mut max_height_seen = bc.min().height;
        root_and_vector_tab_data.for_each(|root_and_tab_data, i| {
            let widget_child = match self.widget_children.get_mut(&TabKey::new(&root_and_tab_data.1.id)) {
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
            let rect = Rect::from_origin_size(origin, widget_child_size);
            widget_child.set_layout_rect(ctx, root_and_tab_data, env, rect);
            max_height_seen = max_height_seen.max(widget_child_size.height);
        });

        let my_size = Size::new((root_and_vector_tab_data.data_len() as f64) * tab_width, max_height_seen);
        my_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, root_and_vector_tab_data: &RootAndVectorTabData, env: &Env) {
        root_and_vector_tab_data.for_each(|root_and_tab_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&TabKey::new(&root_and_tab_data.1.id)) {
                widget_child.paint(ctx, root_and_tab_data, env);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::hash_map::DefaultHasher, sync::Mutex, hash::Hasher};
    use nushift_core::ReusableIdPool;

    fn tab_keys_setup() -> (TabKey, TabKey) {
        let pool = Arc::new(Mutex::new(ReusableIdPool::new()));
        let id = ReusableIdPool::allocate(&pool);
        let cloned_arc_id = Arc::clone(&id);

        (TabKey(id), TabKey(cloned_arc_id))
    }

    #[test]
    fn tab_key_eq_is_true_for_cloned_arc_id() {
        let (tab_key_1, tab_key_2) = tab_keys_setup();

        assert!(tab_key_1.eq(&tab_key_2));
    }

    #[test]
    fn tab_key_hash_is_equal_for_cloned_arc_id() {
        let (tab_key_1, tab_key_2) = tab_keys_setup();

        let mut hasher = DefaultHasher::new();
        tab_key_1.hash(&mut hasher);
        let hash_1 = hasher.finish();

        let mut hasher = DefaultHasher::new();
        tab_key_2.hash(&mut hasher);
        let hash_2 = hasher.finish();

        assert_eq!(hash_1, hash_2);
    }

    #[test]
    fn tab_list_new_creates_widget_with_empty_vec() {
        let tab_list = TabList::new();
        assert!(tab_list.widget_children.is_empty());
    }

    #[test]
    fn tab_list_create_and_remove_widget_children() {
        // Set up RootAndVectorTabData with 3 tabs.
        let mut mock_root_and_vector_tab_data = crate::model::combined::tests::mock_root_and_vector_tab_data();
        let mock_tab_data_2 = crate::model::tab_data::tests::mock();
        let mock_tab_data_3 = crate::model::tab_data::tests::mock();
        mock_root_and_vector_tab_data.1.push_back(mock_tab_data_2);
        mock_root_and_vector_tab_data.1.push_back(mock_tab_data_3);
        let env = Env::default();

        // Call create_and_remove_widget_children.
        let mut tab_list = TabList::new();
        let mut is_changed = tab_list.create_and_remove_widget_children(&mock_root_and_vector_tab_data, &env);

        // It should add three widgets, and report it was changed.
        assert!(is_changed);
        assert_eq!(3, tab_list.widget_children.len());

        // Call it again. It should report NOT changed.
        is_changed = tab_list.create_and_remove_widget_children(&mock_root_and_vector_tab_data, &env);
        assert!(!is_changed);
        assert_eq!(3, tab_list.widget_children.len());

        // Remove one data element, the corresponding widget should be removed.
        let removed_tab = mock_root_and_vector_tab_data.1.remove(1);
        is_changed = tab_list.create_and_remove_widget_children(&mock_root_and_vector_tab_data, &env);
        assert!(is_changed);
        assert_eq!(2, tab_list.widget_children.len());
        assert!(!tab_list.widget_children.contains_key(&TabKey::new(&removed_tab.id)));

        // Remove and add a different data element, the length should be the same, BUT it should report it has changed.
        mock_root_and_vector_tab_data.1.remove(1);
        mock_root_and_vector_tab_data.1.push_back(crate::model::tab_data::tests::mock());
        is_changed = tab_list.create_and_remove_widget_children(&mock_root_and_vector_tab_data, &env);
        assert!(is_changed);
        assert_eq!(2, tab_list.widget_children.len());
    }
}
