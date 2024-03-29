// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use core::ops::Index;

// Instead of using the structs in this file for mpsc IPC, I would like to go
// back to using a condition variable. However with the current structure of the
// ckb-vm library, it might not be possible to do that.

pub struct SyscallEnter<R>([R; 5]);
impl<R> SyscallEnter<R> {
    pub fn new(syscall_num: R, first_arg: R, second_arg: R, third_arg: R, fourth_arg: R) -> Self {
        Self([syscall_num, first_arg, second_arg, third_arg, fourth_arg])
    }
}
impl<R> Index<SyscallEnterIndex> for SyscallEnter<R> {
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
impl<R> Index<SyscallReturnIndex> for Return<R> {
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
pub const FOURTH_ARG_REGISTER_INDEX: SyscallEnterIndex = SyscallEnterIndex(4);

pub struct SyscallReturnIndex(usize);
pub const RETURN_VAL_REGISTER_INDEX: SyscallReturnIndex = SyscallReturnIndex(0);
pub const ERROR_RETURN_VAL_REGISTER_INDEX: SyscallReturnIndex = SyscallReturnIndex(1);
