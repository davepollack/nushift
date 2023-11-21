const std = @import("std");

pub fn build(
    b: *std.Build,
    app_name: []const u8,
    main_path: []const u8,
    strip: bool,
    exe_callback: ?*const fn (b: *std.Build, exe: *std.Build.Step.Compile) void,
) void {
    // Standard target options allows the person running `zig build` to choose
    // what target to build for. Here we set a default target. Other options for
    // restricting supported target set are available.
    const target = b.standardTargetOptions(.{
        .default_target = .{
            .cpu_arch = .riscv64,
            // For now, use a CPU that doesn't support floating point
            // extensions. In the future, hypervisor support for this should be
            // added.
            .cpu_features_sub = std.Target.riscv.featureSet(&.{.d}),
            .os_tag = .freestanding,
            .abi = .none,
        },
    });

    // Standard optimize options allow the person running `zig build` to select
    // between Debug, ReleaseSafe, ReleaseFast, and ReleaseSmall.
    const optimize = b.standardOptimizeOption(.{});

    const os_nushift_module = b.createModule(.{
        .source_file = .{ .path = "../../nulib/src/os_nushift.zig" },
        .dependencies = &.{},
    });

    const main_module = b.createModule(.{
        .source_file = .{ .path = main_path },
        .dependencies = &.{
            .{ .name = "os_nushift", .module = os_nushift_module },
        },
    });

    const exe = b.addExecutable(.{
        .name = app_name,
        .root_source_file = .{ .path = "../../nulib/src/start_nushift.zig" },
        .target = target,
        .optimize = optimize,
    });
    exe.addModule("main", main_module);
    exe.addModule("os_nushift", os_nushift_module);
    exe.strip = strip;
    if (exe_callback) |present_exe_callback| {
        present_exe_callback(b, exe);
    }
    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run the app");
    run_step.dependOn(&run_cmd.step);

    const exe_tests = b.addTest(.{
        .root_source_file = .{ .path = main_path },
        .target = target,
        .optimize = optimize,
    });

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&exe_tests.step);
}
