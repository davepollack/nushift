const std = @import("std");

const MAX_OUTPUTS: usize = 4;
const MAX_DIMENSIONS: usize = 3;

pub const Error = error{ UnsupportedOutputs, UnsupportedDimensions, EndOfStream, Overflow };

pub const Output = struct {
    size_px: [MAX_DIMENSIONS]u64,
    scale: [MAX_DIMENSIONS]u64,

    const Self = @This();

    fn read_output(self: *Self, reader: anytype) Error!void {
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
            self.scale[i] = try std.leb.readULEB128(u64, reader);
        }
    }
};

pub fn read_outputs(reader: anytype) Error![MAX_OUTPUTS]Output {
    const outputs_length = try std.leb.readULEB128(usize, reader);
    if (outputs_length > MAX_OUTPUTS) {
        return error.UnsupportedOutputs;
    }

    var outputs: [MAX_OUTPUTS]Output = undefined;

    for (0..outputs_length) |i| {
        try outputs[i].read_output(reader);
    }

    return outputs;
}
