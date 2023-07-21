use std::{sync::{Mutex, Arc}, fs};
use reusable_id_pool::{ReusableIdPool, ArcId};
use super::tab::Tab;

pub struct Hypervisor {
    tabs: Vec<Tab>,
    tabs_reusable_id_pool: Arc<Mutex<ReusableIdPool>>
}

impl Hypervisor {
    /// Create a hypervisor.
    pub fn new() -> Self {
        Hypervisor {
            tabs: vec![],
            tabs_reusable_id_pool: ReusableIdPool::new(),
        }
    }

    /// Add a new tab, with the given title.
    ///
    /// Internally, this generates an ID for the new tab, based on an ID pool
    /// owned by the `Hypervisor`.
    ///
    /// The newly-created ID is returned.
    pub fn add_new_tab<S: Into<String>>(&mut self, title: S) -> ArcId {
        let new_tab_id = ReusableIdPool::allocate(&self.tabs_reusable_id_pool);
        let mut new_tab = Tab::new(new_tab_id, title);

        let binary_blob_result = fs::read("../examples/hello-world/zig-out/bin/hello-world");
        match binary_blob_result {
            Ok(binary_blob) => {
                new_tab.load(binary_blob);
                new_tab.run();
            },
            Err(err) => log::error!("Hardcoded binary blob path error: {err:?}"),
        }

        self.tabs.push(new_tab);

        ArcId::clone(&self.tabs.last().unwrap().id())
    }

    /// Close a tab.
    ///
    /// If the passed-in `tab_id` does not exist, this method does nothing.
    pub fn close_tab(&mut self, tab_id: &ArcId) {
        match self.tabs.iter().enumerate().find(|(_index, tab)| tab.id() == tab_id) {
            Some((index, _tab)) => {
                self.tabs.remove(index);
            },
            None => {},
        }
    }
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

        assert_eq!("Tab title 1", hypervisor.tabs[0].title());
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
        let tab_id = ReusableIdPool::allocate(&ReusableIdPool::new());

        hypervisor.close_tab(&tab_id);

        assert_eq!(0, hypervisor.tabs.len());
    }
}
