use ckb_vm::registers::{A0, A1, A2, A3, T0};

pub const SYSCALL_NUM_REGISTER: usize = A0;
pub const FIRST_ARG_REGISTER: usize = A1;
pub const SECOND_ARG_REGISTER: usize = A2;
pub const THIRD_ARG_REGISTER: usize = A3;

pub const RETURN_VAL_REGISTER: usize = A0;
/// a1 is used by the RISC-V calling conventions for a second return value,
/// rather than t0, but my concern is with the whole 32-bit app thing using
/// multiple registers to encode a 64-bit value. Maybe the 32-bit ABI will just
/// use a0 and a2 and the 64-bit will use a0 and a1. For now, using t0.
pub const ERROR_RETURN_VAL_REGISTER: usize = T0;

pub const SYSCALL_NUM_REGISTER_INDEX: usize = 0;
pub const FIRST_ARG_REGISTER_INDEX: usize = 1;
pub const SECOND_ARG_REGISTER_INDEX: usize = 2;
pub const THIRD_ARG_REGISTER_INDEX: usize = 3;

pub type SyscallEnter<R> = [R; 4];
pub enum SyscallReturn<R> {
    UserExit { exit_reason: u64 },
    /// Return val index 0, error return val index 1
    Return([R; 2]),
}
