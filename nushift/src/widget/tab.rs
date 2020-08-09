use druid::widget::prelude::*;
use druid::{
    widget::{ListIter, Flex, Label, MainAxisAlignment, Container, Painter, ControllerHost, Click},
    WidgetPod, Widget, WidgetExt, Point, Rect, Color,
};
use std::{sync::Arc, hash::Hash, collections::{HashSet, HashMap}};
use nushift_core::{Id, IdEq};

use crate::model::{TabListAndSharedRootData, TabAndSharedRootData};
use super::{value, button};

const TAB_BACKGROUND_COLOR: Color = Color::rgb8(0xa1, 0xf0, 0xf0);
const TAB_HOVER_BACKGROUND_COLOR: Color = Color::rgb8(0xbd, 0xf5, 0xf5);
const TAB_SELECTED_BACKGROUND_COLOR: Color = Color::rgb8(0xe9, 0xfc, 0xfc);
const TAB_NORMAL_WIDTH: f64 = 200.0;

type Tab = ControllerHost<Container<TabAndSharedRootData>, Click<TabAndSharedRootData>>;

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

fn tab() -> Tab {
    let selected_or_non_selected_background = Painter::new(|ctx, data: &TabAndSharedRootData, _| {
        let bounds = ctx.size().to_rect();
        match &data.0.currently_selected_tab_id {
            Some(id) if id.id_eq(&data.1.id) => {
                ctx.fill(bounds, &TAB_SELECTED_BACKGROUND_COLOR);
            },
            _ => {
                if ctx.is_hot() {
                    ctx.fill(bounds, &TAB_HOVER_BACKGROUND_COLOR);
                } else {
                    ctx.fill(bounds, &TAB_BACKGROUND_COLOR);
                }
            },
        }
    });

    let tab = Flex::row()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_child(Label::new(|(_root, tab_data): &TabAndSharedRootData, _env: &_| tab_data.title.to_owned()))
        .with_child(button::close_button())
        .padding((value::TAB_HORIZONTAL_PADDING, 0.0))
        .background(selected_or_non_selected_background)
        .on_click(|_, _, _| {
            // Attach `Click` widget to get "hot" tracking and other useful
            // mouse handling, but don't actually use it for the select handler,
            // we're going to do that ourselves.
        });

    tab
}

pub fn tab_list() -> TabList {
    TabList::new()
}

/// Based on druid::widget::List, but has more child tracking, and custom
/// `layout()` method
pub struct TabList {
    widget_children: HashMap<TabKey, WidgetPod<TabAndSharedRootData, Tab>>,
}

impl TabList {
    fn new() -> Self {
        TabList { widget_children: HashMap::new() }
    }

    /// When the widget is created or the data changes, create or remove children
    /// as needed.
    ///
    /// Returns `true` if children were added or removed.
    fn create_and_remove_widget_children(&mut self, data: &TabListAndSharedRootData, _env: &Env) -> bool {
        let mut is_changed = false;
        let original_widget_children_len = self.widget_children.len();
        let original_data_len = data.data_len();

        let mut data_ids_set = HashSet::with_capacity(original_data_len);
        data.for_each(|child_data, _| {
            data_ids_set.insert(TabKey::new(&child_data.1.id));
        });

        // Wipe all widget children that are no longer in the data
        self.widget_children.retain(|tab_key: &TabKey, _| data_ids_set.contains(tab_key));

        if self.widget_children.len() != original_widget_children_len {
            is_changed = true;
        }

        // Add new widget children corresponding to new IDs
        for tab_key in data_ids_set {
            if !self.widget_children.contains_key(&tab_key) {
                self.widget_children.insert(tab_key, WidgetPod::new(tab()));
                is_changed = true;
            }
        }

        is_changed
    }
}

impl Widget<TabListAndSharedRootData> for TabList {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut TabListAndSharedRootData, env: &Env) {
        data.for_each_mut(|child_data: &mut TabAndSharedRootData, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&TabKey::new(&child_data.1.id)) {
                widget_child.event(ctx, event, child_data, env);
            }
        });
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &TabListAndSharedRootData, env: &Env) {
        if let LifeCycle::WidgetAdded = event {
            let widgets_were_added_or_removed = self.create_and_remove_widget_children(data, env);
            if widgets_were_added_or_removed {
                ctx.children_changed();
            }
        }

        data.for_each(|child_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&TabKey::new(&child_data.1.id)) {
                widget_child.lifecycle(ctx, event, child_data, env);
            }
        });
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &TabListAndSharedRootData, data: &TabListAndSharedRootData, env: &Env) {
        // we send update to children first, before adding or removing children;
        // this way we avoid sending update to newly added children, at the cost
        // of potentially updating children that are going to be removed.
        data.for_each(|child_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&TabKey::new(&child_data.1.id)) {
                widget_child.update(ctx, child_data, env);
            }
        });

        let widgets_were_added_or_removed = self.create_and_remove_widget_children(data, env);
        if widgets_were_added_or_removed {
            ctx.children_changed();
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, data: &TabListAndSharedRootData, env: &Env) -> Size {
        let len = data.data_len();

        let tab_width = if len == 0 {
            0.0
        } else if TAB_NORMAL_WIDTH * (len as f64) > bc.max().width { // If too many tabs, squash them
            bc.max().width / (len as f64)
        } else { // Else, the normal width
            TAB_NORMAL_WIDTH
        };
        let tab_height = value::TAB_HEIGHT.min(bc.max().height);

        let mut max_height_seen = bc.min().height;
        data.for_each(|child_data, i| {
            let widget_child = match self.widget_children.get_mut(&TabKey::new(&child_data.1.id)) {
                Some(widget_child) => widget_child,
                None => {
                    return;
                },
            };

            let child_bc = BoxConstraints::new(
                Size::new(tab_width, tab_height),
                Size::new(tab_width, tab_height),
            );

            let widget_child_size = widget_child.layout(ctx, &child_bc, child_data, env);
            // Tabs should be rendered right-to-left
            let origin = Point::new(((len - 1 - i) as f64) * tab_width, 0.0);
            let rect = Rect::from_origin_size(origin, widget_child_size);
            widget_child.set_layout_rect(ctx, child_data, env, rect);
            max_height_seen = max_height_seen.max(widget_child_size.height);
        });

        let my_size = Size::new((data.data_len() as f64) * tab_width, max_height_seen);
        my_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &TabListAndSharedRootData, env: &Env) {
        data.for_each(|child_data, _| {
            if let Some(widget_child) = self.widget_children.get_mut(&TabKey::new(&child_data.1.id)) {
                widget_child.paint(ctx, child_data, env);
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
        // Set up TabListAndSharedRootData with 3 tabs.
        let mut mock_tab_list_and_shared_root_data = crate::model::tab_data::tests::mock_tab_list_and_shared_root_data();
        let mock_tab_data_2 = crate::model::tab_data::tests::mock();
        let mock_tab_data_3 = crate::model::tab_data::tests::mock();
        mock_tab_list_and_shared_root_data.1.push_back(mock_tab_data_2);
        mock_tab_list_and_shared_root_data.1.push_back(mock_tab_data_3);
        let env = Env::default();

        // Call create_and_remove_widget_children.
        let mut tab_list = TabList::new();
        let mut is_changed = tab_list.create_and_remove_widget_children(&mock_tab_list_and_shared_root_data, &env);

        // It should add three widgets, and report it was changed.
        assert!(is_changed);
        assert_eq!(3, tab_list.widget_children.len());

        // Call it again. It should report NOT changed.
        is_changed = tab_list.create_and_remove_widget_children(&mock_tab_list_and_shared_root_data, &env);
        assert!(!is_changed);
        assert_eq!(3, tab_list.widget_children.len());

        // Remove one data element, the corresponding widget should be removed.
        mock_tab_list_and_shared_root_data.1.remove(1);
        is_changed = tab_list.create_and_remove_widget_children(&mock_tab_list_and_shared_root_data, &env);
        assert!(is_changed);
        assert_eq!(2, tab_list.widget_children.len());
        let widget_children_keys: HashSet<TabKey> = tab_list.widget_children.keys().map(|key| key.to_owned()).collect();
        let data_keys: HashSet<TabKey> = mock_tab_list_and_shared_root_data.1.iter().map(|tab_data| TabKey::new(&tab_data.id)).collect();
        assert_eq!(widget_children_keys, data_keys);

        // Remove and add a different data element, the length should be the same, BUT it should report it has changed.
        mock_tab_list_and_shared_root_data.1.remove(1);
        mock_tab_list_and_shared_root_data.1.push_back(crate::model::tab_data::tests::mock());
        is_changed = tab_list.create_and_remove_widget_children(&mock_tab_list_and_shared_root_data, &env);
        assert!(is_changed);
        assert_eq!(2, tab_list.widget_children.len());
    }
}
