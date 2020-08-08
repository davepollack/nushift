use druid::{im::Vector, Data};
use std::sync::Arc;
use nushift_core::Id;
use super::RootData;

pub type TabAndSharedRootData = (RootData, TabData);
pub type TabListAndSharedRootData = (RootData, Vector<TabData>);

#[derive(Clone, Data)]
pub struct TabData {
    pub id: Arc<Id>,
    pub title: String,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::{Mutex, Arc};
    use nushift_core::ReusableIdPool;
    use druid::im::vector;

    pub fn mock() -> TabData {
        let reusable_id_pool = Arc::new(Mutex::new(ReusableIdPool::new()));
        TabData {
            id: ReusableIdPool::allocate(&reusable_id_pool),
            title: "Mock title".into()
        }
    }

    pub fn mock_tab_and_shared_root_data() -> TabAndSharedRootData {
        let mut mock_root_data = super::super::root_data::tests::mock();
        let mock_tab_data = mock();
        // Set up mock_root_data
        let tab_id = Arc::clone(&mock_tab_data.id);
        mock_root_data.tabs.push_back(mock_tab_data);
        mock_root_data.currently_selected_tab_id = Some(tab_id);

        (mock_root_data.clone(), mock_root_data.tabs.front().unwrap().clone())
    }

    pub fn mock_tab_list_and_shared_root_data() -> TabListAndSharedRootData {
        let (mock_root_data, mock_tab_data) = mock_tab_and_shared_root_data();

        (mock_root_data.clone(), vector![mock_tab_data.clone()])
    }
}
