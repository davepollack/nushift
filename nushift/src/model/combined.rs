use druid::im::Vector;
use super::{RootData, TabData};

pub type RootAndTabData = (RootData, TabData);
pub type RootAndVectorTabData = (RootData, Vector<TabData>);

#[cfg(test)]
pub mod tests {
    use super::*;
    use druid::im::vector;
    use reusable_id_pool::ArcId;

    pub fn mock_root_and_tab_data() -> RootAndTabData {
        let mut mock_root_data = super::super::root_data::tests::mock();
        let mock_tab_data = super::super::tab_data::tests::mock();
        // Set up mock_root_data
        let tab_id = ArcId::clone(&mock_tab_data.id);
        mock_root_data.tabs.push_back(mock_tab_data);
        mock_root_data.currently_selected_tab_id = Some(tab_id);

        (mock_root_data.clone(), mock_root_data.tabs.front().unwrap().clone())
    }

    pub fn mock_root_and_vector_tab_data() -> RootAndVectorTabData {
        let (mock_root_data, mock_tab_data) = mock_root_and_tab_data();

        (mock_root_data.clone(), vector![mock_tab_data.clone()])
    }
}
