// Copyright 2023 The reusable-id-pool Authors.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(feature = "std")]
use linked_hash_map::LinkedHashMap;

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::collections::{BTreeSet, VecDeque};

use super::ReusableIdPoolError;

#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkedHashSet(LinkedHashMap<u64, ()>);

#[cfg(feature = "std")]
impl LinkedHashSet {
    fn new() -> Self {
        LinkedHashSet(LinkedHashMap::new())
    }

    fn pop_front(&mut self) -> Option<u64> {
        Some(self.0.pop_front()?.0)
    }

    fn contains(&self, value: &u64) -> bool {
        self.0.contains_key(value)
    }

    fn insert(&mut self, value: u64) -> Option<u64> {
        self.0.insert(value, ()).map(|_| value)
    }
}

/// NoStdFreeList, with multiple data structures, is used so the semantics (FIFO
/// for reused IDs) match the std version.
#[cfg(not(feature = "std"))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NoStdFreeList {
    insertion_order: VecDeque<u64>,
    free_list: BTreeSet<u64>,
}

#[cfg(not(feature = "std"))]
impl NoStdFreeList {
    fn new() -> Self {
        Self { insertion_order: VecDeque::new(), free_list: BTreeSet::new() }
    }

    fn pop_front(&mut self) -> Option<u64> {
        match self.insertion_order.pop_front() {
            Some(item) => {
                self.free_list.remove(&item);
                Some(item)
            },
            None => None,
        }
    }

    fn contains(&self, value: u64) -> bool {
        self.free_list.contains(&value)
    }

    fn push_back(&mut self, value: u64) {
        self.free_list.insert(value);
        self.insertion_order.push_back(value);
    }
}

pub struct ReusableIdPoolManual {
    frontier: u64,
    #[cfg(feature = "std")]
    free_list: LinkedHashSet,
    #[cfg(not(feature = "std"))]
    free_list: NoStdFreeList,
}

impl ReusableIdPoolManual {
    /// Create a new reusable ID pool.
    pub fn new() -> Self {
        ReusableIdPoolManual {
            frontier: 0,
            #[cfg(feature = "std")]
            free_list: LinkedHashSet::new(),
            #[cfg(not(feature = "std"))]
            free_list: NoStdFreeList::new(),
        }
    }

    /// This does not hand out u64::MAX, so you can use that as a sentinel value.
    pub fn allocate(&mut self) -> u64 {
        self.try_allocate().unwrap()
    }

    /// This does not hand out u64::MAX, so you can use that as a sentinel value.
    pub fn try_allocate(&mut self) -> Result<u64, ReusableIdPoolError> {
        if let Some(free_list_id) = self.free_list.pop_front() {
            Ok(free_list_id)
        } else if self.frontier == u64::MAX {
            Err(ReusableIdPoolError::TooManyLiveIDs)
        } else {
            let frontier_id = self.frontier;
            self.frontier += 1;
            Ok(frontier_id)
        }
    }

    /// Silently rejects invalid free requests (double frees and never-allocated), rather than returning an error.
    #[cfg(feature = "std")]
    pub fn release(&mut self, id: u64) {
        if id >= self.frontier {
            return;
        }
        // We have to explicitly check for a double free and not continue,
        // otherwise calling `insert` will change the insertion order.
        if self.free_list.contains(&id) {
            return;
        }
        self.free_list.insert(id);
    }

    /// Silently rejects invalid free requests (double frees and never-allocated), rather than returning an error.
    #[cfg(not(feature = "std"))]
    pub fn release(&mut self, id: u64) {
        if id >= self.frontier {
            return;
        }
        // If it's not past the frontier, then it's either in the free list
        // which is an invalid free (double free), or it's a valid free. If it's
        // an invalid free, we don't continue so we don't corrupt the
        // insertion_order data structure.
        if self.free_list.contains(id) {
            return;
        }
        self.free_list.push_back(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_creates_ids() {
        let mut reusable_id_pool_manual = ReusableIdPoolManual::new();

        let id1 = reusable_id_pool_manual.allocate();
        let id2 = reusable_id_pool_manual.allocate();

        assert_eq!(0, id1);
        assert_eq!(1, id2);
    }

    #[test]
    fn allocate_reuses_released_ids() {
        let mut reusable_id_pool_manual = ReusableIdPoolManual::new();

        let _id1 = reusable_id_pool_manual.allocate();
        let id2 = reusable_id_pool_manual.allocate();
        let id3 = reusable_id_pool_manual.allocate();

        reusable_id_pool_manual.release(id3);
        reusable_id_pool_manual.release(id2);

        let id4 = reusable_id_pool_manual.allocate();
        let id5 = reusable_id_pool_manual.allocate();

        // id4 and id5 should be 2 and 1 (FIFO order of freeing).
        assert_eq!(2, id4);
        assert_eq!(1, id5);
    }

    #[test]
    #[should_panic]
    fn allocate_panics_if_all_ids_are_in_use() {
        let mut reusable_id_pool_manual = ReusableIdPoolManual::new();
        reusable_id_pool_manual.frontier = u64::MAX;
        reusable_id_pool_manual.allocate();
    }

    #[test]
    fn release_rejects_free_request_on_frontier_boundary() {
        let mut reusable_id_pool_manual = ReusableIdPoolManual::new();

        let _id1 = reusable_id_pool_manual.allocate();
        let _id2 = reusable_id_pool_manual.allocate();

        let old_free_list = reusable_id_pool_manual.free_list.clone();
        reusable_id_pool_manual.release(2);
        assert_eq!(old_free_list, reusable_id_pool_manual.free_list);
    }

    #[test]
    fn release_rejects_free_requests_above_frontier() {
        let mut reusable_id_pool_manual = ReusableIdPoolManual::new();

        let _id1 = reusable_id_pool_manual.allocate();
        let _id2 = reusable_id_pool_manual.allocate();

        let old_free_list = reusable_id_pool_manual.free_list.clone();
        reusable_id_pool_manual.release(10);
        assert_eq!(old_free_list, reusable_id_pool_manual.free_list);
    }

    #[test]
    fn release_rejects_double_free_requests() {
        let mut reusable_id_pool_manual = ReusableIdPoolManual::new();

        let _id1 = reusable_id_pool_manual.allocate();
        let id2 = reusable_id_pool_manual.allocate();
        let id3 = reusable_id_pool_manual.allocate();

        reusable_id_pool_manual.release(id2);
        reusable_id_pool_manual.release(id3);
        let old_free_list = reusable_id_pool_manual.free_list.clone();
        // Double-freeing in a reverse order should not even change the FIFO
        // order for reused IDs, since double frees are totally invalid and
        // shouldn't change anything. Eq will check this.
        reusable_id_pool_manual.release(id3);
        reusable_id_pool_manual.release(id2);
        assert_eq!(old_free_list, reusable_id_pool_manual.free_list);
    }
}
