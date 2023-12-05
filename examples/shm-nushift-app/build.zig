// Copyright 2023 The Nushift Authors.
//
// Distributed under the Boost Software License, Version 1.0.
// (See accompanying file LICENSE or copy at
// https://www.boost.org/LICENSE_1_0.txt)

const std = @import("std");
const build_nushift = @import("./build_nushift.zig");

pub fn build(b: *std.Build) void {
    build_nushift.build(b, "shm-nushift-app", "src/main.zig", true, null);
}
