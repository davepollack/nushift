// Copyright 2024 The Nushift Authors.
//
// Distributed under the Boost Software License, Version 1.0.
// (See accompanying file LICENSE or copy at
// https://www.boost.org/LICENSE_1_0.txt)

const std = @import("std");
const Allocator = std.mem.Allocator;

const writing = @import("./writing.zig");

const AccessibilityTree = @This();

allocator: Allocator,
surfaces: std.ArrayList(Surface),

pub const VirtualPoint = f64;

const Surface = struct {
    const Self = @This();

    display_list: std.ArrayList(DisplayItem),

    const DisplayItem = union(DisplayItems) {
        text: struct { aabb: struct { std.ArrayList(VirtualPoint), std.ArrayList(VirtualPoint) }, text: std.ArrayList(u8) },
    };

    const DisplayItems = enum {
        text,
    };
};

pub fn initOneTextItem(allocator: Allocator) Allocator.Error!AccessibilityTree {
    const display_item = Surface.DisplayItem{
        .text = .{
            .aabb = .{ std.ArrayList(VirtualPoint).init(allocator), std.ArrayList(VirtualPoint).init(allocator) },
            .text = std.ArrayList(u8).init(allocator),
        },
    };

    var surface = Surface{
        .display_list = std.ArrayList(Surface.DisplayItem).init(allocator),
    };
    try surface.display_list.append(display_item);

    var a11y_tree = AccessibilityTree{
        .allocator = allocator,
        .surfaces = std.ArrayList(Surface).init(allocator),
    };
    try a11y_tree.surfaces.append(surface);

    return a11y_tree;
}

pub fn deinit(self: *AccessibilityTree) void {
    for (self.surfaces.items) |surface| {
        for (surface.display_list.items) |display_item| {
            switch (display_item) {
                .text => |text_item| {
                    text_item.aabb[0].deinit();
                    text_item.aabb[1].deinit();
                    text_item.text.deinit();
                },
            }
        }
        surface.display_list.deinit();
    }
    self.surfaces.deinit();

    self.* = undefined;
}

pub fn write(self: *AccessibilityTree, writer: anytype) !void {
    try std.leb.writeULEB128(writer, self.surfaces.items.len);

    for (self.surfaces.items) |surface| {
        try std.leb.writeULEB128(writer, surface.display_list.items.len);

        for (surface.display_list.items) |display_item| {
            switch (display_item) {
                .text => |text_item| {
                    // struct_variant, discriminant 0
                    try std.leb.writeULEB128(writer, @as(u8, 0)); // Cast just because writeULEB128 doesn't accept comptime_int

                    try writing.writeF64Seq(writer, text_item.aabb[0].items);
                    try writing.writeF64Seq(writer, text_item.aabb[1].items);
                    try writing.writeStr(writer, text_item.text.items);
                },
            }
        }
    }
}
