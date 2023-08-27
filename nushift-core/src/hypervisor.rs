use std::{fs, collections::HashMap};

use reusable_id_pool::{ReusableIdPool, ArcId};

use super::tab::Tab;

pub struct Hypervisor {
    tabs: HashMap<ArcId, Tab>,
    tabs_reusable_id_pool: ReusableIdPool,
}

impl Hypervisor {
    /// Create a hypervisor.
    pub fn new() -> Self {
        Hypervisor {
            tabs: HashMap::new(),
            tabs_reusable_id_pool: ReusableIdPool::new(),
        }
    }

    /// Add a new tab.
    ///
    /// Internally, this generates an ID for the new tab, based on an ID pool
    /// owned by the `Hypervisor`.
    ///
    /// The newly-created ID is returned.
    pub fn add_new_tab(&mut self) -> ArcId {
        let new_tab_id = self.tabs_reusable_id_pool.allocate();

        let new_tab_id_cloned_for_tab = ArcId::clone(&new_tab_id);
        let new_tab_id_cloned_for_key = ArcId::clone(&new_tab_id);

        let mut new_tab = Tab::new(new_tab_id_cloned_for_tab);

        let binary_blob_result = fs::read("../examples/hello-world/zig-out/bin/hello-world");
        match binary_blob_result {
            Ok(binary_blob) => new_tab.load_and_run(binary_blob),
            Err(err) => tracing::error!("Hardcoded binary blob path error: {err:?}"),
        }

        self.tabs.insert(new_tab_id_cloned_for_key, new_tab);

        new_tab_id
    }

    /// Get a tab.
    pub fn get_tab(&self, tab_id: &ArcId) -> Option<&Tab> {
        self.tabs.get(tab_id)
    }

    /// Close a tab.
    ///
    /// If the passed-in `tab_id` does not exist, this method does nothing.
    pub fn close_tab(&mut self, tab_id: &ArcId) {
        self.tabs.remove(tab_id);
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

    // TODO: Delete these two tests? They're not unit tests anymore.
    //
    // Consumer crate (nushift)'s tests also call it though...
    #[test]
    fn hypervisor_add_new_tab_adds_new_tab() {
        let mut hypervisor = Hypervisor::new();

        hypervisor.add_new_tab();

        assert_eq!(1, hypervisor.tabs.len());
    }

    #[test]
    fn hypervisor_close_tab_closes_existing_tab() {
        let mut hypervisor = Hypervisor::new();
        let tab_id = hypervisor.add_new_tab();

        hypervisor.close_tab(&tab_id);

        assert_eq!(0, hypervisor.tabs.len());
    }

    #[test]
    fn hypervisor_close_tab_does_nothing_if_tab_does_not_exist() {
        let mut hypervisor = Hypervisor::new();
        let tab_id = ReusableIdPool::new().allocate();

        hypervisor.close_tab(&tab_id);

        assert_eq!(0, hypervisor.tabs.len());
    }
}
