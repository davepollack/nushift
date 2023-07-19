// TODO: Use linked_hash_map when std is in configuration.

use std::collections::BTreeSet;

use super::ReusableIdPoolError;

pub struct ReusableIdPoolManual {
    frontier: u64,
    free_list: BTreeSet<u64>,
}

impl ReusableIdPoolManual {
    /// Create a new reusable ID pool.
    pub fn new() -> Self {
        ReusableIdPoolManual {
            frontier: 0,
            free_list: BTreeSet::new(),
        }
    }

    /// This does not hand out u64::MAX, so you can use that as a sentinel value.
    pub fn allocate(&mut self) -> u64 {
        self.try_allocate().unwrap()
    }

    /// This does not hand out u64::MAX, so you can use that as a sentinel value.
    pub fn try_allocate(&mut self) -> Result<u64, ReusableIdPoolError> {
        if let Some(free_list_id) = self.free_list.pop_first() {
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
    pub fn release(&mut self, id: u64) {
        if id >= self.frontier {
            return;
        }
        self.free_list.insert(id);
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

        // id4 and id5 should be 1 and 2 again.
        assert_eq!(1, id4);
        assert_eq!(2, id5);
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

        reusable_id_pool_manual.release(id2);
        let old_free_list = reusable_id_pool_manual.free_list.clone();
        reusable_id_pool_manual.release(id2);
        assert_eq!(old_free_list, reusable_id_pool_manual.free_list);
    }
}
