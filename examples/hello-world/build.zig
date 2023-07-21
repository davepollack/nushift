const std = @import("std");
const BuildNushift = @import("./build_nushift.zig");

pub fn build(b: *std.build.Builder) void {
    BuildNushift.build(b, "hello-world", "src/main.zig");
}
