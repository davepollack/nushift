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
    const STACK_END: usize = 0x80000000;
    const STACK_NUMBER_OF_4_KIB_PAGES: usize = 64;

    const shm_cap_id = OsNushift.syscall(.shm_new_and_acquire, .{ .shm_type = OsNushift.ShmType.four_kib, .length = STACK_NUMBER_OF_4_KIB_PAGES, .address = STACK_END }) catch {
        // Hardcode the .exit_reason rather than using the error from
        // .shm_new_and_acquire. Because if we do the latter, the stack is used
        // before we initialise the stack :( including in the happy path.
        _ = OsNushift.syscall_ignore_errors(.exit, .{ .exit_reason = @intFromEnum(OsNushift.SyscallErrorEnum.internal_error) });
        unreachable;
    };

    // Set SP to base
    asm volatile (""
        :
        : [sp_val] "{sp}" (STACK_END + (4096 * STACK_NUMBER_OF_4_KIB_PAGES)),
        : "sp"
    );

    return shm_cap_id;
}

fn deinit_stack(shm_cap_id: usize) void {
    _ = OsNushift.syscall_ignore_errors(.shm_release_and_destroy, .{ .shm_cap_id = shm_cap_id });
}
