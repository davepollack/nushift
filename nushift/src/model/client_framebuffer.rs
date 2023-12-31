// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use druid::{Data, Size};
use druid::im::Vector;
use nushift_core::PresentBufferFormat;

#[derive(Debug, Clone, Data)]
pub struct ClientFramebuffer {
    #[data(eq)]
    pub present_buffer_format: PresentBufferFormat,
    pub size_px: Vector<u64>,
    pub framebuffer: Arc<[u8]>,
}

impl ClientFramebuffer {
    pub fn druid_2d_size(&self) -> Option<Size> {
        // It is intentional that this matches an exact length of 2 and higher
        // lengths should not match.
        if let [&width, &height] = self.size_px.iter().collect::<Vec<_>>().as_slice() {
            // I really would like `TryFrom`/a better conversion than `as` to be
            // supported for u64 to f64
            Some(Size::new(width as f64, height as f64))
        } else {
            None
        }
    }

    pub fn usize_2d_size(&self) -> Option<(usize, usize)> {
        // It is intentional that this matches an exact length of 2 and higher
        // lengths should not match.
        let [&width, &height] = self.size_px.iter().collect::<Vec<_>>().as_slice() else { return None; };

        let width = usize::try_from(width).ok()?;
        let height = usize::try_from(height).ok()?;

        Some((width, height))
    }
}
