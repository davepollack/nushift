const std = @import("std");
const build_nushift = @import("./build_nushift.zig");

pub fn build(b: *std.Build) void {
    build_nushift.build(b, "nothing-nushift-app", "src/main.zig", true, null);
}
