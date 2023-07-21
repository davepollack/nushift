const OsNushift = @import("os_nushift");

pub fn main() usize {
    const a11y_tree_new_cap_result = OsNushift.syscall(.accessibility_tree_new_cap, .{});
    const a11y_tree_cap_id = switch (a11y_tree_new_cap_result) {
        .ok => |val| val,
        .fail => |err_enum| return @intFromEnum(err_enum),
    };

    const destroy_result = OsNushift.syscall(.accessibility_tree_destroy_cap, .{ .accessibility_tree_cap_id = a11y_tree_cap_id });
    switch (destroy_result) {
        .ok => {},
        .fail => |err_enum| return @intFromEnum(err_enum),
    }

    return a11y_tree_cap_id + 1000;
}
