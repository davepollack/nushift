const std = @import("std");

pub const Syscall = enum(usize) {
    exit = 0,
    shm_new = 1,
    shm_destroy = 5,
};

pub const SyscallError = enum(usize) {
    unknown_syscall = 0,

    shm_duplicate_id = 1,
    shm_exhausted = 2,
    shm_unknown_shm_type = 3,
};

pub const SyscallResult = union(enum) {
    success: usize,
    @"error": SyscallError,
};

pub fn SyscallArgs(comptime sys: Syscall) type {
    return switch (sys) {
        .exit => struct { exit_reason: usize },
        .shm_new => struct { shm_type: ShmType },
        .shm_destroy => struct { shm_cap_id: usize },
    };
}

pub const ShmType = enum(usize) {
    four_kib = 0,
    two_mib = 1,
    four_mib = 2,
    one_gib = 3,
    five_twelve_gib = 4,
};

pub fn syscall(comptime sys: Syscall, sys_args: SyscallArgs(sys)) SyscallResult {
    return syscall_internal(sys, sys_args, false, SyscallResult);
}

pub fn syscall_ignore_errors(comptime sys: Syscall, sys_args: SyscallArgs(sys)) usize {
    return syscall_internal(sys, sys_args, true, usize);
}

fn syscall_internal(comptime sys: Syscall, sys_args: SyscallArgs(sys), comptime ignore_errors: bool, comptime ReturnType: type) ReturnType {
    return switch (sys) {
        .exit => syscall1(@enumToInt(sys), sys_args.exit_reason, ignore_errors, ReturnType),
        .shm_new => syscall1(@enumToInt(sys), @enumToInt(sys_args.shm_type), ignore_errors, ReturnType),
        .shm_destroy => syscall1(@enumToInt(sys), sys_args.shm_cap_id, ignore_errors, ReturnType),
    };
}

fn syscall1(syscall_number: usize, arg1: usize, comptime ignore_errors: bool, comptime ReturnType: type) ReturnType {
    if (ignore_errors) {
        return asm volatile ("ecall"
            : [ret] "={a0}" (-> usize),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (arg1),
            : "memory"
        );
    }

    var a0_output: usize = undefined;
    var t0_output: usize = undefined;

    asm volatile ("ecall"
        : [ret_a0] "={a0}" (a0_output),
          [ret_t0] "={t0}" (t0_output),
        : [syscall_number] "{a0}" (syscall_number),
          [arg1] "{a1}" (arg1),
        : "memory"
    );

    if (a0_output == std.math.maxInt(usize)) {
        return SyscallResult{ .@"error" = @intToEnum(SyscallError, t0_output) };
    }

    return SyscallResult{ .success = a0_output };
}
