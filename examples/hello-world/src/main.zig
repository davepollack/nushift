const std = @import("std");
const OsNushift = @import("os_nushift");
const ron = @embedFile("./accessibility_tree.ron");

const ACQUIRE_ADDRESS: usize = 0x90000000;

pub fn main() usize {
    const a11y_tree_new_cap_result = OsNushift.syscall(.accessibility_tree_new_cap, .{});
    const a11y_tree_cap_id = switch (a11y_tree_new_cap_result) {
        .ok => |val| val,
        .fail => |err_enum| return @intFromEnum(err_enum),
    };

    const input_shm_cap_result = OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 10 });
    const input_shm_cap_id = switch (input_shm_cap_result) {
        .ok => |val| val,
        .fail => |err_enum| return @intFromEnum(err_enum),
    };

    const acquire_result = OsNushift.syscall(.shm_acquire, .{ .shm_cap_id = input_shm_cap_id, .address = ACQUIRE_ADDRESS });
    switch (acquire_result) {
        .ok => {},
        .fail => |err_enum| return @intFromEnum(err_enum),
    }
    const ron_length_dest: *[@sizeOf(usize)]u8 = @ptrFromInt(ACQUIRE_ADDRESS);
    const data_dest: *[ron.len]u8 = @ptrFromInt(ACQUIRE_ADDRESS + @sizeOf(usize));
    std.mem.writeIntLittle(usize, ron_length_dest, ron.len);
    @memcpy(data_dest, ron);

    const publish_result = OsNushift.syscall(.accessibility_tree_publish, .{ .accessibility_tree_cap_id = a11y_tree_cap_id, .input_shm_cap_id = input_shm_cap_id });
    switch (publish_result) {
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
