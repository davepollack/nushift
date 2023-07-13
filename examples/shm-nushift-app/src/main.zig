const OsNushift = @import("os_nushift");

pub fn main() usize {
    const new_result = OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1 });
    const shm_cap_id = switch (new_result) {
        .ok => |val| val,
        .fail => |err_enum| return @enumToInt(err_enum),
    };

    const acquire_result = OsNushift.syscall(.shm_acquire, .{ .shm_cap_id = shm_cap_id, .address = 0x90000000 });
    switch (acquire_result) {
        .ok => {},
        .fail => |err_enum| return @enumToInt(err_enum),
    }

    const release_result = OsNushift.syscall(.shm_release, .{ .shm_cap_id = shm_cap_id });
    switch (release_result) {
        .ok => {},
        .fail => |err_enum| return @enumToInt(err_enum),
    }

    const destroy_result = OsNushift.syscall(.shm_destroy, .{ .shm_cap_id = shm_cap_id });
    switch (destroy_result) {
        .ok => return 0,
        .fail => |err_enum| return @enumToInt(err_enum),
    }
}
