const std = @import("std");
const OsNushift = @import("os_nushift");
const ron = @embedFile("./accessibility_tree.ron");

const title: []const u8 = "Hello World App";

const TITLE_INPUT_ACQUIRE_ADDRESS: usize = 0x90000000;
const A11Y_INPUT_ACQUIRE_ADDRESS: usize = 0x90001000;

pub fn main() usize {
    return main_impl() catch |err| OsNushift.errorCodeFromSyscallError(err);
}

fn main_impl() OsNushift.SyscallError!usize {
    const title_cap_id = try OsNushift.syscall(.title_new, .{});
    defer _ = OsNushift.syscall_ignore_errors(.title_destroy, .{ .title_cap_id = title_cap_id });

    const title_input_shm_cap_id = try OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1, .address = TITLE_INPUT_ACQUIRE_ADDRESS });
    defer _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = title_input_shm_cap_id });

    // TODO: Return an `error.`
    write_str_to_input_cap(@as([*]u8, @ptrFromInt(TITLE_INPUT_ACQUIRE_ADDRESS))[0..4096], title) catch return 1;

    const title_task_id = try OsNushift.syscall(.title_publish, .{ .title_cap_id = title_cap_id, .input_shm_cap_id = title_input_shm_cap_id });
    _ = title_task_id;

    // TODO: Title cap and input cap should be destroyed after deferred task is
    // finished. The current defer statements mean this is indeed happening,
    // just not immediately after the deferred task is finished.

    const a11y_tree_cap_id = try OsNushift.syscall(.accessibility_tree_new, .{});
    defer _ = OsNushift.syscall_ignore_errors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = a11y_tree_cap_id });

    const a11y_input_shm_cap_id = try OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 10, .address = A11Y_INPUT_ACQUIRE_ADDRESS });
    defer _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = a11y_input_shm_cap_id });

    // TODO: Return an `error.`
    write_str_to_input_cap(@as([*]u8, @ptrFromInt(A11Y_INPUT_ACQUIRE_ADDRESS))[0..40960], ron) catch return 1;

    const a11y_task_id = try OsNushift.syscall(.accessibility_tree_publish, .{ .accessibility_tree_cap_id = a11y_tree_cap_id, .input_shm_cap_id = a11y_input_shm_cap_id });
    _ = a11y_task_id;

    // TODO: Accessibility cap and input cap should be destroyed after deferred
    // task is finished. The current defer statements mean this is indeed
    // happening, just not immediately after the deferred task is finished.

    return a11y_tree_cap_id + 1000;
}

fn write_str_to_input_cap(buffer: []u8, str: []const u8) std.io.FixedBufferStream([]u8).WriteError!void {
    var stream = std.io.fixedBufferStream(buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, str.len);
    _ = try writer.write(str);
}
