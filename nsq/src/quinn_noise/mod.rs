// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use hex_literal::hex;

mod config;
mod session;
mod fixed_buffer;

// TODO: Ensure a wrapper function that creates a client endpoint config for the
// user, uses this version, and if appropriate, server configs too.
// TODO: Register range with QUIC WG.
const NSQ_QUIC_VERSION: u32 = 0x6e737100; // "nsq" + 0[0-f]

/// Derived by calling HKDF-Expand (*not* TLS 1.3 HKDF-Expand-Label), with
/// 0xd9c9943e6101fd200021506bcc02814c73030f25c79d71ce876eca876e6fca8e (retry
/// secret from RFC 9001) as PRK, 32 as length, "quic key" as info, and SHA-256
/// as the HMAC hash function
const RETRY_KEY: [u8; 32] = hex!("3337597c92ceb8fa6351d223fad8a795140f8976c25b9589f65c95740b1cd08b");
/// Derived by calling HKDF-Expand (*not* TLS 1.3 HKDF-Expand-Label), with
/// 0xd9c9943e6101fd200021506bcc02814c73030f25c79d71ce876eca876e6fca8e (retry
/// secret from RFC 9001) as PRK, 12 as length, "quic iv" as info, and SHA-256
/// as the HMAC hash function
const RETRY_NONCE: [u8; 12] = hex!("433b6818e1af1874007a4df3");
