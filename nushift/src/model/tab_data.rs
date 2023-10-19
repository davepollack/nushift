use druid::{Data, text::ArcStr};
use reusable_id_pool::ArcId;

use super::client_framebuffer::ClientFramebuffer;

#[derive(Debug, Clone, Data)]
pub struct TabData {
    #[data(eq)]
    pub id: ArcId,
    pub title: ArcStr,
    pub client_framebuffer: Option<ClientFramebuffer>,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use reusable_id_pool::ReusableIdPool;

    pub fn mock() -> TabData {
        let reusable_id_pool = ReusableIdPool::new();
        TabData {
            id: reusable_id_pool.allocate(),
            title: "Mock title".into(),
            client_framebuffer: None,
        }
    }
}
