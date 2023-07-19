use core::fmt::{self, Display, Debug};

mod reusable_id_pool;
mod reusable_id_pool_manual;

pub use crate::reusable_id_pool::{ReusableIdPool, Id, ArcId};
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
