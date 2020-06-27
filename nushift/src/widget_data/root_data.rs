use std::sync::Arc;
use druid::{Data, Lens};

use crate::widget_data::tabs::{TabData};

#[derive(Clone, Data, Lens)]
pub struct RootData {
    pub tabs: Arc<Vec<TabData>>
}
