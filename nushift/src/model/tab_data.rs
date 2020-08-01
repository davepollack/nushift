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
