// Copyright 2024 The Nushift Authors.
//
// Distributed under the Boost Software License, Version 1.0.
// (See accompanying file LICENSE or copy at
// https://www.boost.org/LICENSE_1_0.txt)

const std = @import("std");

pub fn build(b: *std.Build) void {
    _ = b.addModule("os_nushift", .{ .root_source_file = b.path("src/os_nushift.zig") });
    _ = b.addModule("start_nushift", .{ .root_source_file = b.path("src/start_nushift.zig") });
}

/// This function is used by Nushift apps to encapsulate common logic for
/// building Nushift apps.
pub fn build_nushift(
    b: *std.Build,
    app_name: []const u8,
    main_path: []const u8,
    exe_callback: ?*const fn (b: *std.Build, exe: *std.Build.Step.Compile, main_module: *std.Build.Module) void,
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

    const strip = b.option(bool, "strip", "Strip debug info") orelse true;

    const this_dep = b.dependencyFromBuildZig(@This(), .{});

    const main_module = b.createModule(.{
        .root_source_file = b.path(main_path),
    });
    main_module.addImport("os_nushift", this_dep.module("os_nushift"));

    const exe = b.addExecutable(.{
        .name = app_name,
        .root_source_file = this_dep.path("src/start_nushift.zig"),
        .target = target,
        .optimize = optimize,
    });
    exe.root_module.addImport("main", main_module);
    exe.root_module.addImport("os_nushift", this_dep.module("os_nushift"));
    exe.root_module.strip = strip;
    if (exe_callback) |present_exe_callback| {
        present_exe_callback(b, exe, main_module);
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
        .root_source_file = b.path(main_path),
        .target = target,
        .optimize = optimize,
    });

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&exe_tests.step);
}
