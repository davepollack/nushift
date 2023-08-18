const std = @import("std");
const OsNushift = @import("os_nushift");
const ron = @embedFile("./accessibility_tree.ron");

const title: []const u8 = "Hello World App";

const TITLE_INPUT_ACQUIRE_ADDRESS: usize = 0x90000000;
const A11Y_INPUT_ACQUIRE_ADDRESS: usize = 0x90001000;

pub fn main() usize {
    const title_new_cap_result = OsNushift.syscall(.title_new_cap, .{});
    const title_cap_id = switch (title_new_cap_result) {
        .ok => |val| val,
        .fail => |err_enum| return @intFromEnum(err_enum),
    };
    const title_input_shm_cap_result = OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1, .address = TITLE_INPUT_ACQUIRE_ADDRESS });
    const title_input_shm_cap_id = switch (title_input_shm_cap_result) {
        .ok => |val| val,
        .fail => |err_enum| return @intFromEnum(err_enum),
    };

    write_to_input_cap(TITLE_INPUT_ACQUIRE_ADDRESS, title);

    const title_publish_result = OsNushift.syscall(.title_publish, .{ .title_cap_id = title_cap_id, .input_shm_cap_id = title_input_shm_cap_id });
    switch (title_publish_result) {
        .ok => {},
        .fail => |err_enum| return @intFromEnum(err_enum),
    }

    // TODO: Destroy input SHM cap? Wait for it to be remapped? Destroy title cap?

    const a11y_tree_new_cap_result = OsNushift.syscall(.accessibility_tree_new_cap, .{});
    const a11y_tree_cap_id = switch (a11y_tree_new_cap_result) {
        .ok => |val| val,
        .fail => |err_enum| return @intFromEnum(err_enum),
    };
    const a11y_input_shm_cap_result = OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 10, .address = A11Y_INPUT_ACQUIRE_ADDRESS });
    const a11y_input_shm_cap_id = switch (a11y_input_shm_cap_result) {
        .ok => |val| val,
        .fail => |err_enum| return @intFromEnum(err_enum),
    };

    write_to_input_cap(A11Y_INPUT_ACQUIRE_ADDRESS, ron);

    const a11y_publish_result = OsNushift.syscall(.accessibility_tree_publish, .{ .accessibility_tree_cap_id = a11y_tree_cap_id, .input_shm_cap_id = a11y_input_shm_cap_id });
    switch (a11y_publish_result) {
        .ok => {},
        .fail => |err_enum| return @intFromEnum(err_enum),
    }

    // TODO: Destroy input SHM cap? Wait for it to be remapped?

    const destroy_result = OsNushift.syscall(.accessibility_tree_destroy_cap, .{ .accessibility_tree_cap_id = a11y_tree_cap_id });
    switch (destroy_result) {
        .ok => {},
        .fail => |err_enum| return @intFromEnum(err_enum),
    }

    return a11y_tree_cap_id + 1000;
}

fn write_to_input_cap(comptime address: usize, comptime str: []const u8) void {
    const str_length_dest: *[@sizeOf(usize)]u8 = @ptrFromInt(address);
    const data_dest: *[str.len]u8 = @ptrFromInt(address + @sizeOf(usize));
    std.mem.writeIntLittle(usize, str_length_dest, str.len);
    @memcpy(data_dest, str);
}
