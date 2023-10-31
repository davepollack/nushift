const std = @import("std");
const os_nushift = @import("os_nushift");
const qoi = @import("qoi");

const gfx_output = @import("./gfx_output.zig");

const ron = @embedFile("./accessibility_tree.ron");
const hello_world_qoi_data = @embedFile("./hello_world.qoi");
const title: []const u8 = "Hello World App";

const TITLE_INPUT_ACQUIRE_ADDRESS: usize = 0x90000000;
const A11Y_INPUT_ACQUIRE_ADDRESS: usize = 0x90001000;
const BODT_INPUT_ACQUIRE_ADDRESS: usize = 0x9000b000;
const GGO_OUTPUT_ACQUIRE_ADDRESS: usize = 0x9000c000;
const PRESENT_BUFFER_ACQUIRE_ADDRESS: usize = 0x9000d000;

const FBSWriteError = std.io.FixedBufferStream([]u8).WriteError;

pub fn main() usize {
    return main_impl() catch |err| switch (err) {
        // When https://github.com/ziglang/zig/issues/2473 is complete, we can
        // do that instead of inline else.
        inline else => |any_err| if (std.meta.fieldIndex(os_nushift.SyscallError, @errorName(any_err))) |_| blk: {
            break :blk os_nushift.errorCodeFromSyscallError(@field(os_nushift.SyscallError, @errorName(any_err)));
        } else 1,
    };
}

fn main_impl() (FBSWriteError || os_nushift.SyscallError || gfx_output.Error || error{NotQOI})!usize {
    const tasks = blk: {
        const title_task = try TitleTask.init();
        errdefer title_task.deinit();

        const a11y_tree_task = try AccessibilityTreeTask.init();
        errdefer a11y_tree_task.deinit();

        const gfx_get_outputs_task = try GfxGetOutputsTask.init();
        errdefer gfx_get_outputs_task.deinit();

        break :blk .{ title_task, a11y_tree_task, gfx_get_outputs_task };
    };

    const title_task_id = try tasks[0].title_publish();
    const a11y_tree_task_id = try tasks[1].accessibility_tree_publish();
    const gfx_get_outputs_task_id = try tasks[2].gfx_get_outputs();

    // If an error occurs between publishing and the end of
    // block_on_deferred_tasks, you can't deinit the tasks because the resources
    // are in-flight. And we don't. But that does mean the task resources will
    // leak if that error occurs.

    try block_on_deferred_tasks(&.{ title_task_id, a11y_tree_task_id, gfx_get_outputs_task_id });

    tasks[1].deinit();
    tasks[0].deinit();

    // For some reason specifying .always_inline for this extracted logic is
    // needed, otherwise the binary size blows up by 70% :'(
    const output_0_width_px = try @call(.always_inline, get_output_0_width_px, .{&tasks[2]});

    // Present
    const gfx_cpu_present_task = try GfxCpuPresentTask.init(tasks[2].gfx_cap_id, output_0_width_px);
    const gfx_cpu_present_task_id = try gfx_cpu_present_task.gfx_cpu_present();
    try block_on_deferred_tasks(&.{gfx_cpu_present_task_id});
    gfx_cpu_present_task.deinit();

    tasks[2].deinit();

    return 0;
}

const TitleTask = struct {
    title_cap_id: usize,
    title_input_shm_cap_id: usize,
    title_output_shm_cap_id: usize,

    const Self = @This();

    fn init() (FBSWriteError || os_nushift.SyscallError)!Self {
        const title_cap_id = try os_nushift.syscall(.title_new, .{});
        errdefer _ = os_nushift.syscall_ignore_errors(.title_destroy, .{ .title_cap_id = title_cap_id });

        const title_input_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1, .address = TITLE_INPUT_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = title_input_shm_cap_id });

        try write_str_to_input_cap(@as([*]u8, @ptrFromInt(TITLE_INPUT_ACQUIRE_ADDRESS))[0..4096], title);

        const title_output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = title_output_shm_cap_id });

        return Self{
            .title_cap_id = title_cap_id,
            .title_input_shm_cap_id = title_input_shm_cap_id,
            .title_output_shm_cap_id = title_output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = os_nushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = self.title_output_shm_cap_id });
        _ = os_nushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = self.title_input_shm_cap_id });
        _ = os_nushift.syscall_ignore_errors(.title_destroy, .{ .title_cap_id = self.title_cap_id });
    }

    fn title_publish(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.title_publish, .{ .title_cap_id = self.title_cap_id, .input_shm_cap_id = self.title_input_shm_cap_id, .output_shm_cap_id = self.title_output_shm_cap_id });
    }
};

const AccessibilityTreeTask = struct {
    a11y_tree_cap_id: usize,
    a11y_input_shm_cap_id: usize,
    a11y_output_shm_cap_id: usize,

    const Self = @This();

    fn init() (FBSWriteError || os_nushift.SyscallError)!Self {
        const a11y_tree_cap_id = try os_nushift.syscall(.accessibility_tree_new, .{});
        errdefer _ = os_nushift.syscall_ignore_errors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = a11y_tree_cap_id });

        const a11y_input_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 10, .address = A11Y_INPUT_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = a11y_input_shm_cap_id });

        try write_str_to_input_cap(@as([*]u8, @ptrFromInt(A11Y_INPUT_ACQUIRE_ADDRESS))[0..40960], ron);

        const a11y_output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = a11y_output_shm_cap_id });

        return Self{
            .a11y_tree_cap_id = a11y_tree_cap_id,
            .a11y_input_shm_cap_id = a11y_input_shm_cap_id,
            .a11y_output_shm_cap_id = a11y_output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = os_nushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = self.a11y_output_shm_cap_id });
        _ = os_nushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = self.a11y_input_shm_cap_id });
        _ = os_nushift.syscall_ignore_errors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = self.a11y_tree_cap_id });
    }

    fn accessibility_tree_publish(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.accessibility_tree_publish, .{ .accessibility_tree_cap_id = self.a11y_tree_cap_id, .input_shm_cap_id = self.a11y_input_shm_cap_id, .output_shm_cap_id = self.a11y_output_shm_cap_id });
    }
};

const GfxGetOutputsTask = struct {
    gfx_cap_id: usize,
    output_shm_cap_id: usize,

    const Self = @This();

    fn init() os_nushift.SyscallError!Self {
        const gfx_cap_id = try os_nushift.syscall(.gfx_new, .{});
        errdefer _ = os_nushift.syscall_ignore_errors(.gfx_destroy, .{ .gfx_cap_id = gfx_cap_id });

        const output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = output_shm_cap_id });

        return Self{
            .gfx_cap_id = gfx_cap_id,
            .output_shm_cap_id = output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = os_nushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = self.output_shm_cap_id });
        _ = os_nushift.syscall_ignore_errors(.gfx_destroy, .{ .gfx_cap_id = self.gfx_cap_id });
    }

    fn gfx_get_outputs(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.gfx_get_outputs, .{ .gfx_cap_id = self.gfx_cap_id, .output_shm_cap_id = self.output_shm_cap_id });
    }
};

const GfxCpuPresentTask = struct {
    gfx_cap_id: usize,
    present_buffer_shm_cap_id: usize,
    gfx_cpu_present_buffer_cap_id: usize,
    output_shm_cap_id: usize,

    const Self = @This();

    fn init(gfx_cap_id: usize, output_width: u64) (os_nushift.SyscallError || FBSWriteError || error{NotQOI})!Self {
        // 1 MiB buffer
        const present_buffer_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 256, .address = PRESENT_BUFFER_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = present_buffer_shm_cap_id });

        try write_wrapped_image_to_input_cap(@as([*]u8, @ptrFromInt(PRESENT_BUFFER_ACQUIRE_ADDRESS))[0..1048576], hello_world_qoi_data, output_width);

        const gfx_cpu_present_buffer_cap_id = try os_nushift.syscall(.gfx_cpu_present_buffer_new, .{ .gfx_cap_id = gfx_cap_id, .present_buffer_format = os_nushift.PresentBufferFormat.r8g8b8_uint_srgb, .present_buffer_shm_cap_id = present_buffer_shm_cap_id });
        errdefer _ = os_nushift.syscall_ignore_errors(.gfx_cpu_present_buffer_destroy, .{ .gfx_cpu_present_buffer_cap_id = gfx_cpu_present_buffer_cap_id });

        const output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = output_shm_cap_id });

        return Self{
            .gfx_cap_id = gfx_cap_id,
            .present_buffer_shm_cap_id = present_buffer_shm_cap_id,
            .gfx_cpu_present_buffer_cap_id = gfx_cpu_present_buffer_cap_id,
            .output_shm_cap_id = output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = os_nushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = self.output_shm_cap_id });
        _ = os_nushift.syscall_ignore_errors(.gfx_cpu_present_buffer_destroy, .{ .gfx_cpu_present_buffer_cap_id = self.gfx_cpu_present_buffer_cap_id });
        _ = os_nushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = self.present_buffer_shm_cap_id });
    }

    fn gfx_cpu_present(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.gfx_cpu_present, .{ .gfx_cpu_present_buffer_cap_id = self.gfx_cpu_present_buffer_cap_id, .wait_for_vblank = std.math.maxInt(usize), .output_shm_cap_id = self.output_shm_cap_id });
    }
};

fn block_on_deferred_tasks(task_ids: []const u64) (FBSWriteError || os_nushift.SyscallError)!void {
    const block_on_deferred_tasks_input_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1, .address = BODT_INPUT_ACQUIRE_ADDRESS });
    defer _ = os_nushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = block_on_deferred_tasks_input_cap_id });

    try write_task_ids_to_input_cap(@as([*]u8, @ptrFromInt(BODT_INPUT_ACQUIRE_ADDRESS))[0..4096], task_ids);

    _ = try os_nushift.syscall(.block_on_deferred_tasks, .{ .input_shm_cap_id = block_on_deferred_tasks_input_cap_id });
}

fn get_output_0_width_px(gfx_get_outputs_task: *const GfxGetOutputsTask) (os_nushift.SyscallError || gfx_output.Error)!u64 {
    _ = try os_nushift.syscall(.shm_acquire, .{ .shm_cap_id = gfx_get_outputs_task.output_shm_cap_id, .address = GGO_OUTPUT_ACQUIRE_ADDRESS });

    const output_cap_buffer = @as([*]u8, @ptrFromInt(GGO_OUTPUT_ACQUIRE_ADDRESS))[0..4096];
    var stream = std.io.fixedBufferStream(output_cap_buffer);
    const reader = stream.reader();
    const outputs = try gfx_output.read_outputs(reader);

    _ = os_nushift.syscall_ignore_errors(.shm_release, .{ .shm_cap_id = gfx_get_outputs_task.output_shm_cap_id });

    return outputs[0].size_px[0];
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

fn write_wrapped_image_to_input_cap(input_cap_buffer: []u8, qoi_data: []const u8, output_width: u64) (FBSWriteError || error{NotQOI})!void {
    _ = output_width;
    _ = input_cap_buffer;
    const MARGIN_TOP: u32 = 100;
    _ = MARGIN_TOP;

    if (qoi_data.len < 8) {
        return error.NotQOI;
    }
    if (!std.mem.eql(u8, "qoif", qoi_data[0..4])) {
        return error.NotQOI;
    }

    // TODO
}
