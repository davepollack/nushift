const std = @import("std");
const BuildNushift = @import("./build_nushift.zig");

const qoi_path = @import("root").dependencies.build_root.qoi;

pub fn build(b: *std.Build) void {
    BuildNushift.build(b, "hello-world", "src/main.zig", true, addDependencies);
}

fn addDependencies(b: *std.Build, exe: *std.Build.Step.Compile) void {
    exe.addAnonymousModule("qoi", .{ .source_file = .{ .path = b.pathJoin(&.{ qoi_path, "src/qoi.zig" }) } });
}
