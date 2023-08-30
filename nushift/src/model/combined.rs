use druid::{im::Vector, widget::ListIter, Data};
use super::{RootData, TabData};

pub type RootAndVectorTabData = (RootData, Vector<TabData>);

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

impl ListIter<RootAndTabData> for RootAndVectorTabData {
    fn for_each(&self, mut cb: impl FnMut(&RootAndTabData, usize)) {
        for (i, _) in self.1.iter().enumerate() {
            let data = RootAndTabData::new(self.0.clone(), i);
            cb(&data, i);
        }
    }

    fn for_each_mut(&mut self, mut cb: impl FnMut(&mut RootAndTabData, usize)) {
        for (i, _) in self.1.clone().iter().enumerate() {
            let mut data = RootAndTabData::new(self.0.clone(), i);
            cb(&mut data, i);

            if !self.0.same(data.root_data()) {
                self.0 = data.consume();
            }
        }
    }

    fn data_len(&self) -> usize {
        self.1.len()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use druid::im::vector;
    use reusable_id_pool::ArcId;

    pub fn mock_root_and_vector_tab_data() -> RootAndVectorTabData {
        let mut mock_root_data = super::super::root_data::tests::mock();
        let mock_tab_data = super::super::tab_data::tests::mock();
        // Set up mock_root_data
        let tab_id = ArcId::clone(&mock_tab_data.id);
        mock_root_data.tabs.push_back(mock_tab_data.clone());
        mock_root_data.currently_selected_tab_id = Some(tab_id);

        (mock_root_data, vector![mock_tab_data])
    }
}
