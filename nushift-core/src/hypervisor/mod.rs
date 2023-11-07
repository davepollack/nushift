use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::{fs, collections::HashMap};

use reusable_id_pool::{ReusableIdPool, ArcId};

pub(super) mod hypervisor_event;
pub(super) mod tab;
pub(super) mod tab_context;

use crate::gfx_space::GfxOutput;

use self::hypervisor_event::{HypervisorEventHandler, HypervisorEventHandlerFn};
use self::tab::Tab;

pub struct Hypervisor {
    tabs: HashMap<ArcId, Tab>,
    tabs_reusable_id_pool: ReusableIdPool,
    hypervisor_event_handler: HypervisorEventHandler,
}

trait TabLoader {
    fn load(tab: &mut Tab, hypervisor_event_handler: &HypervisorEventHandler);
}

struct RealLoader;
struct MockLoader;

impl TabLoader for RealLoader {
    fn load(tab: &mut Tab, hypervisor_event_handler: &HypervisorEventHandler) {
        let binary_blob_result = fs::read("../examples/hello-world/zig-out/bin/hello-world");
        match binary_blob_result {
            Ok(binary_blob) => tab.load_and_run(binary_blob, Arc::clone(hypervisor_event_handler)),
            Err(err) => tracing::error!("Hardcoded binary blob path error: {err:?}"),
        }
    }
}

impl TabLoader for MockLoader {
    fn load(_tab: &mut Tab, _hypervisor_event_handler: &HypervisorEventHandler) {
        // Intentionally empty
    }
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
    pub fn add_new_tab(&mut self, initial_gfx_output: GfxOutput) -> ArcId {
        self.add_new_tab_impl::<RealLoader>(initial_gfx_output)
    }

    fn add_new_tab_impl<L: TabLoader>(&mut self, initial_gfx_output: GfxOutput) -> ArcId {
        let new_tab_id = self.tabs_reusable_id_pool.allocate();

        let new_tab_id_cloned_for_tab = ArcId::clone(&new_tab_id);
        let new_tab_id_cloned_for_key = ArcId::clone(&new_tab_id);

        let mut new_tab = Tab::new(new_tab_id_cloned_for_tab, initial_gfx_output);
        L::load(&mut new_tab, &self.hypervisor_event_handler);

        self.tabs.insert(new_tab_id_cloned_for_key, new_tab);

        new_tab_id
    }

    /// Close a tab.
    ///
    /// If the passed-in `tab_id` does not exist, this method does nothing.
    pub fn close_tab(&mut self, tab_id: &ArcId) {
        match self.tabs.entry(ArcId::clone(tab_id)) {
            Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().close_tab();
                occupied_entry.remove();
            },
            _ => {},
        }
    }

    /// Update all tab gfx outputs, e.g. when the window scale or size changes.
    ///
    /// When you can have multiple windows (in the future, possibly), you don't
    /// want this to update all tabs but only the tabs in the window affected by
    /// the scale/size change.
    pub fn update_all_tab_gfx_outputs(&mut self, gfx_output: GfxOutput) {
        for tab in self.tabs.values_mut() {
            tab.update_gfx_output(gfx_output.clone());
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

    #[test]
    fn hypervisor_add_new_tab_adds_new_tab() {
        let mut hypervisor = Hypervisor::new(|_| Ok(()));

        hypervisor.add_new_tab_impl::<MockLoader>(GfxOutput::new(vec![1920, 1080], vec![1.25, 1.25]));

        assert_eq!(1, hypervisor.tabs.len());
    }

    #[test]
    fn hypervisor_close_tab_closes_existing_tab() {
        let mut hypervisor = Hypervisor::new(|_| Ok(()));
        let tab_id = hypervisor.add_new_tab_impl::<MockLoader>(GfxOutput::new(vec![1920, 1080], vec![1.25, 1.25]));

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
