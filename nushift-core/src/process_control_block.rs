use std::marker::PhantomData;

use ckb_vm::{
    DefaultCoreMachine,
    SparseMemory,
    SupportMachine,
    CoreMachine,
    Machine as CKBVMMachine,
    Register,
    Error as CKBVMError,
    Bytes,
    decoder::build_decoder,
    instructions::execute,
    Memory,
};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use super::nushift_subsystem::NushiftSubsystem;
use super::protected_memory::ProtectedMemory;

pub struct ProcessControlBlock<R> {
    machine: MachineState<R>,
    exit_reason: ExitReason,
    pub(crate) subsystem: NushiftSubsystem,
}

enum MachineState<R> {
    Unloaded,
    Loaded {
        machine: DefaultCoreMachine<R, StubMemory<R>>,
        executable_machine: DefaultCoreMachine<R, SparseMemory<R>>,
    },
}

#[derive(Copy, Clone, Debug)]
pub enum ExitReason {
    NotExited,
    UserExit { exit_reason: u64 },
}

impl<R: Register> ProcessControlBlock<R> {
    pub fn new() -> Self {
        Self {
            machine: MachineState::Unloaded,
            exit_reason: ExitReason::NotExited,
            subsystem: NushiftSubsystem::new(),
        }
    }

    pub fn load_machine(&mut self, image: Vec<u8>) -> Result<(), ProcessControlBlockError> {
        let mut core_machine = DefaultCoreMachine::<R, StubMemory<R>>::new(
            ckb_vm::ISA_IMC,
            ckb_vm::machine::VERSION1,
            u64::MAX,
        );
        let mut executable_machine = DefaultCoreMachine::<R, SparseMemory<R>>::new(
            ckb_vm::ISA_IMC,
            ckb_vm::machine::VERSION1,
            u64::MAX,
        );

        executable_machine.load_elf(&Bytes::from(image), true).context(ElfLoadingSnafu)?;
        core_machine.update_pc(executable_machine.pc().clone());
        core_machine.commit_pc();

        self.machine = MachineState::Loaded { machine: core_machine, executable_machine };
        Ok(())
    }

    pub fn run(&mut self) -> Result<ExitReason, ProcessControlBlockError> {
        if !matches!(self.machine, MachineState::Loaded { .. }) {
            return RunMachineNotLoadedSnafu.fail();
        }

        // TODO: The decoder is based on PC being in the first 4 MiB, which is an issue.
        let mut decoder = build_decoder::<R>(self.isa(), self.version());

        self.set_running()?;
        while self.is_running()? {
            // We don't have `if self.reset_signal()` here because we're not supporting reset right now
            let instruction = {
                let pc = self.pc().to_u64();
                let memory = match self.machine {
                    MachineState::Loaded { ref mut executable_machine, .. } => executable_machine.memory_mut(),
                    _ => panic!("must be loaded since it was checked before"),
                };
                decoder.decode(memory, pc).context(DecodeSnafu)?
            };
            execute(instruction, self).context(ExecuteSnafu)?;
        }

        Ok(self.exit_reason)
    }

    pub fn user_exit(&mut self, exit_reason: u64) {
        match self.machine {
            MachineState::Loaded { ref mut machine, .. } => {
                self.exit_reason = ExitReason::UserExit { exit_reason };
                machine.set_running(false);
            },
            _ => {},
        }
    }

    fn set_running(&mut self) -> Result<(), ProcessControlBlockError> {
        match self.machine {
            MachineState::Loaded { ref mut machine, .. } => {
                machine.set_running(true);
                Ok(())
            },
            _ => RunMachineNotLoadedSnafu.fail(),
        }
    }

    fn is_running(&self) -> Result<bool, ProcessControlBlockError> {
        match self.machine {
            MachineState::Loaded { ref machine, .. } => Ok(machine.running()),
            _ => RunMachineNotLoadedSnafu.fail(),
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
            MachineState::Loaded { machine, .. } => machine.$name($($arg),*),
            _ => panic!("{}", PANIC_MESSAGE),
        }
    };
    (mut $self:ident, $name:ident$(, $arg:expr)*) => {
        match &mut $self.machine {
            MachineState::Loaded{ machine, .. } => machine.$name($($arg),*),
            _ => panic!("{}", PANIC_MESSAGE),
        }
    };
}

impl<R: Register> CoreMachine for ProcessControlBlock<R> {
    type REG = R;
    type MEM = Self;

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
        self
    }

    fn memory_mut(&mut self) -> &mut Self::MEM {
        self
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
        NushiftSubsystem::ecall(self)
    }

    fn ebreak(&mut self) -> Result<(), CKBVMError> {
        NushiftSubsystem::ebreak(self)
    }
}

impl<R: Register> Memory for ProcessControlBlock<R> {
    type REG = R;

    fn init_pages(
        &mut self,
        _addr: u64,
        _size: u64,
        _flags: u8,
        _source: Option<Bytes>,
        _offset_from_addr: u64,
    ) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn fetch_flag(&mut self, _page: u64) -> Result<u8, CKBVMError> {
        unimplemented!()
    }

    fn set_flag(&mut self, _page: u64,_flag: u8) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn clear_flag(&mut self, _page: u64, _flag: u8) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn store_byte(&mut self, _addr: u64, _size: u64, _value: u8) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn store_bytes(&mut self, _addr: u64, _value: &[u8]) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn execute_load16(&mut self, _addr: u64) -> Result<u16, CKBVMError> {
        unimplemented!()
    }

    fn execute_load32(&mut self, _addr: u64) -> Result<u32, CKBVMError> {
        unimplemented!()
    }

    fn load8(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        ProtectedMemory::load8(self.subsystem.shm_space(), R::to_u64(addr))
            .map(|value| R::from_u8(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn load16(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        ProtectedMemory::load16(self.subsystem.shm_space(), R::to_u64(addr))
            .map(|value| R::from_u16(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn load32(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        ProtectedMemory::load32(self.subsystem.shm_space(), R::to_u64(addr))
            .map(|value| R::from_u32(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn load64(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        ProtectedMemory::load64(self.subsystem.shm_space(), R::to_u64(addr))
            .map(|value| R::from_u64(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn store8(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        ProtectedMemory::store8(self.subsystem.shm_space_mut(), R::to_u64(addr), R::to_u8(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn store16(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        ProtectedMemory::store16(self.subsystem.shm_space_mut(), R::to_u64(addr), R::to_u16(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn store32(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        ProtectedMemory::store32(self.subsystem.shm_space_mut(), R::to_u64(addr), R::to_u32(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn store64(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        ProtectedMemory::store64(self.subsystem.shm_space_mut(), R::to_u64(addr), R::to_u64(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }
}

#[derive(Default)]
struct StubMemory<R>(PhantomData<R>);

impl<R: Register> Memory for StubMemory<R> {
    type REG = R;

    fn init_pages(
        &mut self,
        _addr: u64,
        _size: u64,
        _flags: u8,
        _source: Option<Bytes>,
        _offset_from_addr: u64,
    ) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn fetch_flag(&mut self, _page: u64) -> Result<u8, CKBVMError> {
        unimplemented!()
    }

    fn set_flag(&mut self, _page: u64, _flag: u8) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn clear_flag(&mut self, _page: u64, _flag: u8) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn store_byte(&mut self, _addr: u64, _size: u64, _value: u8) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn store_bytes(&mut self, _addr: u64, _value: &[u8]) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn execute_load16(&mut self, _addr: u64) -> Result<u16, CKBVMError> {
        unimplemented!()
    }

    fn execute_load32(&mut self, _addr: u64) -> Result<u32, CKBVMError> {
        unimplemented!()
    }

    fn load8(&mut self, _addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        unimplemented!()
    }

    fn load16(&mut self, _addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        unimplemented!()
    }

    fn load32(&mut self, _addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        unimplemented!()
    }

    fn load64(&mut self, _addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        unimplemented!()
    }

    fn store8(&mut self, _addr: &Self::REG, _value: &Self::REG) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn store16(&mut self, _addr: &Self::REG, _value: &Self::REG) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn store32(&mut self, _addr: &Self::REG, _value: &Self::REG) -> Result<(), CKBVMError> {
        unimplemented!()
    }

    fn store64(&mut self, _addr: &Self::REG, _value: &Self::REG) -> Result<(), CKBVMError> {
        unimplemented!()
    }
}
