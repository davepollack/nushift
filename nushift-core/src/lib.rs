pub mod hypervisor {
    mod hypervisor;
    mod tab;

    pub use hypervisor::Hypervisor;
}

mod reusable_id_pool;

pub use reusable_id_pool::{ReusableIdPool, Id, IdEq};
