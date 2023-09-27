const OsNushift = @import("os_nushift");

const ACQUIRE_ADDRESS: usize = 0x90000000;

pub fn main() usize {
    return main_impl() catch |err| OsNushift.errorCodeFromSyscallError(err);
}

fn main_impl() OsNushift.SyscallError!usize {
    const shm_cap_id = try OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 1 });
    defer _ = OsNushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = shm_cap_id });

    _ = try OsNushift.syscall(.shm_acquire, .{ .shm_cap_id = shm_cap_id, .address = ACQUIRE_ADDRESS });
    defer _ = OsNushift.syscall_ignore_errors(.shm_release, .{ .shm_cap_id = shm_cap_id });

    const array: *[512]u64 = @ptrFromInt(ACQUIRE_ADDRESS);
    for (array, 0..) |*item, i| {
        item.* = i;
    }
    const one_hundred = array[100];

    return one_hundred;
}
