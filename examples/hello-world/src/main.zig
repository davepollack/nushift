// Copyright 2023 The Nushift Authors.
//
// Distributed under the Boost Software License, Version 1.0.
// (See accompanying file LICENSE or copy at
// https://www.boost.org/LICENSE_1_0.txt)

const std = @import("std");
const os_nushift = @import("os_nushift");
const qoi = @import("qoi");

const writing = @import("./writing.zig");
const AccessibilityTree = @import("./AccessibilityTree.zig");
const GfxOutput = @import("./GfxOutput.zig");

const Allocator = std.mem.Allocator;

const hello_world_qoi_data = @embedFile("./assets/hello_world.qoi");
const title: []const u8 = "Hello World App";

const TITLE_INPUT_ACQUIRE_ADDRESS: usize = 0x90000000;
const A11Y_INPUT_ACQUIRE_ADDRESS: usize = 0x90001000;
const BODT_INPUT_ACQUIRE_ADDRESS: usize = 0x9000b000;
const GGO_OUTPUT_ACQUIRE_ADDRESS: usize = 0x9000c000;
const PRESENT_BUFFER_ACQUIRE_ADDRESS: usize = 0x90200000;
const ALLOCATOR_BUFFER_ACQUIRE_ADDRESS: usize = 0x96600000;
const DEBUG_PRINT_INPUT_ACQUIRE_ADDRESS: usize = 0x96700000;
const CPU_PRESENT_BUFFER_ARGS_INPUT_ACQUIRE_ADDRESS: usize = 0x96701000;

const MARGIN_TOP: u32 = 70;

pub fn main() usize {
    return mainImpl() catch |err| blk: {
        // While there is no particular reason for this error_message to be
        // computed at comptime anymore, for some reason if we change it to
        // runtime, the .text section size jumps by 45%.
        const error_message = switch (err) {
            inline else => |any_err| std.fmt.comptimePrint("Error: {s}", .{@errorName(any_err)}),
        };
        debugPrint(error_message) catch break :blk 1;
        break :blk 1;
    };
}

fn mainImpl() (writing.FBSWriteError || os_nushift.SyscallError || GfxOutput.Error || qoi.DecodeError || Allocator.Error)!usize {
    var title_outputs_tasks = blk: {
        var title_task = try TitleTask.init();
        errdefer title_task.deinit();

        var gfx_get_outputs_task = try GfxGetOutputsTask.init();
        errdefer gfx_get_outputs_task.deinit();

        break :blk .{ title_task, gfx_get_outputs_task };
    };

    const title_task_id = try title_outputs_tasks[0].titlePublish();
    const gfx_get_outputs_task_id = try title_outputs_tasks[1].gfxGetOutputs();

    // If an error occurs between publishing and the end of
    // block_on_deferred_tasks, you can't deinit the tasks because the resources
    // are in-flight. And we don't. But that does mean the task resources will
    // leak if that error occurs.

    try blockOnDeferredTasks(&.{ title_task_id, gfx_get_outputs_task_id });

    title_outputs_tasks[0].deinit();
    // Do not deinit the outputs task yet, since we need to get the output
    // information and keep the gfx_cap_id

    var image_and_allocator = try ImageAndAllocator.init();
    defer image_and_allocator.deinit();

    // For some reason specifying .always_inline for this extracted logic is
    // needed, otherwise the binary size blows up by 70% :'(
    const gfx_output_0 = try @call(.always_inline, getGfxOutput0, .{&title_outputs_tasks[1]});
    const margin = getMargin(gfx_output_0.size_px[0], image_and_allocator.image);

    var a11y_present_tasks = blk: {
        var a11y_tree_task = try AccessibilityTreeTask.init(image_and_allocator.allocator(), margin.left, image_and_allocator.image);
        errdefer a11y_tree_task.deinit();

        var gfx_cpu_present_task = try GfxCpuPresentTask.init(image_and_allocator.allocator(), title_outputs_tasks[1].gfx_cap_id, gfx_output_0.id, gfx_output_0.size_px[0], gfx_output_0.size_px[1], margin.left, margin.right, image_and_allocator.image);
        errdefer gfx_cpu_present_task.deinit();

        break :blk .{ a11y_tree_task, gfx_cpu_present_task };
    };

    const a11y_tree_publish_task_id = try a11y_present_tasks[0].accessibilityTreePublish();
    const gfx_cpu_present_task_id = try a11y_present_tasks[1].gfxCpuPresent();

    // If an error occurs between publishing and the end of
    // block_on_deferred_tasks, you can't deinit the tasks because the resources
    // are in-flight. And we don't. But that does mean the task resources will
    // leak if that error occurs.

    try blockOnDeferredTasks(&.{ a11y_tree_publish_task_id, gfx_cpu_present_task_id });

    a11y_present_tasks[1].deinit();
    a11y_present_tasks[0].deinit();

    title_outputs_tasks[1].deinit();

    return 0;
}

const ImageAndAllocator = struct {
    allocator_buffer_shm_cap_id: usize,
    fixed_buffer_allocator: std.heap.FixedBufferAllocator,
    image: qoi.Image,

    const Self = @This();

    fn init() (os_nushift.SyscallError || qoi.DecodeError)!Self {
        // 1 MiB buffer for allocator
        const allocator_buffer_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 256, .address = ALLOCATOR_BUFFER_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = allocator_buffer_shm_cap_id });
        var fixed_buffer_allocator = std.heap.FixedBufferAllocator.init(@as([*]u8, @ptrFromInt(ALLOCATOR_BUFFER_ACQUIRE_ADDRESS))[0..1048576]);

        const fba_allocator = fixed_buffer_allocator.allocator();
        var image = try qoi.decodeBuffer(fba_allocator, hello_world_qoi_data);
        errdefer image.deinit(fba_allocator);

        return Self{
            .allocator_buffer_shm_cap_id = allocator_buffer_shm_cap_id,
            .fixed_buffer_allocator = fixed_buffer_allocator,
            .image = image,
        };
    }

    fn deinit(self: *Self) void {
        self.image.deinit(self.allocator());
        self.fixed_buffer_allocator.reset();
        _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = self.allocator_buffer_shm_cap_id });

        self.* = undefined;
    }

    fn allocator(self: *Self) Allocator {
        return self.fixed_buffer_allocator.allocator();
    }
};

const TitleTask = struct {
    title_cap_id: usize,
    title_input_shm_cap_id: usize,
    title_output_shm_cap_id: usize,

    const Self = @This();

    fn init() (writing.FBSWriteError || os_nushift.SyscallError)!Self {
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

    fn deinit(self: *Self) void {
        _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = self.title_output_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = self.title_input_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.title_destroy, .{ .title_cap_id = self.title_cap_id });

        self.* = undefined;
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

    fn init(allocator: Allocator, margin_left: u64, image: qoi.Image) (writing.FBSWriteError || os_nushift.SyscallError || Allocator.Error)!Self {
        var a11y_tree = try AccessibilityTree.initOneTextItem(allocator);
        defer a11y_tree.deinit();

        const top_left_x: AccessibilityTree.VirtualPoint = @floatFromInt(margin_left);
        const top_left_y: AccessibilityTree.VirtualPoint = @floatFromInt(MARGIN_TOP);
        const bottom_right_x: AccessibilityTree.VirtualPoint = @floatFromInt(margin_left + image.width); // Allowed to exceed the output dimensions (I guess?)
        const bottom_right_y: AccessibilityTree.VirtualPoint = @floatFromInt(MARGIN_TOP + image.height); // Allowed to exceed the output dimensions (I guess?)

        try a11y_tree.surfaces.items[0].display_list.items[0].text.aabb[0].append(top_left_x);
        try a11y_tree.surfaces.items[0].display_list.items[0].text.aabb[0].append(top_left_y);
        try a11y_tree.surfaces.items[0].display_list.items[0].text.aabb[1].append(bottom_right_x);
        try a11y_tree.surfaces.items[0].display_list.items[0].text.aabb[1].append(bottom_right_y);
        try a11y_tree.surfaces.items[0].display_list.items[0].text.text.appendSlice("Hello, world!");

        const a11y_tree_cap_id = try os_nushift.syscall(.accessibility_tree_new, .{});
        errdefer _ = os_nushift.syscallIgnoreErrors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = a11y_tree_cap_id });

        const a11y_input_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 10, .address = A11Y_INPUT_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = a11y_input_shm_cap_id });

        var stream = std.io.fixedBufferStream(@as([*]u8, @ptrFromInt(A11Y_INPUT_ACQUIRE_ADDRESS))[0..40960]);
        const writer = stream.writer();

        try a11y_tree.write(writer);

        const a11y_output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = a11y_output_shm_cap_id });

        return Self{
            .a11y_tree_cap_id = a11y_tree_cap_id,
            .a11y_input_shm_cap_id = a11y_input_shm_cap_id,
            .a11y_output_shm_cap_id = a11y_output_shm_cap_id,
        };
    }

    fn deinit(self: *Self) void {
        _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = self.a11y_output_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = self.a11y_input_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.accessibility_tree_destroy, .{ .accessibility_tree_cap_id = self.a11y_tree_cap_id });

        self.* = undefined;
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

    fn deinit(self: *Self) void {
        _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = self.output_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.gfx_destroy, .{ .gfx_cap_id = self.gfx_cap_id });

        self.* = undefined;
    }

    fn gfxGetOutputs(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.gfx_get_outputs, .{ .gfx_cap_id = self.gfx_cap_id, .output_shm_cap_id = self.output_shm_cap_id });
    }
};

const GfxCpuPresentTask = struct {
    gfx_cap_id: usize,
    gfx_output_id: u64,
    present_buffer_shm_cap_id: usize,
    gfx_cpu_present_buffer_cap_id: usize,
    output_shm_cap_id: usize,

    const Self = @This();

    fn init(allocator: Allocator, gfx_cap_id: usize, gfx_output_id: u64, gfx_output_width_px: u64, gfx_output_height_px: u64, margin_left: u64, margin_right: i64, image: qoi.Image) (os_nushift.SyscallError || writing.FBSWriteError || qoi.DecodeError)!Self {
        // 100 MiB present buffer
        const present_buffer_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.two_mib, .length = 50, .address = PRESENT_BUFFER_ACQUIRE_ADDRESS });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = present_buffer_shm_cap_id });

        try writeWrappedImageToInputCap(allocator, @as([*]u8, @ptrFromInt(PRESENT_BUFFER_ACQUIRE_ADDRESS))[0..104857600], image, gfx_output_width_px, gfx_output_height_px, margin_left, margin_right);

        // Write CPU present buffer args to an input cap
        const input_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1, .address = CPU_PRESENT_BUFFER_ARGS_INPUT_ACQUIRE_ADDRESS });
        defer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = input_shm_cap_id });
        try writeCpuPresentBufferArgsToInputCap(@as([*]u8, @ptrFromInt(CPU_PRESENT_BUFFER_ARGS_INPUT_ACQUIRE_ADDRESS))[0..4096], os_nushift.PresentBufferFormat.r8g8b8_uint_srgb, &.{ gfx_output_width_px, gfx_output_height_px }, present_buffer_shm_cap_id);

        // Now pass the input cap containing the args to GfxCpuPresentBufferNew
        const gfx_cpu_present_buffer_cap_id = try os_nushift.syscall(.gfx_cpu_present_buffer_new, .{ .gfx_cap_id = gfx_cap_id, .input_shm_cap_id = input_shm_cap_id });
        errdefer _ = os_nushift.syscallIgnoreErrors(.gfx_cpu_present_buffer_destroy, .{ .gfx_cpu_present_buffer_cap_id = gfx_cpu_present_buffer_cap_id });

        const output_shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
        errdefer _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = output_shm_cap_id });

        return Self{
            .gfx_cap_id = gfx_cap_id,
            .gfx_output_id = gfx_output_id,
            .present_buffer_shm_cap_id = present_buffer_shm_cap_id,
            .gfx_cpu_present_buffer_cap_id = gfx_cpu_present_buffer_cap_id,
            .output_shm_cap_id = output_shm_cap_id,
        };
    }

    fn deinit(self: *Self) void {
        _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = self.output_shm_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.gfx_cpu_present_buffer_destroy, .{ .gfx_cpu_present_buffer_cap_id = self.gfx_cpu_present_buffer_cap_id });
        _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = self.present_buffer_shm_cap_id });

        self.* = undefined;
    }

    fn gfxCpuPresent(self: *const Self) os_nushift.SyscallError!usize {
        return os_nushift.syscall(.gfx_cpu_present, .{ .gfx_cpu_present_buffer_cap_id = self.gfx_cpu_present_buffer_cap_id, .gfx_output_id = self.gfx_output_id, .wait_for_vblank = std.math.maxInt(usize), .output_shm_cap_id = self.output_shm_cap_id });
    }
};

fn blockOnDeferredTasks(task_ids: []const u64) (writing.FBSWriteError || os_nushift.SyscallError)!void {
    const block_on_deferred_tasks_input_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1, .address = BODT_INPUT_ACQUIRE_ADDRESS });
    defer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = block_on_deferred_tasks_input_cap_id });

    try writeTaskIdsToInputCap(@as([*]u8, @ptrFromInt(BODT_INPUT_ACQUIRE_ADDRESS))[0..4096], task_ids);

    _ = try os_nushift.syscall(.block_on_deferred_tasks, .{ .input_shm_cap_id = block_on_deferred_tasks_input_cap_id });
}

fn getGfxOutput0(gfx_get_outputs_task: *const GfxGetOutputsTask) (os_nushift.SyscallError || GfxOutput.Error)!GfxOutput {
    _ = try os_nushift.syscall(.shm_acquire, .{ .shm_cap_id = gfx_get_outputs_task.output_shm_cap_id, .address = GGO_OUTPUT_ACQUIRE_ADDRESS });

    const output_cap_buffer = @as([*]u8, @ptrFromInt(GGO_OUTPUT_ACQUIRE_ADDRESS))[0..4096];
    var stream = std.io.fixedBufferStream(output_cap_buffer);
    const reader = stream.reader();
    const gfx_outputs = try GfxOutput.readGfxOutputs(reader);

    _ = os_nushift.syscallIgnoreErrors(.shm_release, .{ .shm_cap_id = gfx_get_outputs_task.output_shm_cap_id });

    return gfx_outputs[0];
}

fn getMargin(gfx_output_width_px: u64, image: qoi.Image) struct { left: u64, right: i64 } {
    // Calculate left margin and right margin for centering image
    const margin_left = @divTrunc(gfx_output_width_px - @min(gfx_output_width_px, image.width), 2);
    // Can be negative, indicating we are going to cut off the right-hand side of the image
    const margin_right: i64 = @as(i64, @intCast(gfx_output_width_px)) - @as(i64, @intCast(image.width)) - @as(i64, @intCast(margin_left));

    return .{ .left = margin_left, .right = margin_right };
}

fn writeStrToInputCap(input_cap_buffer: []u8, str: []const u8) writing.FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, str.len);

    _ = try writer.write(str);
}

fn writeTaskIdsToInputCap(input_cap_buffer: []u8, task_ids: []const u64) writing.FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try writing.writeU64Seq(writer, task_ids);
}

fn writeCpuPresentBufferArgsToInputCap(input_cap_buffer: []u8, present_buffer_format: os_nushift.PresentBufferFormat, present_buffer_size_px: []const u64, present_buffer_shm_cap_id: usize) writing.FBSWriteError!void {
    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, @intFromEnum(present_buffer_format));
    try writing.writeU64Seq(writer, present_buffer_size_px);
    try std.leb.writeULEB128(writer, present_buffer_shm_cap_id);
}

fn writeWrappedImageToInputCap(allocator: Allocator, input_cap_buffer: []u8, image: qoi.Image, gfx_output_width_px: u64, gfx_output_height_px: u64, margin_left: u64, margin_right: i64) writing.FBSWriteError!void {
    debugPrint("Copying image, will take a while...") catch {};

    var stream = std.io.fixedBufferStream(input_cap_buffer);
    const writer = stream.writer();

    try std.leb.writeULEB128(writer, gfx_output_width_px * gfx_output_height_px * 3);

    // Write top margin
    try writer.writeByteNTimes(0xFF, gfx_output_width_px * MARGIN_TOP * 3);

    var current_pixel_pos: usize = 0;

    for (0..@min(image.height, gfx_output_height_px - MARGIN_TOP)) |_| {
        // Write left margin
        try writer.writeByteNTimes(0xFF, margin_left * 3);

        // Write either all of the row, or some of the row if the image is being cut off on the right-hand side
        const pixels_to_write = @max(0, @min(image.width, image.width + margin_right));

        for (image.pixels[current_pixel_pos .. current_pixel_pos + pixels_to_write]) |pixel| {
            try writer.writeByte(pixel.r);
            try writer.writeByte(pixel.g);
            try writer.writeByte(pixel.b);
        }

        current_pixel_pos += image.width;

        // Write right margin
        if (margin_right > 0) {
            const margin_right_usize: usize = @intCast(margin_right);
            try writer.writeByteNTimes(0xFF, margin_right_usize * 3);
        }
    }

    // Write bottom margin
    try writer.writeByteNTimes(0xFF, @max(0, @as(i64, @intCast(gfx_output_height_px)) - @as(i64, MARGIN_TOP) - @as(i64, image.height)) * gfx_output_width_px * 3);

    debugPrint(std.fmt.allocPrint(allocator, "Done. {d} bytes written", .{stream.pos}) catch "Fmt error") catch {};
}

fn debugPrint(str: []const u8) (writing.FBSWriteError || os_nushift.SyscallError)!void {
    const debug_print_input_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1, .address = DEBUG_PRINT_INPUT_ACQUIRE_ADDRESS });
    defer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = debug_print_input_cap_id });

    const debug_print_buffer = @as([*]u8, @ptrFromInt(DEBUG_PRINT_INPUT_ACQUIRE_ADDRESS))[0..4096];
    var stream = std.io.fixedBufferStream(debug_print_buffer);
    const writer = stream.writer();
    try std.leb.writeULEB128(writer, str.len);
    _ = try writer.write(str);

    _ = try os_nushift.syscall(.debug_print, .{ .input_shm_cap_id = debug_print_input_cap_id });
}
