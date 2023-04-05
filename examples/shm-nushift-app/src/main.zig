const OsNushift = @import("os_nushift");

pub fn main() usize {
    // TODO: Check error register
    const shm_cap_id = OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib });
    const destroy_result = OsNushift.syscall(.shm_destroy, .{ .shm_cap_id = shm_cap_id });
    if (destroy_result == 0) {
        return 0;
    }
    return 1;
}
