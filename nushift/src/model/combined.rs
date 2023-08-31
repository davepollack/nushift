use druid::{widget::ListIter, Data};
use super::{RootData, TabData};

/// A struct of root and a tab data for a particular tab widget.
///
/// Only certain methods are exposed, to keep the root and tab in sync. Tab
/// widgets can call close_tab() on the root data for instance. Or they can
/// update a tab with get_tab_data_mut(). In this implementation, only the
/// RootData is actually stored, so that is how keeping it in sync is achieved.
#[derive(Debug, Clone, Data)]
pub struct RootAndTabData {
    root_data: RootData,
    tab_data_index: usize,
}

impl RootAndTabData {
    pub fn new(root_data: RootData, tab_data_index: usize) -> Self {
        Self { root_data, tab_data_index }
    }

    pub fn root_data(&self) -> &RootData {
        &self.root_data
    }

    pub fn root_data_mut(&mut self) -> &mut RootData {
        &mut self.root_data
    }

    pub fn tab_data(&self) -> &TabData {
        &self.root_data.tabs[self.tab_data_index] // I think panicking here for out of bounds is okay
    }

    pub fn tab_data_mut(&mut self) -> &mut TabData {
        &mut self.root_data.tabs[self.tab_data_index] // I think panicking here for out of bounds is okay
    }

    pub fn tab_data_cloned(&self) -> TabData {
        self.root_data.tabs[self.tab_data_index].clone() // I think panicking here for out of bounds is okay
    }

    fn consume(self) -> RootData {
        self.root_data
    }
}

impl ListIter<RootAndTabData> for RootData {
    fn for_each(&self, mut cb: impl FnMut(&RootAndTabData, usize)) {
        for i in 0..self.tabs.len() {
            let data = RootAndTabData::new(self.clone(), i);
            cb(&data, i);
        }
    }

    fn for_each_mut(&mut self, mut cb: impl FnMut(&mut RootAndTabData, usize)) {
        for i in 0..self.tabs.len() {
            let mut data = RootAndTabData::new(self.clone(), i);
            cb(&mut data, i);

            if !self.same(data.root_data()) {
                *self = data.consume();
            }
        }
    }

    fn data_len(&self) -> usize {
        self.tabs.len()
    }
}
