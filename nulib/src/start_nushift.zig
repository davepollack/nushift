const OsNushift = @import("os_nushift");
const main = @import("main");

export fn _start() callconv(.Naked) noreturn {
    const exit_reason = main.main();
    _ = OsNushift.syscall(.exit, .{ .exit_reason = exit_reason });
    unreachable;
}
