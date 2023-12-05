// Copyright 2023 The Nushift Authors.
//
// Distributed under the Boost Software License, Version 1.0.
// (See accompanying file LICENSE or copy at
// https://www.boost.org/LICENSE_1_0.txt)

const std = @import("std");
const build_nushift = @import("./build_nushift.zig");

const qoi_path = @import("root").dependencies.build_root.qoi;

pub fn build(b: *std.Build) void {
    build_nushift.build(b, "hello-world", "src/main.zig", true, addDependencies);
}

fn addDependencies(b: *std.Build, exe: *std.Build.Step.Compile) void {
    const qoi_module = b.createModule(.{ .source_file = .{ .path = b.pathJoin(&.{ qoi_path, "src/qoi.zig" }) } });
    const main_module = exe.modules.get("main") orelse @panic("main module should exist");
    main_module.dependencies.put("qoi", qoi_module) catch @panic("OOM");
}
