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
        .shm_new => struct { type: ShmType, length: usize },
        .shm_destroy => struct { shm_cap_id: usize },
    };
}

pub const ShmType = enum(usize) {
    four_kib = 0,
    two_mib = 1,
    one_gib = 2,
};

pub fn syscall(comptime sys: Syscall, sys_args: SyscallArgs(sys)) SyscallResult {
    return syscall_internal(sys, sys_args, false, SyscallResult);
}

pub fn syscall_ignore_errors(comptime sys: Syscall, sys_args: SyscallArgs(sys)) usize {
    return syscall_internal(sys, sys_args, true, usize);
}

fn syscall_internal(comptime sys: Syscall, sys_args: SyscallArgs(sys), comptime ignore_errors: bool, comptime ReturnType: type) ReturnType {
    return switch (sys) {
        .exit => syscall_internal_args(@enumToInt(sys), 1, [_]usize{sys_args.exit_reason}, ignore_errors, ReturnType),
        .shm_new => syscall_internal_args(@enumToInt(sys), 2, [_]usize{ @enumToInt(sys_args.type), sys_args.length }, ignore_errors, ReturnType),
        .shm_destroy => syscall_internal_args(@enumToInt(sys), 1, [_]usize{sys_args.shm_cap_id}, ignore_errors, ReturnType),
    };
}

fn syscall_internal_args(syscall_number: usize, comptime num_args: comptime_int, args: [num_args]usize, comptime ignore_errors: bool, comptime ReturnType: type) ReturnType {
    if (ignore_errors) {
        if (num_args >= 2) {
            return asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                  [arg1] "{a1}" (args[0]),
                  [arg2] "{a2}" (args[1]),
                : "memory"
            );
        }
        if (num_args == 1) {
            return asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                  [arg1] "{a1}" (args[0]),
                : "memory"
            );
        }
        return asm volatile ("ecall"
            : [ret] "={a0}" (-> usize),
            : [syscall_number] "{a0}" (syscall_number),
            : "memory"
        );
    }

    var a0_output: usize = undefined;
    var t0_output: usize = undefined;

    if (num_args >= 2) {
        asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (args[0]),
              [arg2] "{a2}" (args[1]),
            : "memory"
        );
    } else if (num_args == 1) {
        asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (args[0]),
            : "memory"
        );
    } else {
        asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
            : "memory"
        );
    }

    if (a0_output == std.math.maxInt(usize)) {
        return SyscallResult{ .@"error" = @intToEnum(SyscallError, t0_output) };
    }

    return SyscallResult{ .success = a0_output };
}
