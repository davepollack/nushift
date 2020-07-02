use std::sync::Arc;
use druid::{Data, Lens};

use super::tabs::{TabData};

#[derive(Clone, Data, Lens)]
pub struct RootData {
    pub tabs: Arc<Vec<TabData>>
}
