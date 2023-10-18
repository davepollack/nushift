use std::fmt::Debug;
use std::sync::{Mutex, Arc};

use druid::{Data, Env, LocalizedString, Lens};
use druid::im::Vector;
use nushift_core::Hypervisor;
use reusable_id_pool::ArcId;

use super::client_framebuffer::ClientFramebuffer;
use super::scale_and_size::ScaleAndSize;
use super::tab_data::TabData;

#[derive(Clone, Data, Lens)]
pub struct RootData {
    pub tabs: Vector<TabData>,
    #[data(eq)]
    pub currently_selected_tab_id: Option<ArcId>,
    /// Currently, it should only be possible to submit one of these at a time, i.e. this should always have a length of 1
    #[data(eq)]
    pub close_tab_requests: Vector<ArcId>,
    pub scale_and_size: Option<ScaleAndSize>,
    pub client_framebuffer: Option<ClientFramebuffer>, // TODO: This must be per-tab

    #[data(ignore)]
    pub hypervisor: Arc<Mutex<Hypervisor>>,
}

impl Debug for RootData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootData")
            .field("tabs", &self.tabs)
            .field("currently_selected_tab_id", &self.currently_selected_tab_id)
            .field("close_tab_requests", &self.close_tab_requests)
            .field("scale_and_size", &self.scale_and_size)
            .finish_non_exhaustive()
    }
}

impl RootData {
    /// Before calling add_new_tab, self.scale_and_size MUST be initialised or
    /// this will panic. It must also be initialised before calling any future
    /// method that restores tabs, for example, or a future version that by
    /// default starts with one tab open.
    pub fn add_new_tab(&mut self, env: &Env) -> ArcId {
        let mut hypervisor = self.hypervisor.lock().unwrap();
        let mut title = LocalizedString::new("nushift-new-tab");
        title.resolve(self, env);
        let tab_id = hypervisor.add_new_tab(self.scale_and_size.as_ref().expect("scale_and_size should be present at this point").output());

        self.currently_selected_tab_id = Some(ArcId::clone(&tab_id));

        self.tabs.push_back(TabData {
            id: ArcId::clone(&tab_id),
            title: title.localized_str(),
        });

        ArcId::clone(&tab_id)
    }

    pub fn select_tab(&mut self, tab_id: &ArcId) {
        self.currently_selected_tab_id = Some(ArcId::clone(tab_id));
    }

    pub fn close_selected_tab(&mut self) {
        match self.currently_selected_tab_id.as_ref().map(ArcId::clone) {
            Some(ref tab_id) => self.close_tab(tab_id),
            None => {},
        }
    }

    pub fn request_close_tab_from_tab_iter(&mut self, tab_id: &ArcId) {
        self.close_tab_requests.push_back(ArcId::clone(tab_id));
    }

    pub fn process_tab_iter_close_requests(&mut self) {
        for tab_id in self.close_tab_requests.split_off(0) {
            self.close_tab(&tab_id);
        }
    }

    fn close_tab(&mut self, tab_id: &ArcId) {
        let mut hypervisor = self.hypervisor.lock().unwrap();

        let mut id_to_remove = None;
        let mut index_to_remove = None;
        match self.tabs.iter().enumerate().find(|(_index, tab)| &tab.id == tab_id) {
            Some((index, tab)) => {
                id_to_remove = Some(ArcId::clone(&tab.id));
                index_to_remove = Some(index);
            }
            None => {},
        }
        if let Some(index) = index_to_remove {
            self.tabs.remove(index);
        }

        match (&id_to_remove, &self.currently_selected_tab_id, index_to_remove) {
            // First tab was closed, is currently selected, and there are no tabs left
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(0)) if id_to_remove == currently_selected_tab_id && self.tabs.is_empty() => {
                self.currently_selected_tab_id = None;
            },
            // First tab was closed, is currently selected, and there are still some tabs left
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(0)) if id_to_remove == currently_selected_tab_id => {
                let first_tab_id = ArcId::clone(&self.tabs[0].id);
                self.currently_selected_tab_id = Some(first_tab_id);
            },
            // Other tab was closed, is currently selected
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(index)) if id_to_remove == currently_selected_tab_id => {
                let previous_tab_id = ArcId::clone(&self.tabs[index - 1].id);
                self.currently_selected_tab_id = Some(previous_tab_id);
            },
            // Closed tab is not the currently selected one, or nothing was closed
            _ => {},
        }

        if let Some(ref id) = id_to_remove {
            hypervisor.close_tab(id);
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use druid::im::vector;
    use reusable_id_pool::ReusableIdPool;

    pub fn mock() -> RootData {
        let hypervisor = Arc::new(Mutex::new(Hypervisor::new(|_| Ok(()))));
        RootData {
            tabs: vector![],
            currently_selected_tab_id: None,
            close_tab_requests: vector![],
            scale_and_size: Some(ScaleAndSize { window_scale: vector![1.25, 1.25], client_area_size_dp: vector![1536.0, 864.0] }),
            client_framebuffer: None,
            hypervisor,
        }
    }

    #[test]
    fn add_new_tab_adds_new_and_sets_currently_selected() {
        let mut root_data = mock();

        let newly_added_tab_id = root_data.add_new_tab(&Env::empty());

        assert_eq!(1, root_data.tabs.len());
        assert!(root_data.tabs[0].id == newly_added_tab_id);
        assert!(root_data.currently_selected_tab_id.is_some());
        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &newly_added_tab_id);
    }

    #[test]
    fn select_tab_selects_tab() {
        let mut root_data = mock();

        let tab1 = root_data.add_new_tab(&Env::empty());
        let tab2 = root_data.add_new_tab(&Env::empty());

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab2);

        root_data.select_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab1);
    }

    #[test]
    fn close_tab_should_remove_from_tabs_vector() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab(&Env::empty());

        root_data.close_tab(&tab1);

        assert!(root_data.tabs.is_empty());
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_none_if_no_tabs_left() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab(&Env::empty());

        root_data.close_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.is_none());
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_next_tab_if_first_tab_was_closed() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab(&Env::empty());
        let tab2 = root_data.add_new_tab(&Env::empty());
        root_data.select_tab(&tab1);

        root_data.close_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab2);
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_previous_tab_if_tab_other_than_first_was_closed() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab(&Env::empty());
        let tab2 = root_data.add_new_tab(&Env::empty());

        root_data.close_tab(&tab2);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab1);
    }

    #[test]
    fn close_tab_should_not_set_currently_selected_if_not_currently_selected_tab_was_closed() {
        let mut root_data = mock();
        let tab1 = root_data.add_new_tab(&Env::empty());
        let tab2 = root_data.add_new_tab(&Env::empty());

        root_data.close_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab2);
    }

    #[test]
    fn close_tab_should_do_nothing_if_other_id_is_passed_in() {
        let mut root_data = mock();
        let _tab1 = root_data.add_new_tab(&Env::empty());
        let tab2 = root_data.add_new_tab(&Env::empty());
        let reusable_id_pool = ReusableIdPool::new();
        let other_id = reusable_id_pool.allocate();

        root_data.close_tab(&other_id);

        assert_eq!(2, root_data.tabs.len());
        assert!(root_data.currently_selected_tab_id.is_some());
        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab2);
    }
}
