use std::sync::Arc;

use druid::Data;
use nushift_core::PresentBufferFormat;

#[derive(Debug, Clone, Data)]
pub struct ClientFramebuffer {
    #[data(eq)]
    pub present_buffer_format: PresentBufferFormat,
    pub framebuffer: Arc<[u8]>,
}
