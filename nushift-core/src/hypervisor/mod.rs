use std::sync::Arc;
use std::{fs, collections::HashMap};

use reusable_id_pool::{ReusableIdPool, ArcId};

pub(super) mod hypervisor_event;
pub(super) mod tab;

use self::hypervisor_event::{HypervisorEventHandler, HypervisorEventHandlerFn};
use self::tab::{Tab, Output};

pub struct Hypervisor {
    tabs: HashMap<ArcId, Tab>,
    tabs_reusable_id_pool: ReusableIdPool,
    hypervisor_event_handler: HypervisorEventHandler,
}

impl Hypervisor {
    /// Create a hypervisor.
    pub fn new<H: HypervisorEventHandlerFn>(hypervisor_event_handler: H) -> Self {
        Hypervisor {
            tabs: HashMap::new(),
            tabs_reusable_id_pool: ReusableIdPool::new(),
            hypervisor_event_handler: Arc::new(hypervisor_event_handler),
        }
    }

    /// Add a new tab.
    ///
    /// Internally, this generates an ID for the new tab, based on an ID pool
    /// owned by the `Hypervisor`.
    ///
    /// The newly-created ID is returned.
    pub fn add_new_tab(&mut self, initial_output: Output) -> ArcId {
        let new_tab_id = self.tabs_reusable_id_pool.allocate();

        let new_tab_id_cloned_for_tab = ArcId::clone(&new_tab_id);
        let new_tab_id_cloned_for_key = ArcId::clone(&new_tab_id);

        let mut new_tab = Tab::new(new_tab_id_cloned_for_tab, Arc::clone(&self.hypervisor_event_handler), initial_output);

        let binary_blob_result = fs::read("../examples/hello-world/zig-out/bin/hello-world");
        match binary_blob_result {
            Ok(binary_blob) => new_tab.load_and_run(binary_blob),
            Err(err) => tracing::error!("Hardcoded binary blob path error: {err:?}"),
        }

        self.tabs.insert(new_tab_id_cloned_for_key, new_tab);

        new_tab_id
    }

    /// Close a tab.
    ///
    /// If the passed-in `tab_id` does not exist, this method does nothing.
    pub fn close_tab(&mut self, tab_id: &ArcId) {
        self.tabs.remove(tab_id);
    }

    /// Update all tab outputs, e.g. when the window scale or size changes.
    ///
    /// When you can have multiple windows (in the future, possibly), you don't
    /// want this to update all tabs but only the tabs in the window affected by
    /// the output change.
    pub fn update_all_tab_outputs(&mut self, output: Output) {
        for tab in self.tabs.values_mut() {
            tab.update_output(output.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypervisor_new_creates_new() {
        let hypervisor = Hypervisor::new(|_| Ok(()));

        assert_eq!(0, hypervisor.tabs.len());
    }

    // TODO: Delete these two tests? They're not unit tests anymore.
    //
    // Consumer crate (nushift)'s tests also call it though...
    #[test]
    fn hypervisor_add_new_tab_adds_new_tab() {
        let mut hypervisor = Hypervisor::new(|_| Ok(()));

        hypervisor.add_new_tab(Output::new(vec![1920, 1080], vec![1.25, 1.25]));

        assert_eq!(1, hypervisor.tabs.len());
    }

    #[test]
    fn hypervisor_close_tab_closes_existing_tab() {
        let mut hypervisor = Hypervisor::new(|_| Ok(()));
        let tab_id = hypervisor.add_new_tab(Output::new(vec![1920, 1080], vec![1.25, 1.25]));

        hypervisor.close_tab(&tab_id);

        assert_eq!(0, hypervisor.tabs.len());
    }

    #[test]
    fn hypervisor_close_tab_does_nothing_if_tab_does_not_exist() {
        let mut hypervisor = Hypervisor::new(|_| Ok(()));
        let tab_id = ReusableIdPool::new().allocate();

        hypervisor.close_tab(&tab_id);

        assert_eq!(0, hypervisor.tabs.len());
    }
}
