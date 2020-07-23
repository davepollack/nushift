use druid::{Data, Lens};
use std::sync::Arc;
use nushift_core::Id;

#[derive(Clone, Data, Lens)]
pub struct TabData {
    pub id: Arc<Id>,
    pub title: String,
}
