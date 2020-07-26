use druid::Data;
use std::sync::Arc;
use nushift_core::Id;

#[derive(Clone, Data)]
pub struct TabData {
    pub id: Arc<Id>,
    pub title: String,
}
