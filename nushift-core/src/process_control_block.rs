use core::fmt::LowerHex;
use core::marker::PhantomData;
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
    registers::{A0, A1, A2, A3, T0},
};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use super::nushift_subsystem::NushiftSubsystem;
use super::protected_memory::{ProtectedMemory, ProtectedMemoryError};
use super::register_ipc::{SyscallEnter, SyscallReturn, RETURN_VAL_REGISTER_INDEX, ERROR_RETURN_VAL_REGISTER_INDEX};
use super::shm_space::ShmSpace;

const SYSCALL_NUM_REGISTER: usize = A0;
const FIRST_ARG_REGISTER: usize = A1;
const SECOND_ARG_REGISTER: usize = A2;
const THIRD_ARG_REGISTER: usize = A3;

const RETURN_VAL_REGISTER: usize = A0;
/// a1 is used by the RISC-V calling conventions for a second return value,
/// rather than t0, but my concern is with the whole 32-bit app thing using
/// multiple registers to encode a 64-bit value. Maybe the 32-bit ABI will just
/// use a0 and a2 and the 64-bit will use a0 and a1. For now, using t0.
const ERROR_RETURN_VAL_REGISTER: usize = T0;

pub struct ProcessControlBlock<R> {
    machine: Machine<R>,
    exit_reason: ExitReason,
    syscall_enter: Sender<SyscallEnter<R>>,
    syscall_return: Receiver<SyscallReturn<R>>,
    locked_subsystem: Arc<Mutex<NushiftSubsystem>>,
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

impl<R> ProcessControlBlock<R>
where
    R: Register + LowerHex,
{
    pub fn new(syscall_enter: Sender<SyscallEnter<R>>, syscall_return: Receiver<SyscallReturn<R>>, locked_subsystem: Arc<Mutex<NushiftSubsystem>>) -> Self {
        Self {
            machine: Machine::Unloaded,
            exit_reason: ExitReason::NotExited,
            syscall_enter,
            syscall_return,
            locked_subsystem,
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

        self.machine = Machine::Loaded { machine: core_machine, executable_machine };
        Ok(())
    }

    pub fn run(&mut self) -> Result<ExitReason, ProcessControlBlockError> {
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

macro_rules! proxy_to_self_machine {
    ($self:ident, $name:ident$(, $arg:expr)*) => {
        proxy_to_self_machine_impl!(; $self, $name$(, $arg)*)
    };
    (mut $self:ident, $name:ident$(, $arg:expr)*) => {
        proxy_to_self_machine_impl!(mut; $self, $name$(, $arg)*)
    };
}
macro_rules! proxy_to_self_machine_impl {
    ($($mut:ident)?; $self:ident, $name:ident$(, $arg:expr)*) => {
        match $self.machine {
            Machine::Loaded { ref $($mut)? machine, .. } => machine.$name($($arg),*),
            _ => panic!("process_control_block.rs: Machine attempted to be used but not loaded"),
        }
    };
}

impl<R> CoreMachine for ProcessControlBlock<R>
where
    R: Register + LowerHex,
{
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

impl<R> CKBVMMachine for ProcessControlBlock<R>
where
    R: Register + LowerHex,
{
    fn ecall(&mut self) -> Result<(), CKBVMError> {
        let send = SyscallEnter::new(self.registers()[SYSCALL_NUM_REGISTER].clone(), self.registers()[FIRST_ARG_REGISTER].clone(), self.registers()[SECOND_ARG_REGISTER].clone(), self.registers()[THIRD_ARG_REGISTER].clone());
        self.syscall_enter.send(send).expect("Send should succeed");
        let recv = self.syscall_return.recv().expect("Receive should succeed");
        match recv {
            SyscallReturn::UserExit { exit_reason } => self.user_exit(exit_reason),
            SyscallReturn::Return(recv) => {
                self.set_register(RETURN_VAL_REGISTER, recv[RETURN_VAL_REGISTER_INDEX].clone());
                self.set_register(ERROR_RETURN_VAL_REGISTER, recv[ERROR_RETURN_VAL_REGISTER_INDEX].clone());
            },
        }
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

impl<R> Memory for ProcessControlBlock<R>
where
    R: Register + LowerHex,
{
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
        load_impl(self, addr, ProtectedMemory::load8, R::from_u8)
    }

    fn load16(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        load_impl(self, addr, ProtectedMemory::load16, R::from_u16)
    }

    fn load32(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        load_impl(self, addr, ProtectedMemory::load32, R::from_u32)
    }

    fn load64(&mut self, addr: &Self::REG) -> Result<Self::REG, CKBVMError> {
        load_impl(self, addr, ProtectedMemory::load64, R::from_u64)
    }

    fn store8(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        store_impl(self, addr, value, ProtectedMemory::store8, R::to_u8)
    }

    fn store16(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        store_impl(self, addr, value, ProtectedMemory::store16, R::to_u16)
    }

    fn store32(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        store_impl(self, addr, value, ProtectedMemory::store32, R::to_u32)
    }

    fn store64(&mut self, addr: &Self::REG, value: &Self::REG) -> Result<(), CKBVMError> {
        store_impl(self, addr, value, ProtectedMemory::store64, R::to_u64)
    }
}

fn load_impl<T, L, F, R>(pcb: &ProcessControlBlock<R>, addr: &R, protected_memory_load_func: L, from_val_func: F) -> Result<R, CKBVMError>
where
    L: FnOnce(&ShmSpace, u64) -> Result<T, ProtectedMemoryError>,
    F: FnOnce(T) -> R,
    R: Register + LowerHex,
{
    let subsystem = pcb.locked_subsystem.lock().unwrap();
    protected_memory_load_func(subsystem.shm_space(), R::to_u64(addr))
        .map(|value| from_val_func(value))
        .map_err(|_| {
            log::error!("Out of bounds load: addr {addr:#x}, PC {:#x}", pcb.pc());
            CKBVMError::MemOutOfBound
        })
}

fn store_impl<T, S, F, R>(pcb: &ProcessControlBlock<R>, addr: &R, value: &R, protected_memory_store_func: S, to_val_func: F) -> Result<(), CKBVMError>
where
    S: FnOnce(&mut ShmSpace, u64, T) -> Result<(), ProtectedMemoryError>,
    F: FnOnce(&R) -> T,
    R: Register + LowerHex,
{
    let mut subsystem = pcb.locked_subsystem.lock().unwrap();
    protected_memory_store_func(subsystem.shm_space_mut(), R::to_u64(addr), to_val_func(value))
        .map_err(|_| {
            log::error!("Out of bounds store: addr {addr:#x}, PC {:#x}", pcb.pc());
            CKBVMError::MemOutOfBound
        })
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
