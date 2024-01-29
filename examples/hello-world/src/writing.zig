// Copyright 2024 The Nushift Authors.
//
// Distributed under the Boost Software License, Version 1.0.
// (See accompanying file LICENSE or copy at
// https://www.boost.org/LICENSE_1_0.txt)

const std = @import("std");

pub fn writeU64Seq(writer: anytype, seq: []const u64) !void {
    try std.leb.writeULEB128(writer, seq.len);

    for (seq) |elem| {
        try std.leb.writeULEB128(writer, elem);
    }
}

pub fn writeF64Seq(writer: anytype, seq: []const f64) !void {
    try std.leb.writeULEB128(writer, seq.len);

    for (seq) |elem| {
        try writeF64(writer, elem);
    }
}

pub fn writeF64(writer: anytype, value: f64) !void {
    try writer.writeIntLittle(u64, @as(u64, @bitCast(value)));
}

pub fn writeStr(writer: anytype, str: []const u8) !void {
    try std.leb.writeULEB128(writer, str.len);
    _ = try writer.write(str);
}
