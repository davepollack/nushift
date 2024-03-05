// Copyright 2024 The Nushift Authors.
//
// Distributed under the Boost Software License, Version 1.0.
// (See accompanying file LICENSE or copy at
// https://www.boost.org/LICENSE_1_0.txt)

const std = @import("std");
const os_nushift = @import("os_nushift");

const ACQUIRE_ADDRESS: usize = 0x90000000;
const DEBUG_PRINT_INPUT_ACQUIRE_ADDRESS: usize = 0x90001000;

pub fn main() usize {
    return mainImpl() catch |err| os_nushift.errorCodeFromSyscallError(err);
}

fn mainImpl() os_nushift.SyscallError!usize {
    const input_shm_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = 1, .address = ACQUIRE_ADDRESS });
    defer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = input_shm_cap_id });

    const word: *usize = @ptrFromInt(ACQUIRE_ADDRESS);
    word.* = 1; // This is not a valid title string, but we don't care because we're not going to get past the initial setup part of the TitlePublish syscall
    debugPrint("OK: Acquired input cap is initially accessible") catch {};

    const title_cap_id = try os_nushift.syscall(.title_new, .{});
    defer _ = os_nushift.syscallIgnoreErrors(.title_destroy, .{ .title_cap_id = title_cap_id });

    // Make a TitlePublish syscall with an invalid output_shm_cap_id
    const result = os_nushift.syscall(.title_publish, .{ .title_cap_id = title_cap_id, .input_shm_cap_id = input_shm_cap_id, .output_shm_cap_id = 12345 });
    if (result) |_| {
        debugPrint("FAIL: We expected this TitlePublish syscall to error, but it succeeded.") catch {};
        return 1;
    } else |err| {
        if (err != error.CapNotFound) {
            debugPrint("FAIL: We expected a CapNotFound error from the TitlePublish syscall, but it returned a different error.") catch {};
            return err;
        }
    }

    word.* = 2;
    debugPrint("OK: Acquired input cap is still accessible (internal hypervisor changes were successfully rolled back!)") catch {};

    return 0;
}

fn debugPrint(str: []const u8) (std.io.FixedBufferStream(u8).WriteError || os_nushift.SyscallError)!void {
    // Maximum varint length of a u64 is 10 bytes
    const bytes_needed = 10 + str.len;
    const pages_needed = (bytes_needed + 4095) / 4096;

    const debug_print_input_cap_id = try os_nushift.syscall(.shm_new_and_acquire, .{ .shm_type = os_nushift.ShmType.four_kib, .length = pages_needed, .address = DEBUG_PRINT_INPUT_ACQUIRE_ADDRESS });
    defer _ = os_nushift.syscallIgnoreErrors(.shm_release_and_destroy, .{ .shm_cap_id = debug_print_input_cap_id });

    const debug_print_buffer = @as([*]u8, @ptrFromInt(DEBUG_PRINT_INPUT_ACQUIRE_ADDRESS))[0..bytes_needed];
    var stream = std.io.fixedBufferStream(debug_print_buffer);
    const writer = stream.writer();
    try std.leb.writeULEB128(writer, str.len);
    _ = try writer.write(str);

    _ = try os_nushift.syscall(.debug_print, .{ .input_shm_cap_id = debug_print_input_cap_id });
}
