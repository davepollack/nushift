// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::any::Any;

use chacha20::{
    cipher::{typenum::U10, KeyIvInit, StreamCipherCore, StreamCipherSeekCore},
    ChaChaCore,
};
use chacha20poly1305::{
    aead::{bytes::BytesMut, generic_array::typenum::Unsigned, AeadInPlace, Buffer, Error as AeadError, Result as AeadResult},
    AeadCore,
    ChaCha20Poly1305,
};
use quinn_proto::{
    crypto::{CryptoError, ExportKeyingMaterialError, HeaderKey, KeyPair, Keys, PacketKey, Session},
    transport_parameters::TransportParameters,
    ConnectionId,
    Side,
    TransportError,
};
use snow::HandshakeState;

// TODO: Remove when this is used
#[allow(dead_code)]
pub(crate) enum NoiseSession {
    SnowHandshaking(HandshakeState),
    Transport(Keys),
}

impl NoiseSession {
    // TODO: Remove when this is used
    #[allow(dead_code)]
    pub fn new(handshake_state: HandshakeState) -> Self {
        Self::SnowHandshaking(handshake_state)
    }
}

impl Session for NoiseSession {
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

struct TransportHeaderKey {
    /// Should be already derived from HKDF and the label "quic hp"
    hp_key: [u8; 32],
}

impl TransportHeaderKey {
    /// The "header_protection" function from RFC 9001 (QUIC-TLS). It requires
    /// use of the raw ChaCha20 function.
    fn header_protection(hp_key: [u8; 32], sample: [u8; 16]) -> [u8; 5] {
        let counter = sample[..4].try_into().expect("Should be of length 4");
        let nonce = &sample[4..];
        let mut mask = [0u8; 5];

        let mut chacha20_core = ChaChaCore::<U10>::new(&hp_key.into(), nonce.into());
        let int_counter = u32::from_le_bytes(counter);
        chacha20_core.set_block_pos(int_counter);
        chacha20_core.apply_keystream_partial((&mut mask[..]).into());

        mask
    }

    fn get_sample<const SL: usize>(pn_offset: usize, packet: &[u8]) -> Result<[u8; SL], ()> {
        let sample_offset = pn_offset.checked_add(4).ok_or(())?;
        let sample_end = sample_offset.checked_add(SL).ok_or(())?;
        Ok(packet.get(sample_offset..sample_end).ok_or(())?.try_into().map_err(|_| ())?)
    }

    /// The "sample algorithm for applying header protection" from RFC 9001
    /// (QUIC-TLS)
    fn encrypt_fallible(&self, pn_offset: usize, packet: &mut [u8]) -> Result<(), ()> {
        let sample = Self::get_sample(pn_offset, packet)?;
        let mask = Self::header_protection(self.hp_key, sample);

        // Get packet number length before masking it and preceding bits
        let packet_first_byte = packet.get_mut(0).ok_or(())?;
        let pn_length = (*packet_first_byte & 0x03).checked_add(1).ok_or(())?;

        // Mask packet number length and preceding bits
        if *packet_first_byte & 0x80 == 0x80 {
            // Long header: 4 bits masked
            *packet_first_byte ^= mask[0] & 0x0f;
        } else {
            // Short header: 5 bits masked
            *packet_first_byte ^= mask[0] & 0x1f;
        }

        // Obtain reference to packet number bytes
        let pn_end = pn_offset.checked_add(pn_length as usize).ok_or(())?;
        let pn_bytes = packet.get_mut(pn_offset..pn_end).ok_or(())?;

        // Mask the packet number
        pn_bytes.iter_mut()
            .zip(mask[1..(1 + pn_length as usize)].iter())
            .for_each(|(pn_byte, m)| *pn_byte ^= m);

        Ok(())
    }

    /// The "sample algorithm for applying header protection" (decrypt version)
    /// from RFC 9001 (QUIC-TLS)
    fn decrypt_fallible(&self, pn_offset: usize, packet: &mut [u8]) -> Result<(), ()> {
        let sample = Self::get_sample(pn_offset, packet)?;
        let mask = Self::header_protection(self.hp_key, sample);

        // First mask packet number length and preceding bits
        let packet_first_byte = packet.get_mut(0).ok_or(())?;
        if *packet_first_byte & 0x80 == 0x80 {
            // Long header: 4 bits masked
            *packet_first_byte ^= mask[0] & 0x0f;
        } else {
            // Short header: 5 bits masked
            *packet_first_byte ^= mask[0] & 0x1f;
        }

        // Now get unprotected packet number length
        let pn_length = (*packet_first_byte & 0x03).checked_add(1).ok_or(())?;

        // Obtain reference to protected packet number bytes
        let pn_end = pn_offset.checked_add(pn_length as usize).ok_or(())?;
        let pn_bytes = packet.get_mut(pn_offset..pn_end).ok_or(())?;

        // Mask the packet number
        pn_bytes.iter_mut()
            .zip(mask[1..(1 + pn_length as usize)].iter())
            .for_each(|(pn_byte, m)| *pn_byte ^= m);

        Ok(())
    }
}

impl HeaderKey for TransportHeaderKey {
    fn decrypt(&self, pn_offset: usize, packet: &mut [u8]) {
        if let Err(_) = self.decrypt_fallible(pn_offset, packet) {
            packet.fill(0);
        }
    }

    fn encrypt(&self, pn_offset: usize, packet: &mut [u8]) {
        if let Err(_) = self.encrypt_fallible(pn_offset, packet) {
            packet.fill(0);
        }
    }

    fn sample_size(&self) -> usize {
        16
    }
}

struct TransportPacketKey(ChaCha20Poly1305);

impl PacketKey for TransportPacketKey {
    fn encrypt(&self, packet: u64, buf: &mut [u8], header_len: usize) {
        let Some(payload_end_index) = buf.len().checked_sub(self.tag_len()) else {
            buf.fill(0);
            return;
        };

        // TODO: Change to split_at_mut_checked when that is stabilised
        if header_len > buf.len() {
            buf.fill(0);
            return;
        }
        let (header, payload_and_tag) = buf.split_at_mut(header_len);

        let mut fixed_buffer = FixedBuffer::new(payload_and_tag, payload_end_index);

        // Construct nonce as in the Noise specification, but using the packet
        // number for n instead of internal state n
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&packet.to_le_bytes());
        let nonce = nonce.into();

        if let Err(_) = self.0.encrypt_in_place(&nonce, header, &mut fixed_buffer) {
            buf.fill(0);
            return;
        }
    }

    fn decrypt(
        &self,
        packet: u64,
        header: &[u8],
        payload: &mut BytesMut,
    ) -> Result<(), CryptoError> {
        // Construct nonce as in the Noise specification, but using the packet
        // number for n instead of internal state n
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&packet.to_le_bytes());
        let nonce = nonce.into();

        self.0.decrypt_in_place(&nonce, header, payload).map_err(|_| CryptoError)
    }

    fn tag_len(&self) -> usize {
        <ChaCha20Poly1305 as AeadCore>::TagSize::to_usize()
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

struct FixedBuffer<'buf> {
    buffer: &'buf mut [u8],
    end: usize,
}

impl<'buf> FixedBuffer<'buf> {
    fn new(buffer: &'buf mut [u8], end: usize) -> Self {
        Self { buffer, end }
    }
}

impl AsRef<[u8]> for FixedBuffer<'_> {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[..self.end]
    }
}

impl AsMut<[u8]> for FixedBuffer<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buffer[..self.end]
    }
}

impl Buffer for FixedBuffer<'_> {
    fn extend_from_slice(&mut self, other: &[u8]) -> AeadResult<()> {
        let new_end = self.end.checked_add(other.len()).ok_or_else(|| AeadError)?;
        if new_end > self.buffer.len() {
            return Err(AeadError);
        }

        self.buffer[self.end..new_end].copy_from_slice(other);
        self.end = new_end;
        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        if len < self.end {
            self.end = len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_buffer_len_is_end() {
        let mut buffer = [0u8; 8];
        let fixed_buffer = FixedBuffer::new(&mut buffer, 6);

        assert_eq!(6, fixed_buffer.len());
    }

    #[test]
    fn fixed_buffer_is_empty_according_to_end() {
        let mut buffer = [0u8; 8];
        let fixed_buffer = FixedBuffer::new(&mut buffer, 0);

        assert!(fixed_buffer.is_empty());

        let fixed_buffer = FixedBuffer::new(&mut buffer, 2);

        assert!(!fixed_buffer.is_empty());
    }

    #[test]
    fn fixed_buffer_extend_from_slice_ok() {
        let mut buffer = [0u8; 8];
        let mut fixed_buffer = FixedBuffer::new(&mut buffer, 4);

        let extend = [1u8; 2];
        let result = fixed_buffer.extend_from_slice(&extend);

        assert!(result.is_ok());
        assert_eq!([0u8, 0u8, 0u8, 0u8, 1u8, 1u8], fixed_buffer.as_ref());
        assert_eq!([0u8, 0u8, 0u8, 0u8, 1u8, 1u8], fixed_buffer.as_mut());
        assert_eq!(6, fixed_buffer.len());
    }

    #[test]
    fn fixed_buffer_extend_from_slice_overflow_not_allowed() {
        let mut buffer = [0u8; 8];
        let mut fixed_buffer = FixedBuffer::new(&mut buffer, usize::MAX);

        let extend = [1u8; 2];
        let result = fixed_buffer.extend_from_slice(&extend);

        assert!(matches!(result, Err(AeadError)));
        assert_eq!([0u8; 8], fixed_buffer.buffer);
    }

    #[test]
    fn fixed_buffer_extend_from_slice_out_of_bounds_not_allowed() {
        let mut buffer = [0u8; 8];
        let mut fixed_buffer = FixedBuffer::new(&mut buffer, 7);

        let extend = [1u8; 2];
        let result = fixed_buffer.extend_from_slice(&extend);

        assert!(matches!(result, Err(AeadError)));
        assert_eq!([0u8; 7], fixed_buffer.as_ref());
        assert_eq!([0u8; 7], fixed_buffer.as_mut());
        assert_eq!(7, fixed_buffer.len());
    }

    #[test]
    fn fixed_buffer_truncate_ok() {
        let mut buffer = [0u8; 8];
        let mut fixed_buffer = FixedBuffer::new(&mut buffer, 6);

        fixed_buffer.truncate(4);

        assert_eq!([0u8; 4], fixed_buffer.as_ref());
        assert_eq!([0u8; 4], fixed_buffer.as_mut());
        assert_eq!(4, fixed_buffer.len());
    }

    #[test]
    fn fixed_buffer_truncate_does_nothing_if_greater_than_end() {
        let mut buffer = [0u8; 8];
        let mut fixed_buffer = FixedBuffer::new(&mut buffer, 6);

        fixed_buffer.truncate(7);

        assert_eq!([0u8; 6], fixed_buffer.as_ref());
        assert_eq!([0u8; 6], fixed_buffer.as_mut());
        assert_eq!(6, fixed_buffer.len());
    }
}
