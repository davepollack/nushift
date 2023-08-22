use druid::{Data, text::ArcStr};
use reusable_id_pool::ArcId;

#[derive(Clone, Data)]
pub struct TabData {
    #[data(eq)]
    pub id: ArcId,
    pub title: ArcStr,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use reusable_id_pool::ReusableIdPool;

    pub fn mock() -> TabData {
        let reusable_id_pool = ReusableIdPool::new();
        TabData {
            id: reusable_id_pool.allocate(),
            title: "Mock title".into()
        }
    }
}
