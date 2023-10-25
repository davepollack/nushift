const std = @import("std");
const BuildNushift = @import("./build_nushift.zig");

pub fn build(b: *std.build.Builder) void {
    BuildNushift.build(b, "shm-nushift-app", "src/main.zig", true, null);
}
