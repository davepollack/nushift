use druid::{ArcStr, Data};
use reusable_id_pool::Id;
use std::sync::Arc;

#[derive(Clone, Data)]
pub struct TabData {
    pub id: Arc<Id>,
    pub title: ArcStr,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use reusable_id_pool::ReusableIdPool;
    use std::sync::{Mutex, Arc};

    pub fn mock() -> TabData {
        let reusable_id_pool = Arc::new(Mutex::new(ReusableIdPool::new()));
        TabData {
            id: ReusableIdPool::allocate(&reusable_id_pool),
            title: "Mock title".into()
        }
    }
}
