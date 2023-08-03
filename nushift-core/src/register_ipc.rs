pub struct SyscallEnter<R>([R; 4]);
impl<R> SyscallEnter<R> {
    pub fn new(syscall_num: R, first_arg: R, second_arg: R, third_arg: R) -> Self {
        Self([syscall_num, first_arg, second_arg, third_arg])
    }
}
impl<R> core::ops::Index<SyscallEnterIndex> for SyscallEnter<R> {
    type Output = R;
    fn index(&self, index: SyscallEnterIndex) -> &Self::Output {
        &self.0[index.0]
    }
}

pub enum SyscallReturn<R> {
    UserExit { exit_reason: u64 },
    Return(Return<R>),
}
impl<R> SyscallReturn<R> {
    pub fn new_return(return_val: R, error_return_val: R) -> Self {
        Self::Return(Return([return_val, error_return_val]))
    }
}
pub struct Return<R>([R; 2]);
impl<R> core::ops::Index<SyscallReturnIndex> for Return<R> {
    type Output = R;
    fn index(&self, index: SyscallReturnIndex) -> &Self::Output {
        &self.0[index.0]
    }
}

pub struct SyscallEnterIndex(usize);
pub const SYSCALL_NUM_REGISTER_INDEX: SyscallEnterIndex = SyscallEnterIndex(0);
pub const FIRST_ARG_REGISTER_INDEX: SyscallEnterIndex = SyscallEnterIndex(1);
pub const SECOND_ARG_REGISTER_INDEX: SyscallEnterIndex = SyscallEnterIndex(2);
pub const THIRD_ARG_REGISTER_INDEX: SyscallEnterIndex = SyscallEnterIndex(3);

pub struct SyscallReturnIndex(usize);
pub const RETURN_VAL_REGISTER_INDEX: SyscallReturnIndex = SyscallReturnIndex(0);
pub const ERROR_RETURN_VAL_REGISTER_INDEX: SyscallReturnIndex = SyscallReturnIndex(1);
