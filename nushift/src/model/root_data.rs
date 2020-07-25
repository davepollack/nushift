use std::sync::Arc;
use druid::{Data, Lens};
use nushift_core::Id;

use super::tab_data::{TabData};

#[derive(Clone, Data, Lens)]
pub struct RootData {
    pub tabs: Arc<Vec<TabData>>,
    pub currently_selected_tab_id: Arc<Id>
}
