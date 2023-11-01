const os_nushift = @import("os_nushift");

const ACQUIRE_ADDRESS: usize = 0x90000000;

pub fn main() usize {
    return mainImpl() catch |err| os_nushift.errorCodeFromSyscallError(err);
}

fn mainImpl() os_nushift.SyscallError!usize {
    const shm_cap_id = try os_nushift.syscall(.shm_new, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1 });
    defer _ = os_nushift.syscallIgnoreErrors(.shm_destroy, .{ .shm_cap_id = shm_cap_id });

    _ = try os_nushift.syscall(.shm_acquire, .{ .shm_cap_id = shm_cap_id, .address = ACQUIRE_ADDRESS });
    defer _ = os_nushift.syscallIgnoreErrors(.shm_release, .{ .shm_cap_id = shm_cap_id });

    const array: *[512]u64 = @ptrFromInt(ACQUIRE_ADDRESS);
    for (array, 0..) |*item, i| {
        item.* = i;
    }
    const one_hundred = array[100];

    return one_hundred;
}
