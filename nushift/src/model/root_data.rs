use std::sync::Arc;
use druid::{Data, Lens};

use super::tab_data::{TabData};

#[derive(Clone, Data, Lens)]
pub struct RootData {
    pub tabs: Arc<Vec<TabData>>
}
