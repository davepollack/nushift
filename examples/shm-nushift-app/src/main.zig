const OsNushift = @import("os_nushift");

const ACQUIRE_ADDRESS: usize = 0x90000000;

pub fn main() usize {
    const new_result = OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1 });
    const shm_cap_id = switch (new_result) {
        .ok => |val| val,
        .fail => |err_enum| return @intFromEnum(err_enum),
    };

    const acquire_result = OsNushift.syscall(.shm_acquire, .{ .shm_cap_id = shm_cap_id, .address = ACQUIRE_ADDRESS });
    switch (acquire_result) {
        .ok => {},
        .fail => |err_enum| return @intFromEnum(err_enum),
    }

    const array: *[512]u64 = @ptrFromInt(ACQUIRE_ADDRESS);
    for (array, 0..) |*item, i| {
        item.* = i;
    }
    const one_hundred = array[100];

    const release_result = OsNushift.syscall(.shm_release, .{ .shm_cap_id = shm_cap_id });
    switch (release_result) {
        .ok => {},
        .fail => |err_enum| return @intFromEnum(err_enum),
    }

    const destroy_result = OsNushift.syscall(.shm_destroy, .{ .shm_cap_id = shm_cap_id });
    switch (destroy_result) {
        .ok => {},
        .fail => |err_enum| return @intFromEnum(err_enum),
    }

    return one_hundred;
}
