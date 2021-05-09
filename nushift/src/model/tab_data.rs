use druid::{ArcStr, Data};
use std::sync::Arc;
use nushift_core::Id;

#[derive(Clone, Data)]
pub struct TabData {
    pub id: Arc<Id>,
    pub title: ArcStr,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::{Mutex, Arc};
    use nushift_core::ReusableIdPool;

    pub fn mock() -> TabData {
        let reusable_id_pool = Arc::new(Mutex::new(ReusableIdPool::new()));
        TabData {
            id: ReusableIdPool::allocate(&reusable_id_pool),
            title: "Mock title".into()
        }
    }
}
