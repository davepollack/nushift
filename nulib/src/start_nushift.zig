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

    // TODO: Use shm_new_and_acquire when that is implemented.
    const new_result = OsNushift.syscall(.shm_new, .{ .shm_type = OsNushift.ShmType.four_kib, .length = 64 });
    const shm_cap_id = switch (new_result) {
        .ok => |val| val,
        .fail => |err_enum| {
            _ = OsNushift.syscall_ignore_errors(.exit, .{ .exit_reason = @intFromEnum(err_enum) });
            unreachable;
        },
    };

    const acquire_result = OsNushift.syscall(.shm_acquire, .{ .shm_cap_id = shm_cap_id, .address = (0x80000000 - (4096 * 64)) });
    switch (acquire_result) {
        .ok => {},
        .fail => |err_enum| {
            _ = OsNushift.syscall_ignore_errors(.exit, .{ .exit_reason = @intFromEnum(err_enum) });
            unreachable;
        },
    }

    asm volatile (""
        :
        : [sp_val] "{sp}" (0x80000000),
        : "sp"
    );

    return shm_cap_id;
}

fn deinit_stack(shm_cap_id: usize) void {
    // TODO: Use shm_release_and_destroy when that is implemented.
    _ = OsNushift.syscall_ignore_errors(.shm_release, .{ .shm_cap_id = shm_cap_id });
    _ = OsNushift.syscall_ignore_errors(.shm_destroy, .{ .shm_cap_id = shm_cap_id });
}
