# nushift

A new shift in running programs with shareable URLs.

[screenshot here]

## Overview

Nushift is a new way of using software that has the accessibility and shareability of web URLs, but uses different technologies to the web. Apps (or pages) are RISC-V programs, that interact with a syscall ABI defined by us.

The Nushift hypervisor is focused on being small and having a strict API. Layout is entirely done by apps themselves, while communicating what is being done through the accessibility tree API.

## SHM

SHM is the main method of communication between apps and the hypervisor. While it stands for "shared memory", this is mainly to distinguish it from IPC, and the ability to share it is limited.

A region of memory is a "cap" (capability), and this is distinguished from the address it may be mapped into. An SHM cap can only be mapped into one logical thread of execution at a time. To communicate with the hypervisor, an SHM cap is unmapped from the app and mapped into the hypervisor, while the hypervisor is working on it.

## Storage

TODO. The planned storage system will not be a filesystem API, which has been the cause of many security vulnerabilities. The storage concepts will interact with each other in a more secure and better way than filesystem APIs.

## Networking

TODO. The networking story of browsers is one of the weakest parts of browsers. Apps should be able to use more networking functionality than they can in browsers.

## nsq://

TODO!

It should be as easy to start a secure server serving Nushift programs as it is to start an SSH server.

## Running

Please run the nushift GUI desktop application from the `nushift` directory. I.e. `cd nushift && cargo run`. NOT `cargo run -p nushift`. This is required for internationalised strings to work.
