// Copyright 2023 The reusable-id-pool Authors.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate alloc;

use core::fmt::Debug;
use alloc::sync::Arc;
use std::sync::Mutex;

use super::ReusableIdPoolError;

#[derive(Debug)]
pub struct ReusableIdPool(Arc<Mutex<ReusableIdPoolInternal>>);

#[derive(Debug)]
struct ReusableIdPoolInternal {
    frontier: u64,
    free_list: Vec<u64>,
}

struct Id {
    per_pool_id: u64,
    pool: Arc<Mutex<ReusableIdPoolInternal>>,
}

impl Debug for Id {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Id")
            .field("per_pool_id", &self.per_pool_id)
            .finish_non_exhaustive()
    }
}

impl Drop for Id {
    fn drop(&mut self) {
        let mut pool = self.pool.lock().unwrap();
        pool.free_list.push(self.per_pool_id);
    }
}

/// An RAII reference to an ID.
///
/// This has no "release" or "free" methods --- the underlying ID is released
/// when the last [`ArcId`] to it is dropped.
#[derive(Debug)]
pub struct ArcId(Arc<Id>);

impl Clone for ArcId {
    /// Clones this [`ArcId`], which creates a new reference to the underlying
    /// ID.
    ///
    /// Multiple [`ArcId`]s created through [`ArcId::clone`] are compared
    /// ([`ArcId::eq`]) as equal.
    ///
    /// # Example
    ///
    /// ```
    /// use reusable_id_pool::{ArcId, ReusableIdPool};
    ///
    /// let reusable_id_pool = ReusableIdPool::new();
    ///
    /// let id_ref_1 = reusable_id_pool.allocate();
    /// let id_ref_2 = ArcId::clone(&id_ref_1); // Or `id_ref_1.clone();`
    ///
    /// assert_eq!(id_ref_1, id_ref_2);
    /// ```
    #[inline]
    fn clone(&self) -> Self {
        ArcId(Arc::clone(&self.0))
    }
}

impl PartialEq for ArcId {
    /// Returns if this [`ArcId`] is the same as the other [`ArcId`].
    ///
    /// When creating a new reference to an ID with [`ArcId::clone`], those
    /// [`ArcId`]s are considered the same.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ArcId {}

impl std::hash::Hash for ArcId {
    /// Hashes this [`ArcId`].
    ///
    /// It is suitable to use [`ArcId`] as a key in
    /// [`HashMap`][std::collections::HashMap] or
    /// [`HashSet`][std::collections::HashSet], and use one reference to an ID to
    /// look up another reference to the same ID, as we satisfy `k1 == k2 ->
    /// hash(k1) == hash(k2)`.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl PartialOrd for ArcId {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ArcId {
    /// Compares [`ArcId`]s. Like [`ArcId::eq`], multiple references to the same
    /// ID created with [`ArcId::clone`] are ordered as equal.
    ///
    /// Different IDs are ordered deterministically, and as not equal, but in an
    /// otherwise unspecified manner.
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        Arc::as_ptr(&self.0).cmp(&Arc::as_ptr(&other.0))
    }
}

impl ReusableIdPool {
    /// Create a new reusable ID pool.
    pub fn new() -> Self {
        ReusableIdPool(Arc::new(Mutex::new(ReusableIdPoolInternal {
            frontier: 0,
            free_list: vec![],
        })))
    }

    pub fn allocate(&self) -> ArcId {
        self.try_allocate().unwrap()
    }

    pub fn try_allocate(&self) -> Result<ArcId, ReusableIdPoolError> {
        let mut pool = self.0.lock().unwrap();

        if let Some(free_list_id) = pool.free_list.pop() {
            Ok(ArcId(Arc::new(Id {
                per_pool_id: free_list_id,
                pool: Arc::clone(&self.0),
            })))
        } else if pool.frontier == u64::MAX {
            Err(ReusableIdPoolError::TooManyLiveIDs)
        } else {
            let frontier_arc_id = ArcId(Arc::new(Id {
                per_pool_id: pool.frontier,
                pool: Arc::clone(&self.0),
            }));
            pool.frontier += 1;
            Ok(frontier_arc_id)
        }
    }

    // Releasing logic is found in the `Drop` impl for `Id`.
}

#[cfg(test)]
mod tests {
    use std::{collections::hash_map::DefaultHasher, hash::{Hash, Hasher}};

    use super::*;

    #[test]
    fn allocate_creates_ids() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = reusable_id_pool.allocate();
        let id2 = reusable_id_pool.allocate();

        assert_eq!(0, id1.0.per_pool_id);
        assert_eq!(1, id2.0.per_pool_id);
    }

    #[test]
    fn allocate_reuses_per_pool_ids_that_have_been_dropped() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = reusable_id_pool.allocate();
        let id2 = reusable_id_pool.allocate();
        let id3 = reusable_id_pool.allocate();

        drop(id1);
        drop(id2);

        let id4 = reusable_id_pool.allocate();

        assert_eq!(2, id3.0.per_pool_id);
        // FILO, should reuse id2's id. I.e. 1.
        assert_eq!(1, id4.0.per_pool_id);
    }

    #[test]
    #[should_panic]
    fn allocate_panics_if_all_per_pool_ids_are_in_use() {
        let reusable_id_pool = ReusableIdPool::new();
        {
            let mut pool = reusable_id_pool.0.lock().unwrap();
            pool.frontier = u64::MAX;
        }
        reusable_id_pool.allocate();
    }

    #[test]
    fn arcid_eq_returns_false_if_different_ids() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = reusable_id_pool.allocate();
        let id2 = reusable_id_pool.allocate();

        assert_ne!(id1, id2);
    }

    #[test]
    fn arcid_eq_returns_true_if_same_id() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = reusable_id_pool.allocate();
        let id2 = ArcId::clone(&id1);

        assert_eq!(id1, id2);
    }

    #[test]
    fn arcid_hash_is_equal_if_same_id() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = reusable_id_pool.allocate();
        let id2 = ArcId::clone(&id1);

        let mut hasher = DefaultHasher::new();
        id1.hash(&mut hasher);
        let hash_1 = hasher.finish();

        let mut hasher = DefaultHasher::new();
        id2.hash(&mut hasher);
        let hash_2 = hasher.finish();

        assert_eq!(hash_1, hash_2);
    }

    #[test]
    fn arcid_cmp_different_if_different_ids() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = reusable_id_pool.allocate();
        let id2 = reusable_id_pool.allocate();

        assert!(id1.cmp(&id2).is_ne());
    }

    #[test]
    fn arcid_cmp_equal_if_same_id() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = reusable_id_pool.allocate();
        let id2 = ArcId::clone(&id1);

        assert!(id1.cmp(&id2).is_eq());
    }
}
