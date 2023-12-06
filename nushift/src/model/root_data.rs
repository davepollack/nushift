// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::sync::{Mutex, Arc};

use druid::{Data, Env, LocalizedString};
use druid::im::{self, Vector};
use nushift_core::{Hypervisor, GfxOutput};
use reusable_id_pool::{ArcId, ReusableIdPool};

use super::scale_and_size::ScaleAndSize;
use super::tab_data::TabData;

#[derive(Clone, Data)]
pub struct RootData {
    pub tabs: im::HashMap<ArcId, TabData>,
    #[data(eq)]
    pub tabs_order: Vector<ArcId>,
    #[data(eq)]
    pub currently_selected_tab_id: Option<ArcId>,
    /// Currently, it should only be possible to submit one of these at a time, i.e. this should always have a length of 1
    #[data(eq)]
    pub close_tab_requests: Vector<ArcId>,
    pub scale_and_size: Option<ScaleAndSize>,

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

trait HypervisorDependency {
    fn add_new_tab(&mut self, hypervisor: &mut Hypervisor, initial_gfx_output: GfxOutput) -> ArcId;
    fn close_tab(&mut self, hypervisor: &mut Hypervisor, tab_id: &ArcId);
}

struct RealImpl;
struct MockImpl {
    pool: ReusableIdPool,
}

impl HypervisorDependency for RealImpl {
    fn add_new_tab(&mut self, hypervisor: &mut Hypervisor, initial_gfx_output: GfxOutput) -> ArcId {
        hypervisor.add_new_tab(initial_gfx_output)
    }

    fn close_tab(&mut self, hypervisor: &mut Hypervisor, tab_id: &ArcId) {
        hypervisor.close_tab(tab_id)
    }
}

impl HypervisorDependency for MockImpl {
    fn add_new_tab(&mut self, _hypervisor: &mut Hypervisor, _initial_gfx_output: GfxOutput) -> ArcId {
        self.pool.allocate()
    }

    fn close_tab(&mut self, _hypervisor: &mut Hypervisor, _tab_id: &ArcId) {
        // Intentionally empty
    }
}

impl RootData {
    /// Before calling add_new_tab, self.scale_and_size MUST be initialised or
    /// this will panic. It must also be initialised before calling any future
    /// method that restores tabs, for example, or a future version that by
    /// default starts with one tab open.
    pub fn add_new_tab(&mut self, env: &Env) -> ArcId {
        self.add_new_tab_impl(&mut RealImpl, env)
    }

    fn add_new_tab_impl<H: HypervisorDependency>(&mut self, hy_dep: &mut H, env: &Env) -> ArcId {
        let mut hypervisor = self.hypervisor.lock().unwrap();
        let mut title = LocalizedString::new("nushift-new-tab");
        title.resolve(self, env);
        let tab_id = hy_dep.add_new_tab(&mut hypervisor, self.scale_and_size.as_ref().expect("scale_and_size should be present at this point").gfx_output(0));

        self.currently_selected_tab_id = Some(ArcId::clone(&tab_id));

        self.tabs.insert(ArcId::clone(&tab_id), TabData {
            id: ArcId::clone(&tab_id),
            title: title.localized_str(),
            client_framebuffer: None,
        });
        self.tabs_order.push_back(ArcId::clone(&tab_id));

        ArcId::clone(&tab_id)
    }

    pub fn get_tab_by_id(&self, tab_id: &ArcId) -> Option<&TabData> {
        self.tabs.get(tab_id)
    }

    pub fn get_tab_by_index(&self, index: usize) -> Option<&TabData> {
        self.tabs_order.get(index).and_then(|tab_id| self.get_tab_by_id(tab_id))
    }

    pub fn get_tab_by_index_mut(&mut self, index: usize) -> Option<&mut TabData> {
        self.tabs_order.get(index).and_then(|tab_id| self.tabs.get_mut(tab_id))
    }

    pub fn select_tab(&mut self, tab_id: &ArcId) {
        self.currently_selected_tab_id = Some(ArcId::clone(tab_id));
    }

    pub fn close_selected_tab(&mut self) {
        match self.currently_selected_tab_id.as_ref().map(ArcId::clone) {
            Some(ref tab_id) => self.close_tab_impl(&mut RealImpl, tab_id),
            None => {},
        }
    }

    pub fn request_close_tab_from_tab_iter(&mut self, tab_id: &ArcId) {
        self.close_tab_requests.push_back(ArcId::clone(tab_id));
    }

    pub fn process_tab_iter_close_requests(&mut self) {
        for tab_id in self.close_tab_requests.split_off(0) {
            self.close_tab_impl(&mut RealImpl, &tab_id);
        }
    }

    fn close_tab_impl<H: HypervisorDependency>(&mut self, hy_dep: &mut H, tab_id: &ArcId) {
        let mut hypervisor = self.hypervisor.lock().unwrap();

        let mut id_to_remove = None;
        let mut index_to_remove = None;
        match self.tabs_order.iter().enumerate().find(|(_index, id)| *id == tab_id) {
            Some((index, id)) => {
                id_to_remove = Some(ArcId::clone(id));
                index_to_remove = Some(index);
            }
            None => {},
        }
        if let (Some(id), Some(index)) = (&id_to_remove, index_to_remove) {
            self.tabs.remove(id);
            self.tabs_order.remove(index);
        }

        // Update currently selected ID
        match (&id_to_remove, &self.currently_selected_tab_id, index_to_remove) {
            // First tab was closed, is currently selected, and there are no tabs left
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(0)) if id_to_remove == currently_selected_tab_id && self.tabs.is_empty() => {
                self.currently_selected_tab_id = None;
            },
            // First tab was closed, is currently selected, and there are still some tabs left
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(0)) if id_to_remove == currently_selected_tab_id => {
                let first_tab_id = ArcId::clone(&self.get_tab_by_index(0).expect("Must exist since there are still some tabs left").id);
                self.currently_selected_tab_id = Some(first_tab_id);
            },
            // Other tab was closed, is currently selected
            (Some(id_to_remove), Some(currently_selected_tab_id), Some(index)) if id_to_remove == currently_selected_tab_id => {
                let previous_tab_id = ArcId::clone(&self.get_tab_by_index(index - 1).expect("Must exist since index == 0 case was handled").id);
                self.currently_selected_tab_id = Some(previous_tab_id);
            },
            // Closed tab is not the currently selected one, or nothing was closed
            _ => {},
        }

        if let Some(ref id) = id_to_remove {
            hy_dep.close_tab(&mut hypervisor, id);
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use druid::im::{vector, hashmap};
    use reusable_id_pool::ReusableIdPool;

    pub fn mock() -> RootData {
        let hypervisor = Arc::new(Mutex::new(Hypervisor::new(|_| Ok(()))));
        RootData {
            tabs: hashmap!{},
            tabs_order: vector![],
            currently_selected_tab_id: None,
            close_tab_requests: vector![],
            scale_and_size: Some(ScaleAndSize { window_scale: vector![1.25, 1.25], client_area_size_dp: vector![1536.0, 864.0] }),
            hypervisor,
        }
    }

    fn mock_impl() -> MockImpl {
        MockImpl { pool: ReusableIdPool::new() }
    }

    #[test]
    fn add_new_tab_adds_new_and_sets_currently_selected() {
        let mut root_data = mock();
        let mut mock_impl = mock_impl();

        let newly_added_tab_id = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());

        assert_eq!(1, root_data.tabs.len());
        assert!(root_data.get_tab_by_index(0).expect("Should exist").id == newly_added_tab_id);
        assert!(root_data.currently_selected_tab_id.is_some());
        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &newly_added_tab_id);
    }

    #[test]
    fn select_tab_selects_tab() {
        let mut root_data = mock();
        let mut mock_impl = mock_impl();

        let tab1 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());
        let tab2 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab2);

        root_data.select_tab(&tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab1);
    }

    #[test]
    fn close_tab_should_remove_from_tabs_vector() {
        let mut root_data = mock();
        let mut mock_impl = mock_impl();

        let tab1 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());

        root_data.close_tab_impl(&mut mock_impl, &tab1);

        assert!(root_data.tabs.is_empty());
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_none_if_no_tabs_left() {
        let mut root_data = mock();
        let mut mock_impl = mock_impl();

        let tab1 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());

        root_data.close_tab_impl(&mut mock_impl, &tab1);

        assert!(root_data.currently_selected_tab_id.is_none());
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_next_tab_if_first_tab_was_closed() {
        let mut root_data = mock();
        let mut mock_impl = mock_impl();

        let tab1 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());
        let tab2 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());
        root_data.select_tab(&tab1);

        root_data.close_tab_impl(&mut mock_impl, &tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab2);
    }

    #[test]
    fn close_tab_should_set_currently_selected_to_previous_tab_if_tab_other_than_first_was_closed() {
        let mut root_data = mock();
        let mut mock_impl = mock_impl();

        let tab1 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());
        let tab2 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());

        root_data.close_tab_impl(&mut mock_impl, &tab2);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab1);
    }

    #[test]
    fn close_tab_should_not_set_currently_selected_if_not_currently_selected_tab_was_closed() {
        let mut root_data = mock();
        let mut mock_impl = mock_impl();

        let tab1 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());
        let tab2 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());

        root_data.close_tab_impl(&mut mock_impl, &tab1);

        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab2);
    }

    #[test]
    fn close_tab_should_do_nothing_if_other_id_is_passed_in() {
        let mut root_data = mock();
        let mut mock_impl = mock_impl();

        let _tab1 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());
        let tab2 = root_data.add_new_tab_impl(&mut mock_impl, &Env::empty());
        let reusable_id_pool = ReusableIdPool::new();
        let other_id = reusable_id_pool.allocate();

        root_data.close_tab_impl(&mut mock_impl, &other_id);

        assert_eq!(2, root_data.tabs.len());
        assert!(root_data.currently_selected_tab_id.is_some());
        assert!(root_data.currently_selected_tab_id.as_ref().unwrap() == &tab2);
    }
}
