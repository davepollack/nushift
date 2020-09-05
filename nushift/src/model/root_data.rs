use std::sync::{Mutex, Arc};
use druid::{Data, Lens};
use druid::im::Vector;
use nushift_core::{Hypervisor, Id, IdEq};

use super::tab_data::TabData;

const NEW_TAB_TITLE: &str = "New tab";

#[derive(Clone, Data, Lens)]
pub struct RootData {
    pub tabs: Vector<TabData>,
    pub currently_selected_tab_id: Option<Arc<Id>>,

    #[data(ignore)]
    pub hypervisor: Arc<Mutex<Hypervisor>>
}

impl RootData {
    pub fn add_new_tab(&mut self) -> Arc<Id> {
        let mut hypervisor = self.hypervisor.lock().unwrap();
        let tab_id = hypervisor.add_new_tab(NEW_TAB_TITLE);

        self.currently_selected_tab_id = Some(Arc::clone(&tab_id));

        self.tabs.push_back(TabData {
            id: Arc::clone(&tab_id),
            title: NEW_TAB_TITLE.into()
        });

        Arc::clone(&tab_id)
    }

    pub fn select_tab(&mut self, tab_id: &Arc<Id>) {
        self.currently_selected_tab_id = Some(Arc::clone(&tab_id));
    }

    pub fn close_tab(&mut self, tab_id: &Arc<Id>) {
        let mut hypervisor = self.hypervisor.lock().unwrap();

        let mut id_to_remove = None;
        let mut index_to_remove = None;
        match self.tabs.iter().enumerate().find(|(_index, tab)| tab.id.id_eq(tab_id)) {
            Some((index, tab)) => {
                id_to_remove = Some(Arc::clone(&tab.id));
                index_to_remove = Some(index);
            }
            None => {},
        }
        if let Some(index) = index_to_remove {
            self.tabs.remove(index);
        }

        match (&id_to_remove, &self.currently_selected_tab_id, index_to_remove) {
            // First tab was closed, is currently selected, and there are no tabs left
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(0)) if id_to_remove.id_eq(currently_selected_tab_id) && self.tabs.is_empty() => {
                self.currently_selected_tab_id = None;
            }
            // First tab was closed, is currently selected, and there are still some tabs left
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(0)) if id_to_remove.id_eq(currently_selected_tab_id) => {
                let first_tab_id = Arc::clone(&self.tabs[0].id);
                self.currently_selected_tab_id = Some(first_tab_id);
            },
            // Other tab was closed, is currently selected
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(index)) if id_to_remove.id_eq(currently_selected_tab_id) => {
                let previous_tab_id = Arc::clone(&self.tabs[index - 1].id);
                self.currently_selected_tab_id = Some(previous_tab_id);
            },
            // Closed tab is not the currently selected one, or nothing was closed
            _ => {},
        }

        if let Some(id) = &id_to_remove {
            hypervisor.close_tab(id);
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use druid::im::vector;
    use nushift_core::ReusableIdPool;

    pub fn mock() -> RootData {
        let hypervisor = Arc::new(Mutex::new(Hypervisor::new()));
        RootData {
            tabs: vector![],
            currently_selected_tab_id: None,
            hypervisor,
        }
    }

    #[test]
    fn add_new_tab_adds_new_and_sets_currently_selected() {
        let mut root_data = mock();

        let newly_added_tab_id = root_data.add_new_tab();

        assert_eq!(1, root_data.tabs.len());
        assert!(root_data.tabs[0].id.id_eq(&newly_added_tab_id));
        assert!(root_data.currently_selected_tab_id.is_some());
        assert!(root_data.currently_selected_tab_id.as_ref().unwrap().id_eq(&newly_added_tab_id));
    }

    #[test]
    fn select_tab_selects_tab() {
        let mut root_data = mock();

        let tab1 = root_data.add_new_tab();
        let tab2 = root_data.add_new_tab();

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap().id_eq(&tab2));

        root_data.select_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap().id_eq(&tab1));
    }

    #[test]
    fn close_tab_should_remove_from_tabs_vector() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab();

        root_data.close_tab(&tab1);

        assert!(root_data.tabs.is_empty());
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_none_if_no_tabs_left() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab();

        root_data.close_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.is_none());
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_next_tab_if_first_tab_was_closed() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab();
        let tab2 = root_data.add_new_tab();
        root_data.select_tab(&tab1);

        root_data.close_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap().id_eq(&tab2));
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_previous_tab_if_tab_other_than_first_was_closed() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab();
        let tab2 = root_data.add_new_tab();

        root_data.close_tab(&tab2);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap().id_eq(&tab1));
    }

    #[test]
    fn close_tab_should_not_set_currently_selected_if_not_currently_selected_tab_was_closed() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab();
        let tab2 = root_data.add_new_tab();

        root_data.close_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap().id_eq(&tab2));
    }

    #[test]
    fn close_tab_should_do_nothing_if_other_id_is_passed_in() {
        let mut root_data = mock();
        let _tab1 = root_data.add_new_tab();
        let tab2 = root_data.add_new_tab();
        let reusable_id_pool = Arc::new(Mutex::new(ReusableIdPool::new()));
        let other_id = ReusableIdPool::allocate(&reusable_id_pool);

        root_data.close_tab(&other_id);

        assert_eq!(2, root_data.tabs.len());
        assert!(root_data.currently_selected_tab_id.is_some());
        assert!(root_data.currently_selected_tab_id.as_ref().unwrap().id_eq(&tab2));
    }
}
