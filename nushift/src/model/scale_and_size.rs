// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use core::iter;
use std::fmt::Debug;

use druid::{Data, Scale, Size};
use druid::im::{Vector, vector};
use nushift_core::GfxOutput;

#[derive(Debug, Clone, Data)]
pub struct ScaleAndSize {
    pub window_scale: Vector<f64>,
    pub client_area_size_dp: Vector<f64>,
}

impl ScaleAndSize {
    pub fn gfx_output(&self, id: u64) -> GfxOutput {
        let size_px: Vec<u64> = iter::zip(&self.window_scale, &self.client_area_size_dp)
            .map(|(scale_dimension, dp_dimension)| (scale_dimension * dp_dimension).round() as u64)
            .collect();

        GfxOutput::new(id, size_px, self.window_scale.iter().cloned().collect())
    }
}

impl From<(Scale, Size)> for ScaleAndSize {
    fn from((scale, size): (Scale, Size)) -> Self {
        Self {
            window_scale: vector![scale.x(), scale.y()],
            client_area_size_dp: vector![size.width, size.height],
        }
    }
}
