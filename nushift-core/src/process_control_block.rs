use ckb_vm::{
    DefaultCoreMachine,
    SparseMemory,
    SupportMachine,
    CoreMachine,
    Register,
    Machine as CKBVMMachine,
    Error as CKBVMError,
};

pub struct ProcessControlBlock<R = u64> {
    machine: Machine<R>,
    exit_reason: ExitReason,
}

enum Machine<R> {
    Unloaded,
    Loaded(DefaultCoreMachine<R, SparseMemory<R>>),
    Running(DefaultCoreMachine<R, SparseMemory<R>>),
    Stopped,
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

const PANIC_MESSAGE: &str = "process_control_block.rs: Machine attempted to be used before loaded";
macro_rules! proxy_to_self_machine {
    ($self:ident, $name:ident$(, $arg:expr)*) => {
        match &$self.machine {
            Machine::Loaded(core_machine) | Machine::Running(core_machine) => core_machine.$name($($arg),*),
            _ => panic!("{}", PANIC_MESSAGE)
        }
    };
    (mut $self:ident, $name:ident$(, $arg:expr)*) => {
        match &mut $self.machine {
            Machine::Loaded(core_machine) | Machine::Running(core_machine) => core_machine.$name($($arg),*),
            _ => panic!("{}", PANIC_MESSAGE)
        }
    };
}

impl<R: Register> CoreMachine for ProcessControlBlock<R> {
    type REG = R;
    type MEM = SparseMemory<R>;

    fn pc(&self) -> &Self::REG {
        proxy_to_self_machine!(self, pc)
    }

    fn update_pc(&mut self, pc: Self::REG) {
        proxy_to_self_machine!(mut self, update_pc, pc)
    }

    fn commit_pc(&mut self) {
        proxy_to_self_machine!(mut self, commit_pc)
    }

    fn memory(&self) -> &Self::MEM {
        proxy_to_self_machine!(self, memory)
    }

    fn memory_mut(&mut self) -> &mut Self::MEM {
        proxy_to_self_machine!(mut self, memory_mut)
    }

    fn registers(&self) -> &[Self::REG] {
        proxy_to_self_machine!(self, registers)
    }

    fn set_register(&mut self, idx: usize, value: Self::REG) {
        proxy_to_self_machine!(mut self, set_register, idx, value)
    }

    fn version(&self) -> u32 {
        proxy_to_self_machine!(self, version)
    }

    fn isa(&self) -> u8 {
        proxy_to_self_machine!(self, isa)
    }
}

impl<R: Register> CKBVMMachine for ProcessControlBlock<R> {
    fn ecall(&mut self) -> Result<(), CKBVMError> {
        todo!()
    }

    fn ebreak(&mut self) -> Result<(), CKBVMError> {
        todo!()
    }
}
