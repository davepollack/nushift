const std = @import("std");

pub const Syscall = enum(usize) {
    exit = 0,
    shm_new = 1,
    shm_acquire = 2,
    shm_new_and_acquire = 3,
    shm_release = 4,
    shm_destroy = 5,
    shm_release_and_destroy = 6,
};

pub const SyscallError = enum(usize) {
    unknown_syscall = 0,

    shm_internal_error = 1,
    shm_exhausted = 2,
    shm_unknown_shm_type = 3,
    shm_invalid_length = 4,
    shm_capacity_not_available = 5,
    shm_cap_not_found = 6,
    shm_cap_currently_acquired = 7,
    shm_address_out_of_bounds = 8,
    shm_address_not_aligned = 9,
    shm_overlaps_existing_acquisition = 10,
};

pub const SyscallResult = union(enum) {
    ok: usize,
    fail: SyscallError,
};

pub fn SyscallArgs(comptime sys: Syscall) type {
    return switch (sys) {
        .exit => struct { exit_reason: usize },
        .shm_new => struct { shm_type: ShmType, length: usize },
        .shm_acquire => struct { shm_cap_id: usize, address: usize },
        .shm_new_and_acquire => struct { shm_type: ShmType, length: usize, address: usize },
        .shm_release, .shm_destroy, .shm_release_and_destroy => struct { shm_cap_id: usize },
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
        .exit => syscall_internal_args(@intFromEnum(sys), 1, [_]usize{sys_args.exit_reason}, ignore_errors, ReturnType),
        .shm_new => syscall_internal_args(@intFromEnum(sys), 2, [_]usize{ @intFromEnum(sys_args.shm_type), sys_args.length }, ignore_errors, ReturnType),
        .shm_acquire => syscall_internal_args(@intFromEnum(sys), 2, [_]usize{ sys_args.shm_cap_id, sys_args.address }, ignore_errors, ReturnType),
        .shm_new_and_acquire => syscall_internal_args(@intFromEnum(sys), 3, [_]usize{ @intFromEnum(sys_args.shm_type), sys_args.length, sys_args.address }, ignore_errors, ReturnType),
        .shm_release, .shm_destroy, .shm_release_and_destroy => syscall_internal_args(@intFromEnum(sys), 1, [_]usize{sys_args.shm_cap_id}, ignore_errors, ReturnType),
    };
}

fn syscall_internal_args(syscall_number: usize, comptime num_args: comptime_int, args: [num_args]usize, comptime ignore_errors: bool, comptime ReturnType: type) ReturnType {
    if (ignore_errors) {
        return switch (num_args) {
            0 => asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                : "memory"
            ),
            1 => asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                  [arg1] "{a1}" (args[0]),
                : "memory"
            ),
            2 => asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                  [arg1] "{a1}" (args[0]),
                  [arg2] "{a2}" (args[1]),
                : "memory"
            ),
            3 => asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                  [arg1] "{a1}" (args[0]),
                  [arg2] "{a2}" (args[1]),
                  [arg3] "{a3}" (args[2]),
                : "memory"
            ),
            else => @compileError("syscall_internal_args does not support " ++ std.fmt.comptimePrint("{}", .{num_args}) ++ " args, please add support if needed"),
        };
    }

    var a0_output: usize = undefined;
    var t0_output: usize = undefined;

    switch (num_args) {
        0 => asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
            : "memory"
        ),
        1 => asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (args[0]),
            : "memory"
        ),
        2 => asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (args[0]),
              [arg2] "{a2}" (args[1]),
            : "memory"
        ),
        3 => asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (args[0]),
              [arg2] "{a2}" (args[1]),
              [arg3] "{a3}" (args[2]),
            : "memory"
        ),
        else => @compileError("syscall_internal_args does not support " ++ std.fmt.comptimePrint("{}", .{num_args}) ++ " args, please add support if needed"),
    }

    if (a0_output == std.math.maxInt(usize)) {
        return SyscallResult{ .fail = @enumFromInt(t0_output) };
    }

    return SyscallResult{ .ok = a0_output };
}
