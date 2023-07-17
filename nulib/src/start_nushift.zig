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
    const STACK_BASE: usize = 0x80000000;
    const STACK_NUMBER_OF_4_KIB_PAGES: usize = 64;

    const new_and_acquire_result = OsNushift.syscall(.shm_new_and_acquire, .{
        .shm_type = OsNushift.ShmType.four_kib,
        .length = STACK_NUMBER_OF_4_KIB_PAGES,
        .address = STACK_BASE - (4096 * STACK_NUMBER_OF_4_KIB_PAGES),
    });
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
        : [sp_val] "{sp}" (STACK_BASE),
        : "sp"
    );

    return shm_cap_id;
}

fn deinit_stack(shm_cap_id: usize) void {
    _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = shm_cap_id });
}
