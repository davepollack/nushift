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
    pub fn add_new_tab(&mut self) {
        let mut hypervisor = self.hypervisor.lock().unwrap();
        let tab_id = hypervisor.add_new_tab(NEW_TAB_TITLE);

        self.currently_selected_tab_id = Some(Arc::clone(&tab_id));

        self.tabs.push_back(TabData {
            id: Arc::clone(&tab_id),
            title: NEW_TAB_TITLE.into()
        });
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

        // TODO if last tab, main title should change to "No tabs"

        match (&id_to_remove, &self.currently_selected_tab_id, index_to_remove) {
            // First tab was closed, is currently selected, and there are no tabs left
            (Some(id), Some(currently_selected_tab_id), Some(0)) if id.id_eq(currently_selected_tab_id) && self.tabs.is_empty() => {
                self.currently_selected_tab_id = None;
            }
            // First tab was closed, is currently selected, and there are still some tabs left
            (Some(id), Some(currently_selected_tab_id), Some(0)) if id.id_eq(currently_selected_tab_id) => {
                let first_tab_id = Arc::clone(&self.tabs[0].id);
                self.currently_selected_tab_id = Some(first_tab_id);
            },
            // Other tab was closed, is currently selected
            (Some(id), Some(currently_selected_tab_id), Some(index)) if id.id_eq(currently_selected_tab_id) => {
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
