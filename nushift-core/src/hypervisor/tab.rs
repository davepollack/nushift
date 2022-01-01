use std::sync::Arc;
use crate::reusable_id_pool::Id;

pub struct Tab {
    id: Arc<Id>,
    title: String,
    // TODO: Add emulated_machine field.
}

impl Tab {
    pub fn new<S: Into<String>>(id: Arc<Id>, title: S) -> Self {
        Tab { id, title: title.into() }
    }

    pub fn id(&self) -> &Arc<Id> {
        &self.id
    }

    // TODO remove the below suppression when `title()` is used
    #[allow(dead_code)]
    pub fn title(&self) -> &str {
        &self.title
    }
}
