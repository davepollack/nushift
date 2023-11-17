# nushift

A new shift in running programs with shareable URLs.

[screenshot here]

## Overview

Nushift attempts to be an alternative to the web that has the accessibility and shareability of web URLs, but uses different technologies than the web. Apps (or pages) are RISC-V programs, that interact with a syscall ABI defined by us.

The Nushift hypervisor is focused on being small and having a strict API. Visual layout is entirely done by apps themselves, while communicating what is being done through the accessibility tree API.

## SHM

SHM is the main method of communication between apps and the hypervisor. While it stands for "shared memory", this is mainly to distinguish it from IPC, and the ability to share it is limited.

A region of memory is a cap (capability), and this is distinguished from the address it may be mapped into. An SHM cap can only be mapped into one logical thread of execution at a time. To communicate with the hypervisor, an SHM cap is unmapped from the app and mapped into the hypervisor, while the hypervisor is working on it.

## 64-bit versus 32-bit

Currently, RV64IMC is supported. Support for more extensions will almost certainly be added. The hypervisor API expects 64-bit values, and this is likely to remain the case when 32-bit apps are supported. 64-bit values, for the purposes of the hypervisor API, should be encoded into one or multiple 32-bit registers by 32-bit apps.

## Hypervisor ABI

The syscall number is passed in `a0`.

The (optional) first, second and third arguments are passed in `a1`, `a2`, `a3`.

The `ecall` instruction is used to issue the call.

Successful return values are returned in `a0`.

If an error occurs, `a0` is set to `u64::MAX`, and the error code is returned in `t0`.

## SHM API

### ShmType (enum)

`FourKiB` = 0,\
`TwoMiB` = 1,\
`OneGiB` = 2.

These correspond to the page and superpage sizes available in the Sv39 scheme described in the RISC-V privileged specification. Apps, furthermore, currently have access to the Sv39 scheme (39-bit virtual addressing giving a total of 512 GiB virtual space, and 56-bit physical addressing). Support for Sv48 and Sv57, and their associated superpage sizes, may be added in the future.

### ShmNew

Arguments: type (`ShmType`), length (`u64`).\
Returns: shm_cap_id (`u64`).\
Errors: `InternalError`, `Exhausted`, `ShmUnknownShmType`, `ShmInvalidLength`, `ShmCapacityNotAvailable`

Creates a new SHM cap. The size (in bytes) of the backing memory of the cap is the page size (in bytes) represented by the `ShmType` provided multiplied by the `length` provided. For example, `ShmType::FourKiB` and a `length` of `1` produces a cap that logically holds one 4 KiB page, and has a total backing memory size of 4096 bytes.

`length` must be greater than 0.

On current commodity operating systems, mmap is used to reserve the memory when you call `ShmNew`.

### ShmAcquire

Arguments: shm_cap_id (`u64`), address (`u64`).\
Returns: `0u64`.\
Errors: `InternalError`, `CapNotFound`, `PermissionDenied`, `ShmCapCurrentlyAcquired`, `ShmAddressOutOfBounds`, `ShmAddressNotAligned`, `ShmOverlapsExistingAcquisition`

Maps (acquires) the requested cap into the app at the requested address.

`address` must be page-aligned to the page type of the provided `shm_cap_id`, and must be less than 2<sup>39</sup>, due to the current Sv39 scheme.

### ShmNewAndAcquire

Arguments: type (`ShmType`), length (`u64`), address (`u64`).\
Returns: shm_cap_id (`u64`).\
Errors: `InternalError`, `Exhausted`, `ShmUnknownShmType`, `ShmInvalidLength`, `ShmCapacityNotAvailable`, `ShmAddressOutOfBounds`, `ShmAddressNotAligned`, `ShmOverlapsExistingAcquisition`

Calls `ShmNew` and `ShmAcquire` in one system call.

### ShmRelease

Arguments: shm_cap_id (`u64`).\
Returns: `0u64`.\
Errors: `InternalError`, `CapNotFound`, `PermissionDenied`

Unmaps (releases) the requested cap from the app.

Silently succeeds if the requested cap is not currently acquired.

### ShmDestroy

Arguments: shm_cap_id (`u64`).\
Returns: `0u64`.\
Errors: `CapNotFound`, `PermissionDenied`, `ShmCapCurrentlyAcquired`

Deletes a cap.

The cap must be released before destroying, otherwise `ShmCapCurrentlyAcquired` is returned.

### ShmReleaseAndDestroy

Arguments: shm_cap_id (`u64`).\
Returns: `0u64`.\
Errors: `InternalError`, `CapNotFound`, `PermissionDenied`

Calls `ShmRelease` and `ShmDestroy` in one system call.

## Accessibility Tree API

### AccessibilityTreeNew

Arguments: None.\
Returns: accessibility_tree_cap_id (`u64`).\
Errors: `InternalError`, `Exhausted`

Creates a new accessibility tree capability, that can be used to publish an accessibility tree.

### AccessibilityTreePublish

Arguments: accessibility_tree_cap_id (`u64`), input_shm_cap_id (`u64`), output_shm_cap_id (`u64`).\
Returns: task_id (`u64`).\
Errors: `InternalError`, `Exhausted`, `CapNotFound`, `InProgress`, `PermissionDenied`

Starts a task to publish the RON-based accessibility tree contained in `input_shm_cap_id`, to the hypervisor.

The format of the data in the cap represented by `input_shm_cap_id` is in [Postcard](https://postcard.jamesmunns.com/wire-format) format, but is a simple string. Hence, it will be a varint-encoded length followed by the string data, as per the Postcard format. An API call that publishes the accessibility tree in full Postcard format should be added in the future, and then this existing call may be renamed to `AccessibilityTreePublishRON`.

As with other deferred-style calls:
* This releases `input_shm_cap_id` and `output_shm_cap_id` and then you can't access them anymore
* It accepts `input_shm_cap_id` and `output_shm_cap_id` that are already released
* The `output_shm_cap_id` cap is created by you, and the hypervisor will write the output of the deferred call to it

An error will be written to the `output_shm_cap_id` cap if the Postcard data cannot be deserialised, or the RON string itself cannot be deserialised. Nothing is written if the call is a success. The lack of a discriminant between success and error values is a serious deficiency in the output format that should be addressed by the Nushift team. The output format is itself in the Postcard format.

### AccessibilityTreeDestroy

Arguments: accessibility_tree_cap_id (`u64`).\
Returns: `0u64`.\
Errors: `CapNotFound`

Destroys an accessibility tree capability. This does not destroy any published accessibility trees.

## Errors (API)

### SyscallError (enum)

`UnknownSyscall` = 0,

The syscall number was not recognised.

`InternalError` = 1,

Should never happen, and indicates a bug in Nushift's code.

`Exhausted` = 2,

The maximum amount of capabilities in this particular capability space have been used. Please destroy some capabilities.

`CapNotFound` = 6,

A capability in this particular capability space with the requested capability ID could not be found.

`InProgress` = 11,

Currently, it is not possible to queue/otherwise process a second deferred operation in a deferred-capable space while one is being processed in that space. This limitation should be removed in the future.

`PermissionDenied` = 12,

An SHM cap ID was provided that is not of the expected SHM cap type. For example, a system-created SHM cap used for storing the program ELF data was provided where an application-created SHM cap was expected.

`ShmUnknownShmType` = 3,

The value provided for the `ShmType` enum was unrecognised.

`ShmInvalidLength` = 4,

The `length` provided in the SHM API call was invalid, for example 0 is invalid.

`ShmCapacityNotAvailable` = 5,

There is not enough available capacity to support this length of this SHM type. Or, there is not enough available backing capacity, currently using mmap, to support this length of this SHM type. Or, the requested capacity in bytes overflows either u64 or usize on this host platform. Note that length in the system call arguments is number of this SHM type's pages, not number of bytes.

`ShmCapCurrentlyAcquired` = 7,

The requested SHM cap is currently acquired. Therefore, it cannot be acquired again, nor destroyed. Please release it first.

`ShmAddressOutOfBounds` = 8,

The requested acquisition address is not within Sv39 (39-bit virtual addressing) bounds.

`ShmAddressNotAligned` = 9,

The requested acquisition address is not aligned at the SHM cap's type (e.g. 4 KiB-aligned, 2 MiB-aligned or 1 GiB-aligned).

`ShmOverlapsExistingAcquisition` = 10,

The requested acquisition address combined with the `length` in the SHM cap forms a range that overlaps an existing acquisition. Please choose a different address.

## Storage

TODO. The planned storage system will not be a filesystem API, which has been the cause of many security vulnerabilities. The storage concepts will interact with each other in a more secure and better way than filesystem APIs.

## Networking

TODO. The networking story of browsers is one of the weakest parts of browsers. Apps should be able to use more networking functionality than they can in browsers.

## nsq://

TODO!

It should be as easy to start a secure server serving Nushift programs as it is to start an SSH server.

## Running

Please run the nushift GUI desktop application from the `nushift` directory. I.e. `cd nushift && cargo run`. NOT `cargo run -p nushift`. This is required for internationalised strings to work.
