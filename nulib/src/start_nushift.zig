const std = @import("std");
const builtin = @import("builtin");
const os_nushift = @import("os_nushift");
const main = @import("main");

// The stack. 256 KiB, but you can change it if you want.
const STACK_END: usize = 0x80000000;
const STACK_NUMBER_OF_4_KIB_PAGES: usize = 64;

export fn _start() callconv(.Naked) noreturn {
    // Since Zig 0.11.0, this has to be inline assembly code rather than Zig
    // code, due to restrictions placed on naked functions.

    var a0_output: usize = undefined;
    var t0_output: usize = undefined;

    // Init stack
    asm volatile ("ecall"
        : [ret_a0] "={a0}" (a0_output),
          [ret_t0] "={t0}" (t0_output),
        : [syscall_number] "{a0}" (os_nushift.Syscall.shm_new_and_acquire),
          [shm_type] "{a1}" (os_nushift.ShmType.four_kib),
          [length] "{a2}" (STACK_NUMBER_OF_4_KIB_PAGES),
          [address] "{a3}" (STACK_END),
        : "memory", "t0", "a0", "a1", "a2", "a3"
    );

    // If error initing stack, exit
    if (a0_output == std.math.maxInt(usize)) {
        asm volatile ("ecall"
            :
            : [syscall_number] "{a0}" (os_nushift.Syscall.exit),
              [exit_reason] "{a1}" (t0_output),
            : "memory", "t0", "a0", "a1"
        );

        // I would like to put `unreachable` here. But I'm not allowed to since
        // Zig 0.11.0. So, I'm emulating it.
        if (builtin.mode == .Debug or builtin.mode == .ReleaseSafe) {
            // Spin on ebreak, what Zig usually emits in these modes
            asm volatile (
                \\ eb:
                \\ ebreak
                \\ j eb
            );
        }
    }

    // Set SP to base
    asm volatile (""
        :
        : [sp_val] "{sp}" (STACK_END + (4096 * STACK_NUMBER_OF_4_KIB_PAGES)),
        : "sp"
    );

    // Call main
    const exit_reason = asm volatile ("call %[main]@plt"
        : [exit_reason] "={a0}" (-> usize),
        : [main] "X" (&main.main),
        : "memory", "ra", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"
    );

    // Deinit stack, ignoring errors
    asm volatile ("ecall"
        :
        : [syscall_number] "{a0}" (os_nushift.Syscall.shm_release_and_destroy),
          [shm_cap_id] "{a1}" (a0_output),
        : "memory", "t0", "a0", "a1"
    );

    // Exit
    asm volatile ("ecall"
        :
        : [syscall_number] "{a0}" (os_nushift.Syscall.exit),
          [exit_reason] "{a1}" (exit_reason),
        : "memory", "t0", "a0", "a1"
    );
}
