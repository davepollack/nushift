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
//! ```
//! # #[cfg(feature = "std")]
//! use reusable_id_pool::ReusableIdPool;
//!
//! # #[cfg(feature = "std")]
//! let reusable_id_pool = ReusableIdPool::new();
//! # #[cfg(feature = "std")]
//! let id = reusable_id_pool.allocate();
//! // Do something with the `id`, like move it into a struct. It will be
//! // returned to the pool when it is dropped.
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

use core::fmt::{self, Display, Debug};

#[cfg(feature = "std")]
mod reusable_id_pool;

mod reusable_id_pool_manual;

#[cfg(feature = "std")]
pub use crate::reusable_id_pool::{ReusableIdPool, ArcId};

pub use crate::reusable_id_pool_manual::ReusableIdPoolManual;

/// The error type for reusable ID pool operations.
pub enum ReusableIdPoolError {
    /// There are too many IDs concurrently in use. The limit is 2<sup>64</sup>
    /// &minus; 1 live IDs. Please return some IDs to the pool.
    TooManyLiveIDs,
}

impl Display for ReusableIdPoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyLiveIDs => write!(f, "There are too many IDs concurrently in use. The limit is (2^64 - 1) live IDs. Please return some IDs to the pool."),
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
