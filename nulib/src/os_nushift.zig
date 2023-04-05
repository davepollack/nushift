const std = @import("std");

pub const Syscall = enum(usize) {
    exit = 0,
    shm_new = 1,
    shm_destroy = 5,
};

pub const ShmType = enum(usize) {
    four_kib = 0,
    two_mib = 1,
    four_mib = 2,
    one_gib = 3,
    five_twelve_gib = 4,
};

pub fn SyscallArgs(comptime sys: Syscall) type {
    return switch (sys) {
        .exit => struct { exit_reason: usize },
        .shm_new => struct { shm_type: ShmType },
        .shm_destroy => struct { shm_cap_id: usize },
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
        .shm_new => syscall1(@enumToInt(sys), @enumToInt(sys_args.shm_type)),
        .shm_destroy => syscall1(@enumToInt(sys), sys_args.shm_cap_id),
    };
}
