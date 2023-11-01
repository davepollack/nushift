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
    return mainImpl() catch |err| switch (err) {
        // When https://github.com/ziglang/zig/issues/2473 is complete, we can
        // do that instead of inline else.
        inline else => |any_err| if (std.meta.fieldIndex(os_nushift.SyscallError, @errorName(any_err))) |_| blk: {
            break :blk os_nushift.errorCodeFromSyscallError(@field(os_nushift.SyscallError, @errorName(any_err)));
        } else 1,
    };
}

fn mainImpl() (FBSWriteError || os_nushift.SyscallError || gfx_output.Error || error{ NotQOI, EndOfStream })!usize {
    const tasks = blk: {
        const title_task = try TitleTask.init();
        errdefer title_task.deinit();

        const a11y_tree_task = try AccessibilityTreeTask.init();
        errdefer a11y_tree_task.deinit();

        const gfx_get_outputs_task = try GfxGetOutputsTask.init();
        errdefer gfx_get_outputs_task.deinit();

        break :blk .{ title_task, a11y_tree_task, gfx_get_outputs_task };
    };

    const title_task_id = try tasks[0].titlePublish();
    const a11y_tree_task_id = try tasks[1].accessibilityTreePublish();
    const gfx_get_outputs_task_id = try tasks[2].gfxGetOutputs();

    // If an error occurs between publishing and the end of
    // block_on_deferred_tasks, you can't deinit the tasks because the resources
    // are in-flight. And we don't. But that does mean the task resources will
    // leak if that error occurs.

    try blockOnDeferredTasks(&.{ title_task_id, a11y_tree_task_id, gfx_get_outputs_task_id });

    tasks[1].deinit();
    tasks[0].deinit();

    // For some reason specifying .always_inline for this extracted logic is
    // needed, otherwise the binary size blows up by 70% :'(
    const output_0_width_px = try @call(.always_inline, getOutput0WidthPx, .{&tasks[2]});

    // Present
    const gfx_cpu_present_task = try GfxCpuPresentTask.init(tasks[2].gfx_cap_id, output_0_width_px);
    const gfx_cpu_present_task_id = try gfx_cpu_present_task.gfxCpuPresent();
    try blockOnDeferredTasks(&.{gfx_cpu_present_task_id});
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
        errdefer _ = os_nushift.syscallIgnoreErrors(.title_destroy, .{ .title_cap_id = title_cap_id });

        const title_input_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1, .address = TITLE_INPUT_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = title_input_shm_cap_id });

        try writeStrToInputCap(@as([*]u8, @ptrFromInt(TITLE_INPUT_ACQUIRE_ADDRESS))[0..4096], title);

        const title_output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = title_output_shm_cap_id });

        return Self{
            .title_cap_id = title_cap_id,
            .title_input_shm_cap_id = title_input_shm_cap_id,
            .title_output_shm_cap_id = title_output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = self.title_output_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = self.title_input_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.title_destroy, .{ .title_cap_id = self.title_cap_id });
    }

    fn titlePublish(self: *const Self) os_nushift.SyscallError!usize {
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
        errdefer _ = os_nushift.syscallIgnoreErrors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = a11y_tree_cap_id });

        const a11y_input_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 10, .address = A11Y_INPUT_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = a11y_input_shm_cap_id });

        try writeStrToInputCap(@as([*]u8, @ptrFromInt(A11Y_INPUT_ACQUIRE_ADDRESS))[0..40960], ron);

        const a11y_output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = a11y_output_shm_cap_id });

        return Self{
            .a11y_tree_cap_id = a11y_tree_cap_id,
            .a11y_input_shm_cap_id = a11y_input_shm_cap_id,
            .a11y_output_shm_cap_id = a11y_output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = self.a11y_output_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = self.a11y_input_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = self.a11y_tree_cap_id });
    }

    fn accessibilityTreePublish(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.accessibility_tree_publish, .{ .accessibility_tree_cap_id = self.a11y_tree_cap_id, .input_shm_cap_id = self.a11y_input_shm_cap_id, .output_shm_cap_id = self.a11y_output_shm_cap_id });
    }
};

const GfxGetOutputsTask = struct {
    gfx_cap_id: usize,
    output_shm_cap_id: usize,

    const Self = @This();

    fn init() os_nushift.SyscallError!Self {
        const gfx_cap_id = try os_nushift.syscall(.gfx_new, .{});
        errdefer _ = os_nushift.syscallIgnoreErrors(.gfx_destroy, .{ .gfx_cap_id = gfx_cap_id });

        const output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = output_shm_cap_id });

        return Self{
            .gfx_cap_id = gfx_cap_id,
            .output_shm_cap_id = output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = self.output_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.gfx_destroy, .{ .gfx_cap_id = self.gfx_cap_id });
    }

    fn gfxGetOutputs(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.gfx_get_outputs, .{ .gfx_cap_id = self.gfx_cap_id, .output_shm_cap_id = self.output_shm_cap_id });
    }
};

const GfxCpuPresentTask = struct {
    gfx_cap_id: usize,
    present_buffer_shm_cap_id: usize,
    gfx_cpu_present_buffer_cap_id: usize,
    output_shm_cap_id: usize,

    const Self = @This();

    fn init(gfx_cap_id: usize, output_width: u64) (os_nushift.SyscallError || FBSWriteError || error{ NotQOI, EndOfStream })!Self {
        // 1 MiB buffer
        const present_buffer_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 256, .address = PRESENT_BUFFER_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = present_buffer_shm_cap_id });

        try writeWrappedImageToInputCap(@as([*]u8, @ptrFromInt(PRESENT_BUFFER_ACQUIRE_ADDRESS))[0..1048576], hello_world_qoi_data, output_width);

        const gfx_cpu_present_buffer_cap_id = try os_nushift.syscall(.gfx_cpu_present_buffer_new, .{ .gfx_cap_id = gfx_cap_id, .present_buffer_format = os_nushift.PresentBufferFormat.r8g8b8_uint_srgb, .present_buffer_shm_cap_id = present_buffer_shm_cap_id });
        errdefer _ = os_nushift.syscallIgnoreErrors(.gfx_cpu_present_buffer_destroy, .{ .gfx_cpu_present_buffer_cap_id = gfx_cpu_present_buffer_cap_id });

        const output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = output_shm_cap_id });

        return Self{
            .gfx_cap_id = gfx_cap_id,
            .present_buffer_shm_cap_id = present_buffer_shm_cap_id,
            .gfx_cpu_present_buffer_cap_id = gfx_cpu_present_buffer_cap_id,
            .output_shm_cap_id = output_shm_cap_id,
        };
    }

    fn deinit(self: Self) void {
        _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = self.output_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.gfx_cpu_present_buffer_destroy, .{ .gfx_cpu_present_buffer_cap_id = self.gfx_cpu_present_buffer_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = self.present_buffer_shm_cap_id });
    }

    fn gfxCpuPresent(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.gfx_cpu_present, .{ .gfx_cpu_present_buffer_cap_id = self.gfx_cpu_present_buffer_cap_id, .wait_for_vblank = std.math.maxInt(usize), .output_shm_cap_id = self.output_shm_cap_id });
    }
};

fn blockOnDeferredTasks(task_ids: []const u64) (FBSWriteError || os_nushift.SyscallError)!void {
    const block_on_deferred_tasks_input_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1, .address = BODT_INPUT_ACQUIRE_ADDRESS });
    defer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = block_on_deferred_tasks_input_cap_id });

    try writeTaskIdsToInputCap(@as([*]u8, @ptrFromInt(BODT_INPUT_ACQUIRE_ADDRESS))[0..4096], task_ids);

    _ = try os_nushift.syscall(.block_on_deferred_tasks, .{ .input_shm_cap_id = block_on_deferred_tasks_input_cap_id });
}

fn getOutput0WidthPx(gfx_get_outputs_task: *const GfxGetOutputsTask) (os_nushift.SyscallError || gfx_output.Error)!u64 {
    _ = try os_nushift.syscall(.shm_acquire, .{ .shm_cap_id = gfx_get_outputs_task.output_shm_cap_id, .address = GGO_OUTPUT_ACQUIRE_ADDRESS });

    const output_cap_buffer = @as([*]u8, @ptrFromInt(GGO_OUTPUT_ACQUIRE_ADDRESS))[0..4096];
    var stream = std.io.fixedBufferStream(output_cap_buffer);
    const reader = stream.reader();
    const outputs = try gfx_output.readOutputs(reader);

    _ = os_nushift.syscallIgnoreErrors(.shm_release, .{ .shm_cap_id = gfx_get_outputs_task.output_shm_cap_id });

    return outputs[0].size_px[0];
}

fn writeStrToInputCap(input_cap_buffer: []u8, str: []const u8) FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, str.len);
    _ = try writer.write(str);
}

fn writeTaskIdsToInputCap(input_cap_buffer: []u8, task_ids: []const u64) FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, task_ids.len);
    for (task_ids) |task_id| {
        try std.leb.writeULEB128(writer, task_id);
    }
}

fn writeWrappedImageToInputCap(input_cap_buffer: []u8, qoi_data: []const u8, output_width: u64) (FBSWriteError || error{ NotQOI, EndOfStream })!void {
    const MARGIN_TOP: u32 = 100;
    const QOI_HEADER_SIZE: u4 = 14;

    if (qoi_data.len < QOI_HEADER_SIZE) {
        return error.NotQOI;
    }
    if (!std.mem.eql(u8, "qoif", qoi_data[0..4])) {
        return error.NotQOI;
    }

    const img_width = std.mem.readIntBig(u32, qoi_data[4..8]);
    const img_height = std.mem.readIntBig(u32, qoi_data[8..12]);

    var qoi_read_stream = std.io.fixedBufferStream(qoi_data[14..]);
    const qoi_reader = qoi_read_stream.reader();
    var decoder = qoi.decoder(qoi_reader);

    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    const margin_top_bytes_len = MARGIN_TOP * output_width * 3;
    const decoded_bytes_len = margin_top_bytes_len + (img_height * output_width * 3);

    try std.leb.writeULEB128(writer, decoded_bytes_len);

    // Write top margin
    try writer.writeByteNTimes(0xFF, margin_top_bytes_len);

    // Calculate left margin and right margin for centering image
    const margin_left = @divTrunc(output_width - @min(output_width, img_width), 2);
    // Can be negative, indicating we are going to cut off the right-hand side of the image
    const margin_right: i64 = @as(i64, @intCast(output_width)) - @as(i64, @intCast(img_width)) - @as(i64, @intCast(margin_left));

    // Variable to store remaining pixels from the last color_run that can be continued on the next row.
    var overflow: ?qoi.ColorRun = null;

    for (0..img_height) |_| {
        // Write left margin
        try writer.writeByteNTimes(0xFF, margin_left * 3);

        // Calculate the number of pixels to write for the current row.
        var row_remaining: u64 = img_width;

        if (overflow) |of| {
            // Continue from a previous row's run.
            const of_remain = try writeColorRun(writer, of, row_remaining, @max(0, img_width + @min(0, margin_right) - @as(i64, @intCast(row_remaining))));
            if (of_remain == 0) {
                overflow = null;
            } else {
                overflow = qoi.ColorRun{
                    .color = of.color,
                    .length = of_remain,
                };
                row_remaining -= (of.length - of_remain);
            }
        }

        while (row_remaining > 0) {
            const color_run = try decoder.fetch();
            const remaining = try writeColorRun(writer, color_run, row_remaining, @max(0, img_width + @min(0, margin_right) - @as(i64, @intCast(row_remaining))));

            if (remaining == 0) {
                row_remaining -= color_run.length;
            } else {
                overflow = qoi.ColorRun{
                    .color = color_run.color,
                    .length = remaining,
                };
                row_remaining = 0;
            }
        }

        if (margin_right > 0) {
            // Write right margin only if it's positive.
            try writer.writeByteNTimes(0xFF, @as(u64, @intCast(margin_right)) * 3);
        } else if (margin_right < 0) {
            // Skip the runs from the right margin that should be cut off.
            var skip = -margin_right;
            while (skip > 0) {
                const color_run = try decoder.fetch();
                if (color_run.length <= skip) {
                    skip -= @as(i64, @intCast(color_run.length));
                } else {
                    overflow = qoi.ColorRun{
                        .color = color_run.color,
                        .length = color_run.length - @as(u64, @intCast(skip)),
                    };
                    skip = 0;
                }
            }
        }
    }
}

fn writeColorRun(writer: anytype, color_run: qoi.ColorRun, remaining: u64, remaining_before_cutoff: u64) FBSWriteError!u64 {
    // Determine the number of pixels to write from the current run.
    const write_count = @min(remaining, color_run.length);

    for (0..write_count) |i| {
        // If this is in the area being cut off by the right margin, actually
        // don't write it.
        if (i >= remaining_before_cutoff) {
            break;
        }
        try writer.writeByte(color_run.color.r);
        try writer.writeByte(color_run.color.g);
        try writer.writeByte(color_run.color.b);
    }

    // Return the number of pixels still remaining in the current run after writing.
    return color_run.length - write_count;
}
