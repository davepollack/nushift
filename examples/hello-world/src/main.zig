const std = @import("std");
const OsNushift = @import("os_nushift");
const ron = @embedFile("./accessibility_tree.ron");

const title: []const u8 = "Hello World App";

const TITLE_INPUT_ACQUIRE_ADDRESS: usize = 0x90000000;
const A11Y_INPUT_ACQUIRE_ADDRESS: usize = 0x90001000;
const BODT_INPUT_ACQUIRE_ADDRESS: usize = 0x9000b000;

const TaskDescriptor = struct {
    task_id: u64,
    input_shm_cap_acquire_addr: usize,
    output_shm_cap_acquire_addr: usize,
};

const FBSWriteError = std.io.FixedBufferStream([]u8).WriteError;

pub fn main() usize {
    return main_impl() catch |err| switch (err) {
        // When https://github.com/ziglang/zig/issues/2473 is complete, we can
        // do that instead of inline else.
        inline else => |any_err| if (std.meta.fieldIndex(OsNushift.SyscallError, @errorName(any_err))) |_| blk: {
            break :blk OsNushift.errorCodeFromSyscallError(@field(OsNushift.SyscallError, @errorName(any_err)));
        } else 1,
    };
}

fn main_impl() (FBSWriteError || OsNushift.SyscallError)!usize {
    const title_cap_id = try OsNushift.syscall(.title_new, .{});
    defer _ = OsNushift.syscall_ignore_errors(.title_destroy, .{ .title_cap_id = title_cap_id });

    const title_input_shm_cap_id = try OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1, .address = TITLE_INPUT_ACQUIRE_ADDRESS });
    defer _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = title_input_shm_cap_id });

    try write_str_to_input_cap(@as([*]u8, @ptrFromInt(TITLE_INPUT_ACQUIRE_ADDRESS))[0..4096], title);

    const title_output_shm_cap_id = try OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1 });
    defer _ = OsNushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = title_output_shm_cap_id });

    const title_task_id = try OsNushift.syscall(.title_publish, .{ .title_cap_id = title_cap_id, .input_shm_cap_id = title_input_shm_cap_id, .output_shm_cap_id = title_output_shm_cap_id });

    // TODO: Title cap, input cap and output cap should be destroyed after
    // deferred task is finished. The current defer statements mean this is
    // indeed happening, just not immediately after the deferred task is
    // finished.

    const a11y_tree_cap_id = try OsNushift.syscall(.accessibility_tree_new, .{});
    defer _ = OsNushift.syscall_ignore_errors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = a11y_tree_cap_id });

    const a11y_input_shm_cap_id = try OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 10, .address = A11Y_INPUT_ACQUIRE_ADDRESS });
    defer _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = a11y_input_shm_cap_id });

    try write_str_to_input_cap(@as([*]u8, @ptrFromInt(A11Y_INPUT_ACQUIRE_ADDRESS))[0..40960], ron);

    const a11y_output_shm_cap_id = try OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1 });
    defer _ = OsNushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = a11y_output_shm_cap_id });

    const a11y_task_id = try OsNushift.syscall(.accessibility_tree_publish, .{ .accessibility_tree_cap_id = a11y_tree_cap_id, .input_shm_cap_id = a11y_input_shm_cap_id, .output_shm_cap_id = a11y_output_shm_cap_id });

    const task_descriptors = [_]TaskDescriptor{
        TaskDescriptor{ .task_id = title_task_id, .input_shm_cap_acquire_addr = 0x1000, .output_shm_cap_acquire_addr = 0x2000 },
        TaskDescriptor{ .task_id = a11y_task_id, .input_shm_cap_acquire_addr = 0x3000, .output_shm_cap_acquire_addr = 0x4000 },
    };

    const block_on_deferred_tasks_input_cap_id = try OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1, .address = BODT_INPUT_ACQUIRE_ADDRESS });
    defer _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = block_on_deferred_tasks_input_cap_id });

    try write_task_descriptors_to_input_cap(@as([*]u8, @ptrFromInt(BODT_INPUT_ACQUIRE_ADDRESS))[0..4096], &task_descriptors);

    _ = try OsNushift.syscall(.block_on_deferred_tasks, .{ .input_shm_cap_id = block_on_deferred_tasks_input_cap_id });

    // TODO: Accessibility cap, input cap and output cap should be destroyed
    // after deferred task is finished. The current defer statements mean this
    // is indeed happening, just not immediately after the deferred task is
    // finished.

    return a11y_tree_cap_id + 1000;
}

fn write_str_to_input_cap(input_cap_buffer: []u8, str: []const u8) FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, str.len);
    _ = try writer.write(str);
}

fn write_task_descriptors_to_input_cap(input_cap_buffer: []u8, task_descriptors: []const TaskDescriptor) FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, task_descriptors.len);
    for (task_descriptors) |task_descriptor| {
        try std.leb.writeULEB128(writer, task_descriptor.task_id);
        try std.leb.writeULEB128(writer, task_descriptor.input_shm_cap_acquire_addr);
        try std.leb.writeULEB128(writer, task_descriptor.output_shm_cap_acquire_addr);
    }
}
