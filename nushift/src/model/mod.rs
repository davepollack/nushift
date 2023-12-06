// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod root_data;
pub(crate) mod tab_data;
mod combined;
pub(crate) mod scale_and_size;
pub(crate) mod client_framebuffer;

pub use root_data::RootData;
pub use tab_data::TabData;
pub use combined::RootAndTabData;
