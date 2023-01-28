// TODO: Before publishing this crate, it would be cool to have an
// `allocate_rc(...) -> RcId` alternative to `allocate(...) -> ArcId`.

use std::{sync::{Arc, Mutex}, hash::Hash};
use snafu::Snafu;
use snafu_cli_debug::SnafuCliDebug;

#[derive(Debug)]
pub struct ReusableIdPool {
    frontier: u64,
    free_list: Vec<u64>,
}

#[derive(Snafu, SnafuCliDebug)]
pub enum ReusableIdPoolError {
    #[snafu(display("There are too many IDs concurrently in use. The limit is 2^64 concurrent IDs. Please release some IDs."))]
    Exhausted,
}

#[derive(Debug)]
pub struct Id {
    per_pool_id: u64,
    pool: Arc<Mutex<ReusableIdPool>>,
}

impl Drop for Id {
    fn drop(&mut self) {
        let mut pool = self.pool.lock().unwrap();
        pool.release(self.per_pool_id);
    }
}

#[derive(Debug, Clone)]
pub struct ArcId(Arc<Id>);

impl PartialEq for ArcId {
    /// Returns if this ID is the same as the other ID.
    ///
    /// When creating a new reference to an ID with `ArcId::clone(&id)`, those IDs
    /// are considered the same.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ArcId {}
impl Hash for ArcId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}
impl PartialOrd for ArcId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ArcId {
    /// Similarly to PartialEq, multiple references to the same ID created with
    /// `ArcId::clone(&id)` should be ordered as equal.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Arc::as_ptr(&self.0).cmp(&Arc::as_ptr(&other.0))
    }
}

impl ReusableIdPool {
    /// Create a new reusable ID pool.
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(ReusableIdPool {
            frontier: 0,
            free_list: vec![]
        }))
    }

    pub fn allocate(reusable_id_pool: &Arc<Mutex<ReusableIdPool>>) -> ArcId {
        Self::try_allocate(reusable_id_pool).unwrap()
    }

    pub fn try_allocate(reusable_id_pool: &Arc<Mutex<ReusableIdPool>>) -> Result<ArcId, ReusableIdPoolError> {
        let mut pool = reusable_id_pool.lock().unwrap();

        if let Some(free_list_id) = pool.free_list.pop() {
            Ok(ArcId(Arc::new(Id {
                per_pool_id: free_list_id,
                pool: Arc::clone(reusable_id_pool),
            })))
        } else if pool.frontier == u64::MAX {
            ExhaustedSnafu.fail()
        } else {
            let frontier_arc_id = ArcId(Arc::new(Id {
                per_pool_id: pool.frontier,
                pool: Arc::clone(reusable_id_pool),
            }));
            pool.frontier += 1;
            Ok(frontier_arc_id)
        }
    }

    /// Only called by `Id`'s `Drop`, not available publicly, hence we can ensure
    /// the free list is unique.
    fn release(&mut self, per_pool_id: u64) {
        self.free_list.push(per_pool_id);
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::hash_map::DefaultHasher, hash::Hasher};

    use super::*;

    #[test]
    fn allocate_creates_ids() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
        let id2 = ReusableIdPool::allocate(&reusable_id_pool);

        assert_eq!(0, id1.0.per_pool_id);
        assert_eq!(1, id2.0.per_pool_id);
    }

    #[test]
    fn allocate_reuses_per_pool_ids_that_have_been_dropped() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
        let id2 = ReusableIdPool::allocate(&reusable_id_pool);
        let id3 = ReusableIdPool::allocate(&reusable_id_pool);

        drop(id1);
        drop(id2);

        let id4 = ReusableIdPool::allocate(&reusable_id_pool);

        assert_eq!(2, id3.0.per_pool_id);
        // FILO, should reuse id2's id. I.e. 1.
        assert_eq!(1, id4.0.per_pool_id);
    }

    #[test]
    #[should_panic]
    fn allocate_panics_if_all_per_pool_ids_are_in_use() {
        let reusable_id_pool = ReusableIdPool::new();
        {
            let mut pool = reusable_id_pool.lock().unwrap();
            pool.frontier = u64::MAX;
        }
        ReusableIdPool::allocate(&reusable_id_pool);
    }

    #[test]
    fn arcid_eq_returns_false_if_different_ids() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
        let id2 = ReusableIdPool::allocate(&reusable_id_pool);

        assert_ne!(id1, id2);
    }

    #[test]
    fn arcid_eq_returns_true_if_same_id() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
        let id2 = ArcId::clone(&id1);

        assert_eq!(id1, id2);
    }

    #[test]
    fn arcid_hash_is_equal_if_same_id() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
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

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
        let id2 = ReusableIdPool::allocate(&reusable_id_pool);

        assert!(id1.cmp(&id2).is_ne());
    }

    #[test]
    fn arcid_cmp_equal_if_same_id() {
        let reusable_id_pool = ReusableIdPool::new();

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
        let id2 = ArcId::clone(&id1);

        assert!(id1.cmp(&id2).is_eq());
    }
}
