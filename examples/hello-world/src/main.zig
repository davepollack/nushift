const std = @import("std");
const OsNushift = @import("os_nushift");
const qoi = @import("qoi");

const ron = @embedFile("./accessibility_tree.ron");
const qoi_data = @embedFile("./hello-world.qoi");
const title: []const u8 = "Hello World App";

const TITLE_INPUT_ACQUIRE_ADDRESS: usize = 0x90000000;
const A11Y_INPUT_ACQUIRE_ADDRESS: usize = 0x90001000;
const BODT_INPUT_ACQUIRE_ADDRESS: usize = 0x9000b000;

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
    const tasks = blk: {
        const title_task = try TitleTask.init();
        errdefer title_task.deinit();

        const a11y_tree_task = try AccessibilityTreeTask.init();
        errdefer a11y_tree_task.deinit();

        break :blk .{ title_task, a11y_tree_task };
    };

    const title_task_id = try tasks[0].title_publish();
    const a11y_tree_task_id = try tasks[1].accessibility_tree_publish();

    // If an error occurs between publishing and the end of
    // block_on_deferred_tasks, you can't deinit the tasks because the resources
    // are in-flight. And we don't. But that does mean the task resources will
    // leak if that error occurs.

    try block_on_deferred_tasks(&.{ title_task_id, a11y_tree_task_id });

    tasks[1].deinit();
    tasks[0].deinit();

    return 1000;
}

const TitleTask = struct {
    title_cap_id: usize,
    title_input_shm_cap_id: usize,
    title_output_shm_cap_id: usize,

    const Self = @This();

    fn init() (FBSWriteError || OsNushift.SyscallError)!Self {
        const title_cap_id = try OsNushift.syscall(.title_new, .{});
        errdefer _ = OsNushift.syscall_ignore_errors(.title_destroy, .{ .title_cap_id = title_cap_id });

        const title_input_shm_cap_id = try OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1, .address = TITLE_INPUT_ACQUIRE_ADDRESS });
        errdefer _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = title_input_shm_cap_id });

        try write_str_to_input_cap(@as([*]u8, @ptrFromInt(TITLE_INPUT_ACQUIRE_ADDRESS))[0..4096], title);

        const title_output_shm_cap_id = try OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1 });
        errdefer _ = OsNushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = title_output_shm_cap_id });

        return Self{
            .title_cap_id = title_cap_id,
            .title_input_shm_cap_id = title_input_shm_cap_id,
            .title_output_shm_cap_id = title_output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = OsNushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = self.title_output_shm_cap_id });
        _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = self.title_input_shm_cap_id });
        _ = OsNushift.syscall_ignore_errors(.title_destroy, .{ .title_cap_id = self.title_cap_id });
    }

    fn title_publish(self: *const Self) OsNushift.SyscallError!usize {
        return OsNushift.syscall(.title_publish, .{ .title_cap_id = self.title_cap_id, .input_shm_cap_id = self.title_input_shm_cap_id, .output_shm_cap_id = self.title_output_shm_cap_id });
    }
};

const AccessibilityTreeTask = struct {
    a11y_tree_cap_id: usize,
    a11y_input_shm_cap_id: usize,
    a11y_output_shm_cap_id: usize,

    const Self = @This();

    fn init() (FBSWriteError || OsNushift.SyscallError)!Self {
        const a11y_tree_cap_id = try OsNushift.syscall(.accessibility_tree_new, .{});
        errdefer _ = OsNushift.syscall_ignore_errors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = a11y_tree_cap_id });

        const a11y_input_shm_cap_id = try OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 10, .address = A11Y_INPUT_ACQUIRE_ADDRESS });
        errdefer _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = a11y_input_shm_cap_id });

        try write_str_to_input_cap(@as([*]u8, @ptrFromInt(A11Y_INPUT_ACQUIRE_ADDRESS))[0..40960], ron);

        const a11y_output_shm_cap_id = try OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1 });
        errdefer _ = OsNushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = a11y_output_shm_cap_id });

        return Self{
            .a11y_tree_cap_id = a11y_tree_cap_id,
            .a11y_input_shm_cap_id = a11y_input_shm_cap_id,
            .a11y_output_shm_cap_id = a11y_output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = OsNushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = self.a11y_output_shm_cap_id });
        _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = self.a11y_input_shm_cap_id });
        _ = OsNushift.syscall_ignore_errors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = self.a11y_tree_cap_id });
    }

    fn accessibility_tree_publish(self: *const Self) OsNushift.SyscallError!usize {
        return OsNushift.syscall(.accessibility_tree_publish, .{ .accessibility_tree_cap_id = self.a11y_tree_cap_id, .input_shm_cap_id = self.a11y_input_shm_cap_id, .output_shm_cap_id = self.a11y_output_shm_cap_id });
    }
};

fn block_on_deferred_tasks(task_ids: []const u64) (FBSWriteError || OsNushift.SyscallError)!void {
    const block_on_deferred_tasks_input_cap_id = try OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1, .address = BODT_INPUT_ACQUIRE_ADDRESS });
    defer _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = block_on_deferred_tasks_input_cap_id });

    try write_task_ids_to_input_cap(@as([*]u8, @ptrFromInt(BODT_INPUT_ACQUIRE_ADDRESS))[0..4096], task_ids);

    _ = try OsNushift.syscall(.block_on_deferred_tasks, .{ .input_shm_cap_id = block_on_deferred_tasks_input_cap_id });
}

fn write_str_to_input_cap(input_cap_buffer: []u8, str: []const u8) FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, str.len);
    _ = try writer.write(str);
}

fn write_task_ids_to_input_cap(input_cap_buffer: []u8, task_ids: []const u64) FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, task_ids.len);
    for (task_ids) |task_id| {
        try std.leb.writeULEB128(writer, task_id);
    }
}

fn write_wrapped_image_to_input_cap(input_cap_buffer: []u8, decoder: qoi.Decoder, output_width: u64) FBSWriteError!void {
    _ = output_width;
    _ = decoder;
    _ = input_cap_buffer;
    const MARGIN_TOP: u32 = 100;
    _ = MARGIN_TOP;
}
