// Copyright 2024 The Nushift Authors.
//
// Distributed under the Boost Software License, Version 1.0.
// (See accompanying file LICENSE or copy at
// https://www.boost.org/LICENSE_1_0.txt)

const std = @import("std");

pub const FBSWriter = std.io.FixedBufferStream([]u8).Writer;
pub const FBSWriteError = std.io.FixedBufferStream([]u8).WriteError;

pub fn writeU64Seq(writer: FBSWriter, seq: []const u64) FBSWriteError!void {
    try std.leb.writeULEB128(writer, seq.len);

    for (seq) |elem| {
        try std.leb.writeULEB128(writer, elem);
    }
}

pub fn writeF64Seq(writer: FBSWriter, seq: []const f64) FBSWriteError!void {
    try std.leb.writeULEB128(writer, seq.len);

    for (seq) |elem| {
        try writeF64(writer, elem);
    }
}

pub fn writeF64(writer: FBSWriter, value: f64) FBSWriteError!void {
    try writer.writeIntLittle(u64, @as(u64, @bitCast(value)));
}

pub fn writeStr(writer: FBSWriter, str: []const u8) FBSWriteError!void {
    try std.leb.writeULEB128(writer, str.len);
    _ = try writer.write(str);
}
