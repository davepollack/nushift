# nushift

A new shift in running programs with shareable URLs.

[screenshot here]

## Overview

Nushift is a new way of using software that has the accessibility and shareability of web URLs, but uses different technologies than the web. Apps (or pages) are RISC-V programs, that interact with a syscall ABI defined by us.

The Nushift hypervisor is focused on being small and having a strict API. Visual layout is entirely done by apps themselves, while communicating what is being done through the accessibility tree API.

## SHM

SHM is the main method of communication between apps and the hypervisor. While it stands for "shared memory", this is mainly to distinguish it from IPC, and the ability to share it is limited.

A region of memory is a "cap" (capability), and this is distinguished from the address it may be mapped into. An SHM cap can only be mapped into one logical thread of execution at a time. To communicate with the hypervisor, an SHM cap is unmapped from the app and mapped into the hypervisor, while the hypervisor is working on it.

## 64-bit versus 32-bit

Currently, riscv64 is supported. The hypervisor API expects 64-bit values, and this is likely to remain the case when 32-bit apps are supported. 64-bit values, for the purposes of the hypervisor API, should be encoded into one or multiple 32-bit registers by 32-bit apps.

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
Errors: `ShmInternalError`, `ShmExhausted`, `ShmUnknownShmType`, `ShmInvalidLength`, `ShmCapacityNotAvailable`

Creates a new SHM cap. The size (in bytes) of the backing memory of the cap is the page size (in bytes) represented by the `ShmType` provided multiplied by the `length` provided.

`length` must be greater than 0.

On current commodity operating systems, mmap is used to reserve the memory when you call `ShmNew`.

### ShmAcquire

Arguments: shm_cap_id (`u64`), address (`u64`).\
Returns: `0u64`\
Errors: `ShmInternalError`, `ShmCapCurrentlyAcquired`, `ShmCapNotFound`, `ShmAddressOutOfBounds`, `ShmAddressNotAligned`, `ShmOverlapsExistingAcquisition`

Maps (also called acquires) the requested cap into the app at the requested address.

`address` must be page-aligned to the page type of the provided `shm_cap_id` and must be less than 2<sup>39</sup>, due to the current Sv39 scheme.

## Storage

TODO. The planned storage system will not be a filesystem API, which has been the cause of many security vulnerabilities. The storage concepts will interact with each other in a more secure and better way than filesystem APIs.

## Networking

TODO. The networking story of browsers is one of the weakest parts of browsers. Apps should be able to use more networking functionality than they can in browsers.

## nsq://

TODO!

It should be as easy to start a secure server serving Nushift programs as it is to start an SSH server.

## Running

Please run the nushift GUI desktop application from the `nushift` directory. I.e. `cd nushift && cargo run`. NOT `cargo run -p nushift`. This is required for internationalised strings to work.
