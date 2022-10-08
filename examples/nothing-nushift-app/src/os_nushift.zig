const std = @import("std");

pub const Syscall = enum(usize) {
    exit = 0,
};

pub fn SyscallArgs(comptime sys: Syscall) type {
    return switch (sys) {
        .exit => struct { exit_reason: usize },
    };
}

fn syscall1(syscall_number: usize, arg1: usize) usize {
    return asm volatile ("ecall"
        : [ret] "={x10}" (-> usize),
        : [syscall_number] "{x10}" (syscall_number),
          [arg1] "{x11}" (arg1),
        : "memory"
    );
}

pub fn syscall(comptime sys: Syscall, sys_args: SyscallArgs(sys)) usize {
    return switch (sys) {
        .exit => syscall1(@enumToInt(sys), sys_args.exit_reason),
    };
}
