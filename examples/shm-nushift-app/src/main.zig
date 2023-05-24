const OsNushift = @import("os_nushift");

pub fn main() usize {
    const new_result = OsNushift.syscall(.shm_new, .{ .type = OsNushift.ShmType.four_kib, .length = 1 });
    const shm_cap_id = switch (new_result) {
        .success => |val| val,
        .@"error" => |err_enum| return @enumToInt(err_enum),
    };

    const destroy_result = OsNushift.syscall(.shm_destroy, .{ .shm_cap_id = shm_cap_id });
    switch (destroy_result) {
        .success => return 0,
        .@"error" => |err_enum| return @enumToInt(err_enum),
    }
}
