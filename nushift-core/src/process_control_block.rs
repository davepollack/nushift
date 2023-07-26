use std::marker::PhantomData;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::{Arc, Mutex};

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
use super::register_ipc::{SyscallEnter, SyscallReturn, SYSCALL_NUM_REGISTER, FIRST_ARG_REGISTER, SECOND_ARG_REGISTER, THIRD_ARG_REGISTER, RETURN_VAL_REGISTER, RETURN_VAL_REGISTER_INDEX, ERROR_RETURN_VAL_REGISTER, ERROR_RETURN_VAL_REGISTER_INDEX};

pub struct ProcessControlBlock<R> {
    machine: Machine<R>,
    exit_reason: ExitReason,
    syscall_enter: Option<Sender<SyscallEnter<R>>>,
    syscall_return: Option<Receiver<SyscallReturn<R>>>,
    locked_subsystem: Option<Arc<Mutex<NushiftSubsystem>>>,
}

enum Machine<R> {
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
            machine: Machine::Unloaded,
            exit_reason: ExitReason::NotExited,
            syscall_enter: None,
            syscall_return: None,
            locked_subsystem: None,
        }
    }

    pub fn set_syscall_enter(&mut self, syscall_enter: Sender<SyscallEnter<R>>) {
        self.syscall_enter = Some(syscall_enter);
    }

    pub fn set_syscall_return(&mut self, syscall_return: Receiver<SyscallReturn<R>>) {
        self.syscall_return = Some(syscall_return);
    }

    pub fn set_locked_subsystem(&mut self, locked_subsystem: Arc<Mutex<NushiftSubsystem>>) {
        self.locked_subsystem = Some(locked_subsystem);
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

        self.machine = Machine::Loaded { machine: core_machine, executable_machine };
        Ok(())
    }

    pub fn run(&mut self) -> Result<ExitReason, ProcessControlBlockError> {
        let run_result = self.run_internal();
        // let (_locked, cvar) = self.locked_self.as_ref().expect("Must be populated at this point").as_ref();
        // self.thread_state = ThreadState::Exited;
        // cvar.notify_one();
        run_result
    }

    fn run_internal(&mut self) -> Result<ExitReason, ProcessControlBlockError> {
        if !matches!(self.machine, Machine::Loaded { .. }) {
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
                    Machine::Loaded { ref mut executable_machine, .. } => executable_machine.memory_mut(),
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
            Machine::Loaded { ref mut machine, .. } => {
                self.exit_reason = ExitReason::UserExit { exit_reason };
                machine.set_running(false);
            },
            _ => {},
        }
    }

    fn set_running(&mut self) -> Result<(), ProcessControlBlockError> {
        match self.machine {
            Machine::Loaded { ref mut machine, .. } => {
                machine.set_running(true);
                Ok(())
            },
            _ => RunMachineNotLoadedSnafu.fail(),
        }
    }

    fn is_running(&self) -> Result<bool, ProcessControlBlockError> {
        match self.machine {
            Machine::Loaded { ref machine, .. } => Ok(machine.running()),
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
            Machine::Loaded { machine, .. } => machine.$name($($arg),*),
            _ => panic!("{}", PANIC_MESSAGE),
        }
    };
    (mut $self:ident, $name:ident$(, $arg:expr)*) => {
        match &mut $self.machine {
            Machine::Loaded{ machine, .. } => machine.$name($($arg),*),
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
        let send = [self.registers()[SYSCALL_NUM_REGISTER].clone(), self.registers()[FIRST_ARG_REGISTER].clone(), self.registers()[SECOND_ARG_REGISTER].clone(), self.registers()[THIRD_ARG_REGISTER].clone()];
        self.syscall_enter.as_ref().expect("Must be populated at this point").send(send).expect("Send should succeed");
        let recv = self.syscall_return.as_ref().expect("Must be populated at this point").recv().expect("Receive should succeed");
        self.set_register(RETURN_VAL_REGISTER, recv[RETURN_VAL_REGISTER_INDEX].clone());
        self.set_register(ERROR_RETURN_VAL_REGISTER, recv[ERROR_RETURN_VAL_REGISTER_INDEX].clone());
        // ecall should always return Ok (i.e. not terminate the app). If this
        // becomes not true in the future, change this!
        Ok(())
    }

    fn ebreak(&mut self) -> Result<(), CKBVMError> {
        // Terminate app.
        // TODO: As an improvement to terminating the app, provide debugging functionality.
        Err(CKBVMError::External(String::from("ebreak encountered; terminating app.")))
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
        let subsystem = self.locked_subsystem.as_ref().expect("Must be populated at this point").lock().unwrap();
        ProtectedMemory::load8(subsystem.shm_space(), R::to_u64(addr))
            .map(|value| R::from_u8(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn load16(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        let subsystem = self.locked_subsystem.as_ref().expect("Must be populated at this point").lock().unwrap();
        ProtectedMemory::load16(subsystem.shm_space(), R::to_u64(addr))
            .map(|value| R::from_u16(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn load32(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        let subsystem = self.locked_subsystem.as_ref().expect("Must be populated at this point").lock().unwrap();
        ProtectedMemory::load32(subsystem.shm_space(), R::to_u64(addr))
            .map(|value| R::from_u32(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn load64(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        let subsystem = self.locked_subsystem.as_ref().expect("Must be populated at this point").lock().unwrap();
        ProtectedMemory::load64(subsystem.shm_space(), R::to_u64(addr))
            .map(|value| R::from_u64(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn store8(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        let mut subsystem = self.locked_subsystem.as_ref().expect("Must be populated at this point").lock().unwrap();
        ProtectedMemory::store8(subsystem.shm_space_mut(), R::to_u64(addr), R::to_u8(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn store16(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        let mut subsystem = self.locked_subsystem.as_ref().expect("Must be populated at this point").lock().unwrap();
        ProtectedMemory::store16(subsystem.shm_space_mut(), R::to_u64(addr), R::to_u16(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn store32(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        let mut subsystem = self.locked_subsystem.as_ref().expect("Must be populated at this point").lock().unwrap();
        ProtectedMemory::store32(subsystem.shm_space_mut(), R::to_u64(addr), R::to_u32(value))
            .map_err(|_| CKBVMError::MemOutOfBound)
    }

    fn store64(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        let mut subsystem = self.locked_subsystem.as_ref().expect("Must be populated at this point").lock().unwrap();
        ProtectedMemory::store64(subsystem.shm_space_mut(), R::to_u64(addr), R::to_u64(value))
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
