use druid::{Data, Lens};

#[derive(Clone, Data, Lens)]
pub struct TabData {
    pub tab_title: String
}
