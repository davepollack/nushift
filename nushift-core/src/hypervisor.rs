use std::sync::{Mutex, Arc};
use crate::reusable_id_pool::{ReusableIdPool, Id};

pub struct Hypervisor {
    tabs: Vec<Tab>,
    tabs_reusable_id_pool: Arc<Mutex<ReusableIdPool>>
}

impl Hypervisor {
    /// Create a hypervisor.
    pub fn new() -> Self {
        Hypervisor {
            tabs: vec![],
            tabs_reusable_id_pool: Arc::new(Mutex::new(ReusableIdPool::new())),
        }
    }

    /// Add a new tab, with the given title.
    ///
    /// Internally, this generates an ID for the new tab, based on an ID pool
    /// owned by the `Hypervisor`.
    ///
    /// The newly-created ID is returned.
    pub fn add_new_tab<S>(&mut self, title: S) -> Arc<Id>
        where S: Into<String>
    {
        let new_tab_id = ReusableIdPool::allocate(&mut self.tabs_reusable_id_pool);

        let new_tab = Tab {
            id: new_tab_id,
            title: title.into(),
        };

        self.tabs.push(new_tab);

        Arc::clone(&self.tabs.last().unwrap().id)
    }

    /// Close a tab.
    ///
    /// If the passed-in `tab_id` does not exist, this method does nothing.
    pub fn close_tab(&mut self, tab_id: &Arc<Id>) {
        match self.tabs.iter().enumerate().find(|(_index, tab)| Arc::ptr_eq(&tab.id, tab_id)) {
            Some((index, _tab)) => {
                self.tabs.remove(index);
            }
            None => {},
        }
    }
}

struct Tab {
    id: Arc<Id>,
    title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypervisor_new_creates_new() {
        let hypervisor = Hypervisor::new();

        assert_eq!(0, hypervisor.tabs.len());
    }

    #[test]
    fn hypervisor_add_new_tab_adds_new_tab() {
        let mut hypervisor = Hypervisor::new();

        hypervisor.add_new_tab("Tab title 1");

        assert_eq!("Tab title 1", hypervisor.tabs[0].title);
    }

    #[test]
    fn hypervisor_close_tab_closes_existing_tab() {
        let mut hypervisor = Hypervisor::new();
        let tab_id = hypervisor.add_new_tab("Tab title 1");

        hypervisor.close_tab(&tab_id);

        assert_eq!(0, hypervisor.tabs.len());
    }

    #[test]
    fn hypervisor_close_tab_does_nothing_if_tab_does_not_exist() {
        let mut hypervisor = Hypervisor::new();
        let tab_id = ReusableIdPool::allocate(&Arc::new(Mutex::new(ReusableIdPool::new())));

        hypervisor.close_tab(&tab_id);

        assert_eq!(0, hypervisor.tabs.len());
    }
}
