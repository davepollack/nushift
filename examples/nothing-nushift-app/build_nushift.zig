const std = @import("std");

pub fn build(b: *std.Build, appName: []const u8, mainPath: []const u8) void {
    buildWithOptions(b, appName, mainPath, true);
}

/// Call with `strip`: false if you want to add back debug symbols.
pub fn buildWithOptions(b: *std.Build, appName: []const u8, mainPath: []const u8, strip: bool) void {
    // Standard target options allows the person running `zig build` to choose
    // what target to build for. Here we set a default target. Other options for
    // restricting supported target set are available.
    const target = b.standardTargetOptions(.{
        .default_target = .{
            .cpu_arch = .riscv64,
            .os_tag = .freestanding,
            .abi = .none,
        }
    });

    // Standard optimize options allow the person running `zig build` to select
    // between Debug, ReleaseSafe, ReleaseFast, and ReleaseSmall.
    const optimize = b.standardOptimizeOption(.{});

    const os_nushift_module = b.createModule(.{
        .source_file = .{ .path = "../../nulib/src/os_nushift.zig" },
        .dependencies = &.{},
    });

    const main_module = b.createModule(.{
        .source_file = .{ .path = mainPath },
        .dependencies = &.{
            .{ .name = "os_nushift", .module = os_nushift_module },
        },
    });

    const exe = b.addExecutable(.{
        .name = appName,
        .root_source_file = .{ .path = "../../nulib/src/start_nushift.zig" },
        .target = target,
        .optimize = optimize,
    });
    exe.addModule("main", main_module);
    exe.addModule("os_nushift", os_nushift_module);
    exe.strip = strip;
    exe.install();

    const run_cmd = exe.run();
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run the app");
    run_step.dependOn(&run_cmd.step);

    const exe_tests = b.addTest(.{
        .root_source_file = .{ .path = mainPath },
        .target = target,
        .optimize = optimize,
    });

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&exe_tests.step);
}