const std = @import("std");
const OsNushift = @import("os_nushift");
const main = @import("main");

export fn _start() callconv(.Naked) noreturn {
    const shm_cap_id = init_stack();
    const exit_reason = main.main();
    deinit_stack(shm_cap_id);
    _ = OsNushift.syscall_ignore_errors(.exit, .{ .exit_reason = exit_reason });
    unreachable;
}

fn init_stack() usize {
    // The stack. 256 KiB, but you can change it if you want.
    const new_and_acquire_result = OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 64, .address = (0x80000000 - (4096 * 64)) });
    const shm_cap_id = switch (new_and_acquire_result) {
        .ok => |val| val,
        .fail => |err_enum| {
            _ = OsNushift.syscall_ignore_errors(.exit, .{ .exit_reason = @intFromEnum(err_enum) });
            unreachable;
        },
    };

    // Set SP to base
    asm volatile (""
        :
        : [sp_val] "{sp}" (0x80000000),
        : "sp"
    );

    return shm_cap_id;
}

fn deinit_stack(shm_cap_id: usize) void {
    _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = shm_cap_id });
}
