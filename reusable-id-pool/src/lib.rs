// Copyright 2023 The reusable-id-pool Authors.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A pool for RAII IDs.
//!
//! ## Example
//!
//! ```rust
//! use reusable_id_pool::ReusableIdPool;
//!
//! let reusable_id_pool = ReusableIdPool::new();
//! let id = reusable_id_pool.allocate();
//! // Do something with the `id`, like move it into a struct. It will be returned to the pool when it is dropped.
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

use core::fmt::{self, Display, Debug};

#[cfg(feature = "std")]
mod reusable_id_pool;

mod reusable_id_pool_manual;

#[cfg(feature = "std")]
pub use crate::reusable_id_pool::{ReusableIdPool, ArcId};

pub use crate::reusable_id_pool_manual::ReusableIdPoolManual;

pub enum ReusableIdPoolError {
    TooManyLiveIDs,
}

impl Display for ReusableIdPoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyLiveIDs => write!(f, "There are too many IDs concurrently in use. The limit is (2^64 - 1) live IDs. Please release some IDs."),
        }
    }
}

impl Debug for ReusableIdPoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyLiveIDs => write!(f, "{} (TooManyLiveIDs)", self),
        }
    }
}

// Change to core when error_in_core is stabilised.
#[cfg(feature = "std")]
impl std::error::Error for ReusableIdPoolError {}
