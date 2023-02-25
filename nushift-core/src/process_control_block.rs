use ckb_vm::machine::{DefaultMachine, DefaultCoreMachine};
use ckb_vm::memory::sparse::SparseMemory;

pub struct ProcessControlBlock {
    machine: Machine,
}

enum Machine {
    Unloaded,
    Loaded(DefaultMachine<DefaultCoreMachine<u64, SparseMemory<u64>>>),
}

impl ProcessControlBlock {
    pub fn new() -> Self {
        Self { machine: Machine::Unloaded }
    }

    pub fn load_machine(&mut self, image: &[u8]) {
        let core_machine = DefaultCoreMachine::<u64, SparseMemory<u64>>::new(
            ckb_vm::ISA_IMC,
            ckb_vm::machine::VERSION1,
            u64::MAX,
        );
    }
}
