const std = @import("std");

pub const Syscall = enum(usize) {
    exit = 0,

    shm_new = 1,
    shm_acquire = 2,
    shm_new_and_acquire = 3,
    shm_release = 4,
    shm_destroy = 5,
    shm_release_and_destroy = 6,

    accessibility_tree_new = 7,
    accessibility_tree_publish = 8,
    accessibility_tree_destroy = 9,

    title_new = 10,
    title_publish = 11,
    title_destroy = 12,

    block_on_deferred_tasks = 13,

    gfx_new = 14,
    gfx_get_outputs = 15,
    gfx_cpu_present_buffer_new = 16,
    gfx_cpu_present = 17,
    gfx_cpu_present_buffer_destroy = 18,
    gfx_destroy = 19,
};

pub fn SyscallArgs(comptime sys: Syscall) type {
    return switch (sys) {
        .exit => struct { exit_reason: usize },

        .shm_new => struct { shm_type: ShmType, length: usize },
        .shm_acquire => struct { shm_cap_id: usize, address: usize },
        .shm_new_and_acquire => struct { shm_type: ShmType, length: usize, address: usize },
        .shm_release, .shm_destroy, .shm_release_and_destroy => struct { shm_cap_id: usize },

        .accessibility_tree_new => struct {},
        .accessibility_tree_publish => struct { accessibility_tree_cap_id: usize, input_shm_cap_id: usize, output_shm_cap_id: usize },
        .accessibility_tree_destroy => struct { accessibility_tree_cap_id: usize },

        .title_new => struct {},
        .title_publish => struct { title_cap_id: usize, input_shm_cap_id: usize, output_shm_cap_id: usize },
        .title_destroy => struct { title_cap_id: usize },

        .block_on_deferred_tasks => struct { input_shm_cap_id: usize },

        .gfx_new => struct {},
        .gfx_get_outputs => struct { gfx_cap_id: usize, output_shm_cap_id: usize },
        .gfx_cpu_present_buffer_new => struct { gfx_cap_id: usize, present_buffer_format: PresentBufferFormat, present_buffer_shm_cap_id: usize },
        .gfx_cpu_present => struct { gfx_cpu_present_buffer_cap_id: usize, wait_for_vblank: usize, output_shm_cap_id: usize },
        .gfx_cpu_present_buffer_destroy => struct { gfx_cpu_present_buffer_cap_id: usize },
        .gfx_destroy => struct { gfx_cap_id: usize },
    };
}

pub const SyscallErrorEnum = enum(usize) {
    unknown_syscall = 0,

    internal_error = 1,
    exhausted = 2,
    cap_not_found = 6,
    in_progress = 11,
    permission_denied = 12,

    shm_unknown_shm_type = 3,
    shm_invalid_length = 4,
    shm_capacity_not_available = 5,
    shm_cap_currently_acquired = 7,
    shm_address_out_of_bounds = 8,
    shm_address_not_aligned = 9,
    shm_overlaps_existing_acquisition = 10,

    deferred_deserialize_task_ids_error = 13,
    deferred_duplicate_task_ids = 14,
    deferred_task_id_not_found = 15,

    gfx_unknown_present_buffer_format = 16,
};

pub const SyscallError = error{
    UnknownSyscall,

    InternalError,
    Exhausted,
    CapNotFound,
    InProgress,
    PermissionDenied,

    ShmUnknownShmType,
    ShmInvalidLength,
    ShmCapacityNotAvailable,
    ShmCapCurrentlyAcquired,
    ShmAddressOutOfBounds,
    ShmAddressNotAligned,
    ShmOverlapsExistingAcquisition,

    DeferredDeserializeTaskIdsError,
    DeferredDuplicateTaskIds,
    DeferredTaskIdNotFound,

    GfxUnknownPresentBufferFormat,
};

pub const ShmType = enum(usize) {
    four_kib = 0,
    two_mib = 1,
    one_gib = 2,
};

pub const PresentBufferFormat = enum(usize) {
    r8g8b8_uint_srgb = 0,
};

pub fn syscall(comptime sys: Syscall, sys_args: SyscallArgs(sys)) SyscallError!usize {
    return syscall_internal(sys, sys_args, false);
}

pub fn syscall_ignore_errors(comptime sys: Syscall, sys_args: SyscallArgs(sys)) usize {
    return syscall_internal(sys, sys_args, true);
}

fn SyscallInternalReturnType(comptime ignore_errors: bool) type {
    return if (ignore_errors) usize else SyscallError!usize;
}

fn syscall_internal(comptime sys: Syscall, sys_args: SyscallArgs(sys), comptime ignore_errors: bool) SyscallInternalReturnType(ignore_errors) {
    return switch (sys) {
        .exit => syscall_internal_args(sys, .{sys_args.exit_reason}, ignore_errors),

        .shm_new => syscall_internal_args(sys, .{ @intFromEnum(sys_args.shm_type), sys_args.length }, ignore_errors),
        .shm_acquire => syscall_internal_args(sys, .{ sys_args.shm_cap_id, sys_args.address }, ignore_errors),
        .shm_new_and_acquire => syscall_internal_args(sys, .{ @intFromEnum(sys_args.shm_type), sys_args.length, sys_args.address }, ignore_errors),
        .shm_release, .shm_destroy, .shm_release_and_destroy => syscall_internal_args(sys, .{sys_args.shm_cap_id}, ignore_errors),

        // Send maxInt(usize) as the first argument. The first argument is not used yet, but may be in the future.
        .accessibility_tree_new => syscall_internal_args(sys, .{std.math.maxInt(usize)}, ignore_errors),
        .accessibility_tree_publish => syscall_internal_args(sys, .{ sys_args.accessibility_tree_cap_id, sys_args.input_shm_cap_id, sys_args.output_shm_cap_id }, ignore_errors),
        .accessibility_tree_destroy => syscall_internal_args(sys, .{sys_args.accessibility_tree_cap_id}, ignore_errors),

        .title_new => syscall_internal_args(sys, .{}, ignore_errors),
        .title_publish => syscall_internal_args(sys, .{ sys_args.title_cap_id, sys_args.input_shm_cap_id, sys_args.output_shm_cap_id }, ignore_errors),
        .title_destroy => syscall_internal_args(sys, .{sys_args.title_cap_id}, ignore_errors),

        .block_on_deferred_tasks => syscall_internal_args(sys, .{sys_args.input_shm_cap_id}, ignore_errors),

        .gfx_new => syscall_internal_args(sys, .{}, ignore_errors),
        .gfx_get_outputs => syscall_internal_args(sys, .{ sys_args.gfx_cap_id, sys_args.output_shm_cap_id }, ignore_errors),
        .gfx_cpu_present_buffer_new => syscall_internal_args(sys, .{ sys_args.gfx_cap_id, @intFromEnum(sys_args.present_buffer_format), sys_args.present_buffer_shm_cap_id }, ignore_errors),
        .gfx_cpu_present => syscall_internal_args(sys, .{ sys_args.gfx_cpu_present_buffer_cap_id, sys_args.wait_for_vblank, sys_args.output_shm_cap_id }, ignore_errors),
        .gfx_cpu_present_buffer_destroy => syscall_internal_args(sys, .{sys_args.gfx_cpu_present_buffer_cap_id}, ignore_errors),
        .gfx_destroy => syscall_internal_args(sys, .{sys_args.gfx_cap_id}, ignore_errors),
    };
}

fn syscall_internal_args(comptime sys: Syscall, args: anytype, comptime ignore_errors: bool) SyscallInternalReturnType(ignore_errors) {
    comptime std.debug.assert(@typeInfo(@TypeOf(args)) == .Struct);
    comptime std.debug.assert(@typeInfo(@TypeOf(args)).Struct.is_tuple);

    const syscall_number: usize = @intFromEnum(sys);

    if (ignore_errors) {
        // t0 is always clobbered by the hypervisor on an ecall. So it needs to
        // be included in the clobber list. Even in this ignore_errors case,
        // otherwise the register allocator is very happy to store things in t0
        // across this inline assembly which destroys it.
        return switch (args.len) {
            0 => asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                : "memory", "t0", "a0"
            ),
            1 => asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                  [arg1] "{a1}" (args[0]),
                : "memory", "t0", "a0", "a1"
            ),
            2 => asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                  [arg1] "{a1}" (args[0]),
                  [arg2] "{a2}" (args[1]),
                : "memory", "t0", "a0", "a1", "a2"
            ),
            3 => asm volatile ("ecall"
                : [ret] "={a0}" (-> usize),
                : [syscall_number] "{a0}" (syscall_number),
                  [arg1] "{a1}" (args[0]),
                  [arg2] "{a2}" (args[1]),
                  [arg3] "{a3}" (args[2]),
                : "memory", "t0", "a0", "a1", "a2", "a3"
            ),
            else => @compileError("syscall_internal_args does not support " ++ std.fmt.comptimePrint("{}", .{args.len}) ++ " args, please add support if needed"),
        };
    }

    var a0_output: usize = undefined;
    var t0_output: usize = undefined;

    switch (args.len) {
        0 => asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
            : "memory", "t0", "a0"
        ),
        1 => asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (args[0]),
            : "memory", "t0", "a0", "a1"
        ),
        2 => asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (args[0]),
              [arg2] "{a2}" (args[1]),
            : "memory", "t0", "a0", "a1", "a2"
        ),
        3 => asm volatile ("ecall"
            : [ret_a0] "={a0}" (a0_output),
              [ret_t0] "={t0}" (t0_output),
            : [syscall_number] "{a0}" (syscall_number),
              [arg1] "{a1}" (args[0]),
              [arg2] "{a2}" (args[1]),
              [arg3] "{a3}" (args[2]),
            : "memory", "t0", "a0", "a1", "a2", "a3"
        ),
        else => @compileError("syscall_internal_args does not support " ++ std.fmt.comptimePrint("{}", .{args.len}) ++ " args, please add support if needed"),
    }

    if (a0_output == std.math.maxInt(usize)) {
        return syscallErrorFromErrorCode(t0_output);
    }

    return a0_output;
}

fn syscallErrorFromErrorCode(error_code: usize) SyscallError {
    const syscall_error_enum: SyscallErrorEnum = @enumFromInt(error_code);

    return switch (syscall_error_enum) {
        inline else => |tag| @field(SyscallError, snakeToCamel(@tagName(tag))),
    };
}

pub fn errorCodeFromSyscallError(syscall_error: SyscallError) usize {
    @setEvalBranchQuota(10000);

    return switch (syscall_error) {
        inline else => |err| @intFromEnum(@field(SyscallErrorEnum, camelToSnake(@errorName(err)))),
    };
}

fn snakeToCamel(comptime snake: []const u8) []const u8 {
    var upper = true;
    var camel: [snake.len]u8 = undefined;
    var camelIndex: usize = 0;

    for (snake) |byte| {
        if (byte == '_') {
            upper = true;
            continue;
        }
        camel[camelIndex] = if (upper) std.ascii.toUpper(byte) else byte;
        upper = false;
        camelIndex += 1;
    }

    return camel[0..camelIndex];
}

fn camelToSnake(comptime camel: []const u8) []const u8 {
    var buffer: [2 * camel.len]u8 = undefined; // At most twice the size if every character is uppercase.
    var bufferIndex: usize = 0;

    for (camel, 0..) |byte, i| {
        if (std.ascii.isUpper(byte) and i > 0) {
            buffer[bufferIndex] = '_';
            bufferIndex += 1;
        }
        buffer[bufferIndex] = std.ascii.toLower(byte);
        bufferIndex += 1;
    }

    return buffer[0..bufferIndex];
}
