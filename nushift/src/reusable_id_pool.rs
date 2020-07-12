// TODO: move into `nushift-core` crate.

// TODO: if making this module a public crate, would be cool to have an
// `allocate_rc(...) -> Rc<Id>` alternative to `allocate(...) -> Arc<Id>`.

use std::sync::{Arc, Mutex};

pub struct ReusableIdPool {
    frontier: u64,
    free_list: Vec<u64>,
}

pub struct Id {
    id: u64,
    pool: Arc<Mutex<ReusableIdPool>>,
}

impl Drop for Id {
    fn drop(&mut self) {
        let mut pool = self.pool.lock().unwrap();
        pool.release(self.id);
    }
}

impl ReusableIdPool {
    pub fn new() -> Self {
        ReusableIdPool {
            frontier: 0,
            free_list: vec![]
        }
    }

    pub fn allocate(reusable_id_pool: &Arc<Mutex<ReusableIdPool>>) -> Arc<Id> {
        let mut pool = reusable_id_pool.lock().unwrap();

        if !pool.free_list.is_empty() {
            Arc::new(Id {
                id: pool.free_list.pop().unwrap(),
                pool: Arc::clone(reusable_id_pool),
            })
        } else {
            let frontier_rc_id = Arc::new(Id {
                id: pool.frontier,
                pool: Arc::clone(reusable_id_pool),
            });
            pool.frontier += 1;
            frontier_rc_id
        }
    }

    /// Only called by `Id`'s `Drop`, not available publicly, hence we can ensure
    /// the free list is unique.
    fn release(&mut self, id: u64) {
        self.free_list.push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_creates_ids() {
        let reusable_id_pool = Arc::new(Mutex::new(ReusableIdPool::new()));

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
        let id2 = ReusableIdPool::allocate(&reusable_id_pool);

        assert_eq!(0, id1.id);
        assert_eq!(1, id2.id);
    }

    #[test]
    fn allocate_reuses_ids_that_have_been_dropped() {
        let reusable_id_pool = Arc::new(Mutex::new(ReusableIdPool::new()));

        let id1 = ReusableIdPool::allocate(&reusable_id_pool);
        let id2 = ReusableIdPool::allocate(&reusable_id_pool);
        let id3 = ReusableIdPool::allocate(&reusable_id_pool);

        drop(id1);
        drop(id2);

        let id4 = ReusableIdPool::allocate(&reusable_id_pool);

        assert_eq!(2, id3.id);
        // FILO, should reuse id2's id. I.e. 1.
        assert_eq!(1, id4.id);
    }
}
