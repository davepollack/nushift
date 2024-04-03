// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::{any::Any, sync::{Arc, Mutex}};

use bytes::BytesMut;
use quinn_proto::{
    crypto::{CryptoError, ExportKeyingMaterialError, HeaderKey, KeyPair, Keys, PacketKey, Session},
    transport_parameters::TransportParameters,
    ConnectionId,
    Side,
    TransportError,
};
use snow::{HandshakeState, TransportState};

// TODO: Remove when this is used
#[allow(dead_code)]
enum SnowSession {
    Handshaking(HandshakeState),
    Transport(Arc<Mutex<TransportState>>),
}

impl Session for SnowSession {
    fn initial_keys(&self, _dst_cid: &ConnectionId, _side: Side) -> Keys {
        todo!()
    }

    fn handshake_data(&self) -> Option<Box<dyn Any>> {
        todo!()
    }

    fn peer_identity(&self) -> Option<Box<dyn Any>> {
        todo!()
    }

    fn early_crypto(&self) -> Option<(Box<dyn HeaderKey>, Box<dyn PacketKey>)> {
        todo!()
    }

    fn early_data_accepted(&self) -> Option<bool> {
        todo!()
    }

    fn is_handshaking(&self) -> bool {
        todo!()
    }

    fn read_handshake(&mut self, _buf: &[u8]) -> Result<bool, TransportError> {
        todo!()
    }

    fn transport_parameters(&self) -> Result<Option<TransportParameters>, TransportError> {
        todo!()
    }

    fn write_handshake(&mut self, _buf: &mut Vec<u8>) -> Option<Keys> {
        todo!()
    }

    fn next_1rtt_keys(&mut self) -> Option<KeyPair<Box<dyn PacketKey>>> {
        todo!()
    }

    fn is_valid_retry(&self, _orig_dst_cid: &ConnectionId, _header: &[u8], _payload: &[u8]) -> bool {
        todo!()
    }

    fn export_keying_material(
        &self,
        _output: &mut [u8],
        _label: &[u8],
        _context: &[u8],
    ) -> Result<(), ExportKeyingMaterialError> {
        todo!()
    }
}

struct TransportStateKey(Arc<Mutex<TransportState>>);

impl PacketKey for TransportStateKey {
    fn encrypt(&self, _packet: u64, buf: &mut [u8], header_len: usize) {
        // Read payload (copy required since snow does not support encrypt-in-place)
        let mut payload = vec![];
        let Some(payload_end_index) = buf.len().checked_sub(self.tag_len()) else {
            buf.fill(0);
            return;
        };
        let Some(buf_reslice_read_payload) = buf.get(header_len..payload_end_index) else {
            buf.fill(0);
            return;
        };
        payload.extend_from_slice(buf_reslice_read_payload);

        // Write/encrypt payload and tag
        let Some(buf_reslice_encrypt) = buf.get_mut(header_len..) else {
            buf.fill(0);
            return;
        };
        // Using this currently doesn't support using the header (or anything)
        // as associated data, but other impls do use the header as associated
        // data
        if let Err(_) = self.0.lock().unwrap().write_message(&payload, buf_reslice_encrypt) {
            buf.fill(0);
            return;
        }
    }

    fn decrypt(
        &self,
        _packet: u64,
        _header: &[u8],
        payload: &mut BytesMut,
    ) -> Result<(), CryptoError> {
        // Copying is required because snow doesn't support decrypt-in-place
        let mut payload_copied = vec![];
        payload_copied.extend_from_slice(payload);

        // Using this currently doesn't support using the header (or anything)
        // as associated data, but other impls do use the header as associated
        // data
        let plain_len = self.0.lock().unwrap()
            .read_message(&payload_copied, payload)
            .map_err(|_| CryptoError)?;

        payload.truncate(plain_len);

        Ok(())
    }

    fn tag_len(&self) -> usize {
        16
    }

    fn confidentiality_limit(&self) -> u64 {
        // From https://eprint.iacr.org/2023/085, the confidentiality and
        // integrity limits seem well above 2^64 for ChaCha20-Poly1305
        u64::MAX
    }

    fn integrity_limit(&self) -> u64 {
        // From https://eprint.iacr.org/2023/085, the confidentiality and
        // integrity limits seem well above 2^64 for ChaCha20-Poly1305
        u64::MAX
    }
}
