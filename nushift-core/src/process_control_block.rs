use ckb_vm::{DefaultMachine, DefaultCoreMachine, SparseMemory, SupportMachine};

pub struct ProcessControlBlock {
    machine: Machine,
    exit_reason: ExitReason,
}

enum Machine {
    Unloaded,
    Loaded(DefaultMachine<DefaultCoreMachine<u64, SparseMemory<u64>>>),
}

enum ExitReason {
    NotExited,
    UserExit { exit_reason: u64 },
}

impl ProcessControlBlock {
    pub fn new() -> Self {
        Self { machine: Machine::Unloaded, exit_reason: ExitReason::NotExited }
    }

    pub fn load_machine(&mut self, image: &[u8]) {
        let core_machine = DefaultCoreMachine::<u64, SparseMemory<u64>>::new(
            ckb_vm::ISA_IMC,
            ckb_vm::machine::VERSION1,
            u64::MAX,
        );

        // TODO
    }

    pub fn user_exit(&mut self, exit_reason: u64) {
        if let Machine::Loaded(machine) = &mut self.machine {
            self.exit_reason = ExitReason::UserExit { exit_reason };
            machine.set_running(false);
        }
    }
}
