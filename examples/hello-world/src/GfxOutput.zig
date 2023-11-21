const std = @import("std");

const MAX_GFX_OUTPUTS: usize = 4;
const MAX_DIMENSIONS: usize = 3;

const GfxOutput = @This();

size_px: [MAX_DIMENSIONS]u64,
scale: [MAX_DIMENSIONS]f64,

pub const Error = error{ UnsupportedGfxOutputs, UnsupportedDimensions, EndOfStream, Overflow };

fn readGfxOutput(self: *GfxOutput, reader: anytype) Error!void {
    const size_px_length = try std.leb.readULEB128(usize, reader);
    if (size_px_length > MAX_DIMENSIONS) {
        return error.UnsupportedDimensions;
    }
    for (0..size_px_length) |i| {
        self.size_px[i] = try std.leb.readULEB128(u64, reader);
    }

    const scale_length = try std.leb.readULEB128(usize, reader);
    if (scale_length > MAX_DIMENSIONS) {
        return error.UnsupportedDimensions;
    }
    for (0..scale_length) |i| {
        self.scale[i] = @bitCast(try reader.readIntLittle(u64));
    }
}

pub fn readGfxOutputs(reader: anytype) Error![MAX_GFX_OUTPUTS]GfxOutput {
    const gfx_outputs_length = try std.leb.readULEB128(usize, reader);
    if (gfx_outputs_length > MAX_GFX_OUTPUTS) {
        return error.UnsupportedGfxOutputs;
    }

    var gfx_outputs: [MAX_GFX_OUTPUTS]GfxOutput = undefined;

    for (0..gfx_outputs_length) |i| {
        try gfx_outputs[i].readGfxOutput(reader);
    }

    return gfx_outputs;
}
