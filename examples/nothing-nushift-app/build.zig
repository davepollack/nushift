const std = @import("std");
const BuildNushift = @import("./build_nushift.zig");

pub fn build(b: *std.Build) void {
    BuildNushift.build(b, "nothing-nushift-app", "src/main.zig", true, null);
}
