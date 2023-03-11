use ckb_vm::{
    DefaultCoreMachine,
    SparseMemory,
    SupportMachine,
    CoreMachine,
    Register,
    Machine as CKBVMMachine,
    Error as CKBVMError,
    Bytes,
    machine::VERSION1,
    registers::SP,
    decoder::build_decoder,
    instructions::execute,
};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

pub struct ProcessControlBlock<R = u64> {
    machine: MachineState<R>,
    exit_reason: ExitReason,
}

enum MachineState<R> {
    Unloaded,
    Loaded(DefaultCoreMachine<R, SparseMemory<R>>),
    Stopped,
}

#[derive(Copy, Clone)]
pub enum ExitReason {
    NotExited,
    UserExit { exit_reason: u64 },
}

impl ProcessControlBlock {
    pub fn new() -> Self {
        Self { machine: MachineState::Unloaded, exit_reason: ExitReason::NotExited }
    }

    pub fn load_machine(&mut self, image: Vec<u8>) -> Result<(), ProcessControlBlockError> {
        let core_machine = DefaultCoreMachine::<u64, SparseMemory<u64>>::new(
            ckb_vm::ISA_IMC,
            ckb_vm::machine::VERSION1,
            u64::MAX,
        );

        // We have to set it as loaded here for the below support methods to
        // work. If we remove the SupportMachine implementation and use our own
        // methods that can take a CoreMachine directly, we may not have to set
        // it as loaded on this line.
        self.machine = MachineState::Loaded(core_machine);

        // Use self.load_elf() and self.initialize_stack(), this is why we
        // implemented SupportMachine, so we don't have to convert the loader
        // code in riscv_machine_wrapper.rs, otherwise, remove the
        // SupportMachine implementation.

        self.load_elf(&Bytes::from(image), true).context(ElfLoadingSnafu)?;

        // The stack. 256 KiB.
        //
        // Should the location and size be determined by app metadata? Should
        // the location be randomised?
        const STACK_BASE: u64 = 0x80000000;
        const STACK_SIZE: u64 = 0x40000;
        self.initialize_stack(&[], STACK_BASE, STACK_SIZE).context(StackInitializationSnafu)?;
        // Make sure SP is 16 byte aligned
        if self.version() >= VERSION1 {
            debug_assert!(self.registers()[SP].to_u64() % 16 == 0);
        }

        Ok(())
    }

    pub fn run(&mut self) -> Result<ExitReason, ProcessControlBlockError> {
        if !matches!(self.machine, MachineState::Loaded(_)) {
            return RunMachineNotLoadedSnafu.fail();
        }

        // TODO: The decoder is based on PC being in the first 4 MiB, which is an issue.
        let mut decoder = build_decoder::<u64>(self.isa(), self.version());

        self.set_running()?;
        while self.is_running()? {
            // We don't have `if self.reset_signal()` here because we're not supporting reset right now
            let instruction = {
                let pc = self.pc().to_u64();
                let memory = self.memory_mut();
                decoder.decode(memory, pc).context(DecodeSnafu)?
            };
            execute(instruction, self).context(ExecuteSnafu)?;
        }

        Ok(self.exit_reason)
    }

    pub fn user_exit(&mut self, exit_reason: u64) {
        if let MachineState::Loaded(machine) = &mut self.machine {
            self.exit_reason = ExitReason::UserExit { exit_reason };
            machine.set_running(false);
        }
    }

    fn set_running(&mut self) -> Result<(), ProcessControlBlockError> {
        match &mut self.machine {
            MachineState::Loaded(machine) => {
                machine.set_running(true);
                Ok(())
            },
            _ => RunMachineNotLoadedSnafu.fail(),
        }
    }

    fn is_running(&self) -> Result<bool, ProcessControlBlockError> {
        match &self.machine {
            MachineState::Loaded(machine) => Ok(machine.running()),
            _ => RunMachineNotLoadedSnafu.fail()
        }
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum ProcessControlBlockError {
    ElfLoadingError { source: CKBVMError },
    StackInitializationError { source: CKBVMError },
    #[snafu(display("Attempted to run a machine that is not loaded"))]
    RunMachineNotLoaded,
    DecodeError { source: CKBVMError },
    ExecuteError { source: CKBVMError },
}

const PANIC_MESSAGE: &str = "process_control_block.rs: Machine attempted to be used but not loaded";
macro_rules! proxy_to_self_machine {
    ($self:ident, $name:ident$(, $arg:expr)*) => {
        match &$self.machine {
            MachineState::Loaded(core_machine) => core_machine.$name($($arg),*),
            _ => panic!("{}", PANIC_MESSAGE),
        }
    };
    (mut $self:ident, $name:ident$(, $arg:expr)*) => {
        match &mut $self.machine {
            MachineState::Loaded(core_machine) => core_machine.$name($($arg),*),
            _ => panic!("{}", PANIC_MESSAGE),
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

impl<R: Register> SupportMachine for ProcessControlBlock<R> {
    fn cycles(&self) -> u64 {
        proxy_to_self_machine!(self, cycles)
    }

    fn set_cycles(&mut self, cycles: u64) {
        proxy_to_self_machine!(mut self, set_cycles, cycles)
    }

    fn max_cycles(&self) -> u64 {
        proxy_to_self_machine!(self, max_cycles)
    }

    fn running(&self) -> bool {
        proxy_to_self_machine!(self, running)
    }

    fn set_running(&mut self, running: bool) {
        proxy_to_self_machine!(mut self, set_running, running)
    }

    fn reset(&mut self, max_cycles: u64) {
        proxy_to_self_machine!(mut self, reset, max_cycles)
    }

    fn reset_signal(&mut self) -> bool {
        proxy_to_self_machine!(mut self, reset_signal)
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
