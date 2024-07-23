// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::any::Any;

use chacha20::{
    cipher::{typenum::U10, KeyIvInit, StreamCipherCore, StreamCipherSeekCore},
    ChaChaCore,
};
use chacha20poly1305::{
    aead::{bytes::BytesMut, generic_array::typenum::Unsigned, AeadInPlace, KeyInit},
    AeadCore,
    ChaCha20Poly1305,
};
use hex_literal::hex;
use hkdf::Hkdf;
use quinn_proto::{
    crypto::{CryptoError, ExportKeyingMaterialError, HeaderKey, KeyPair, Keys, PacketKey, Session, UnsupportedVersion},
    transport_parameters::TransportParameters,
    ConnectionId,
    Side,
    TransportError,
    TransportErrorCode,
};
use sha2::Sha256;
use snow::{Error as SnowError, HandshakeState};

use super::{fixed_buffer::FixedBuffer, NSQ_QUIC_VERSION, RETRY_KEY, RETRY_NONCE};

const RFC_9001_INITIAL_SALT: [u8; 20] = hex!("38762cf7f55934b34d179ae6a4c80cadccbb7f0a");
const CLIENT_INITIAL_INFO: &[u8] = b"client in";
const SERVER_INITIAL_INFO: &[u8] = b"server in";
const KEY_INFO: &[u8] = b"quic key";
const HP_KEY_INFO: &[u8] = b"quic hp";
const KEY_UPDATE_INFO: &[u8] = b"quic ku";

pub(crate) enum NoiseSession {
    SnowHandshaking {
        handshake_state: HandshakeState,
        local_transport_parameters: LocalTransportParameters,
        remote_transport_parameters: RemoteTransportParameters,
    },
    Transport {
        remote_static_key: Option<Vec<u8>>,
        remote_transport_parameters: RemoteTransportParameters,
        current_secrets: CurrentSecrets,
        is_initiator: bool,
    },
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum LocalTransportParameters {
    Unsent(TransportParameters),
    Sent,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum RemoteTransportParameters {
    Received(TransportParameters),
    NotReceived,
}

pub(crate) struct CurrentSecrets {
    i_to_r_cipherstate_key: [u8; 32],
    r_to_i_cipherstate_key: [u8; 32],
}

impl NoiseSession {
    pub(crate) fn new_handshaking(handshake_state: HandshakeState, local_transport_parameters: TransportParameters) -> Self {
        Self::SnowHandshaking {
            handshake_state,
            local_transport_parameters: LocalTransportParameters::Unsent(local_transport_parameters),
            remote_transport_parameters: RemoteTransportParameters::NotReceived,
        }
    }

    pub(crate) fn initial_keys(version: u32, dst_cid: &ConnectionId, side: Side) -> Result<Keys, UnsupportedVersion> {
        if version != NSQ_QUIC_VERSION {
            return Err(UnsupportedVersion);
        }

        let hk = Hkdf::<Sha256>::new(Some(&RFC_9001_INITIAL_SALT), dst_cid);
        let mut client_initial_secret = [0u8; 32];
        let mut server_initial_secret = [0u8; 32];
        hk.expand(CLIENT_INITIAL_INFO, &mut client_initial_secret).expect("Length 32 should be a valid output");
        hk.expand(SERVER_INITIAL_INFO, &mut server_initial_secret).expect("Length 32 should be a valid output");

        let hk_client_keys = Hkdf::<Sha256>::from_prk(&client_initial_secret).expect("Should be a valid PRK length");
        let hk_server_keys = Hkdf::<Sha256>::from_prk(&server_initial_secret).expect("Should be a valid PRK length");

        match side {
            Side::Client => Ok(
                Keys {
                    header: KeyPair {
                        local: Box::new(TransportHeaderKey::from_initial_prk(&hk_client_keys)),
                        remote: Box::new(TransportHeaderKey::from_initial_prk(&hk_server_keys)),
                    },
                    packet: KeyPair {
                        local: Box::new(TransportPacketKey::from_initial_prk(&hk_client_keys)),
                        remote: Box::new(TransportPacketKey::from_initial_prk(&hk_server_keys)),
                    },
                }
            ),

            Side::Server => Ok(
                Keys {
                    header: KeyPair {
                        local: Box::new(TransportHeaderKey::from_initial_prk(&hk_server_keys)),
                        remote: Box::new(TransportHeaderKey::from_initial_prk(&hk_client_keys)),
                    },
                    packet: KeyPair {
                        local: Box::new(TransportPacketKey::from_initial_prk(&hk_server_keys)),
                        remote: Box::new(TransportPacketKey::from_initial_prk(&hk_client_keys)),
                    },
                }
            ),
        }
    }
}

impl Session for NoiseSession {
    fn initial_keys(&self, dst_cid: &ConnectionId, side: Side) -> Keys {
        Self::initial_keys(NSQ_QUIC_VERSION, dst_cid, side).expect("Version should be supported")
    }

    fn handshake_data(&self) -> Option<Box<dyn Any>> {
        match self {
            Self::SnowHandshaking { handshake_state, .. } if handshake_state.is_handshake_finished() => Some(Box::new(())),
            Self::Transport { .. } => Some(Box::new(())),
            _ => None,
        }
    }

    fn peer_identity(&self) -> Option<Box<dyn Any>> {
        match self {
            Self::SnowHandshaking { handshake_state, .. } => handshake_state.get_remote_static().map(|slice| Box::new(slice.to_vec()) as Box<dyn Any>),
            Self::Transport { remote_static_key, .. } => remote_static_key.as_ref().map(|vec| Box::new(vec.clone()) as Box<dyn Any>),
        }
    }

    fn early_crypto(&self) -> Option<(Box<dyn HeaderKey>, Box<dyn PacketKey>)> {
        // TODO: Implement 0-RTT?
        None
    }

    fn early_data_accepted(&self) -> Option<bool> {
        // TODO: Implement 0-RTT?
        None
    }

    fn is_handshaking(&self) -> bool {
        matches!(self, Self::SnowHandshaking { .. })
    }

    fn read_handshake(&mut self, buf: &[u8]) -> Result<bool, TransportError> {
        let Self::SnowHandshaking { handshake_state, remote_transport_parameters, .. } = self else { panic!("Expected to be handshaking when reading handshake"); };

        // If the remote transport parameters are not received, then this is the
        // first call of read_handshake, and receive them. Otherwise, expect an
        // empty payload.
        let mut payload = if let RemoteTransportParameters::NotReceived = remote_transport_parameters {
            const MAX_TP_PAYLOAD_MSG_LEN: usize = 1024;
            vec![0u8; MAX_TP_PAYLOAD_MSG_LEN]
        } else {
            vec![]
        };

        // If our expected `payload` length cannot contain the decrypted payload, `SnowError::Decrypt` will happen.
        let payload_len = handshake_state.read_message(buf, &mut payload).map_err(|snow_error| match snow_error {
            SnowError::Decrypt => TransportError { code: TransportErrorCode::PROTOCOL_VIOLATION, frame: None, reason: "Snow decryption failed".into() },
            other_err => panic!("An internal error occurred when reading handshake: {other_err:?}"),
        })?;

        // If expecting to receive the transport parameters, read them.
        if let RemoteTransportParameters::NotReceived = remote_transport_parameters {
            let side = if handshake_state.is_initiator() { Side::Client } else { Side::Server };

            *remote_transport_parameters = RemoteTransportParameters::Received(
                TransportParameters::read(side, &mut &payload[..payload_len])
                    .map_err(|tp_error| TransportError {
                        code: TransportErrorCode::TRANSPORT_PARAMETER_ERROR,
                        frame: None,
                        reason: format!("Error while receiving transport parameters: {}", tp_error),
                    })?
            );
        }

        // The handshake is possibly finished at this point. write_handshake is
        // always called after read_handshake, and that needs to return the new
        // keys in this case. We let that method handle the case where
        // read_handshake finished the handshake, rather than storing some extra
        // state here for that method to interpret.

        // If the handshake is finished, notify quinn that the handshake data is
        // ready. We're meant to return Ok(false) after we initially return
        // Ok(true) here, but read_handshake should never be called again after
        // this.
        Ok(handshake_state.is_handshake_finished())
    }

    fn transport_parameters(&self) -> Result<Option<TransportParameters>, TransportError> {
        match self {
            Self::Transport {
                remote_transport_parameters: RemoteTransportParameters::Received(remote_transport_parameters),
                ..
            } => Ok(Some(*remote_transport_parameters)),
            _ => Ok(None),
        }
    }

    fn write_handshake(&mut self, buf: &mut Vec<u8>) -> Option<Keys> {
        let Self::SnowHandshaking { handshake_state, local_transport_parameters, remote_transport_parameters } = self else { panic!("Expected to be handshaking when writing handshake"); };

        // If the read_handshake that occurred right before this write_handshake
        // caused the handshake to be finished, then detect that here and return
        // (and *don't* call Snow write_message which would fail).
        if let Some((current_secrets, keys)) = if_handshake_finished_then_get_keys(handshake_state) {
            *self = Self::Transport {
                remote_static_key: handshake_state.get_remote_static().map(|slice| slice.into()),
                remote_transport_parameters: *remote_transport_parameters,
                current_secrets,
                is_initiator: handshake_state.is_initiator(),
            };
            return Some(keys);
        }

        // quinn-proto calls write_handshake again after we just wrote to see if
        // we have any more to write, but we don't, so return without filling
        // `buf` which will cause it to stop calling us.
        if !handshake_state.is_my_turn() {
            return None;
        }

        const MAX_HANDSHAKE_MSG_LEN: usize = 4096;
        let mut handshake_msg_buffer = [0u8; MAX_HANDSHAKE_MSG_LEN];

        // If the local transport parameters have not been sent yet, that means
        // this is the first message in the handshake, and send them. Otherwise,
        // use an empty payload.
        let payload = if let LocalTransportParameters::Unsent(unwrapped_local_transport_parameters) = local_transport_parameters {
            let mut payload = vec![];
            unwrapped_local_transport_parameters.write(&mut payload);

            *local_transport_parameters = LocalTransportParameters::Sent;

            payload
        } else {
            vec![]
        };

        let message_len = handshake_state.write_message(&payload, &mut handshake_msg_buffer).expect("Snow state machine unexpectedly errored when writing handshake");
        buf.extend_from_slice(&handshake_msg_buffer[..message_len]);

        // Now check again whether the handshake is finished.
        if let Some((current_secrets, keys)) = if_handshake_finished_then_get_keys(handshake_state) {
            *self = Self::Transport {
                remote_static_key: handshake_state.get_remote_static().map(|slice| slice.into()),
                remote_transport_parameters: *remote_transport_parameters,
                current_secrets,
                is_initiator: handshake_state.is_initiator(),
            };
            Some(keys)
        } else {
            None
            // TODO
            // Even though at intermediate points in this handshake we have
            // better keys, we can't really get them from the Snow state without
            // calling dangerously_get_raw_split, which we shouldn't call until
            // the end. So return the initial keys to quinn-proto again (which
            // expects us to transition into a handshake keys state).
            // TODO: Some(...)
        }
    }

    fn next_1rtt_keys(&mut self) -> Option<KeyPair<Box<dyn PacketKey>>> {
        let Self::Transport { current_secrets, is_initiator, .. } = self else { return None; };

        let mut new_i_to_r_cipherstate_key = [0u8; 32];
        let mut new_r_to_i_cipherstate_key = [0u8; 32];
        let hk_i_to_r = Hkdf::<Sha256>::from_prk(&current_secrets.i_to_r_cipherstate_key).expect("Should be a valid PRK length");
        let hk_r_to_i = Hkdf::<Sha256>::from_prk(&current_secrets.r_to_i_cipherstate_key).expect("Should be a valid PRK length");
        hk_i_to_r.expand(KEY_UPDATE_INFO, &mut new_i_to_r_cipherstate_key).expect("Length 32 should be a valid output");
        hk_r_to_i.expand(KEY_UPDATE_INFO, &mut new_r_to_i_cipherstate_key).expect("Length 32 should be a valid output");

        *current_secrets = CurrentSecrets {
            i_to_r_cipherstate_key: new_i_to_r_cipherstate_key,
            r_to_i_cipherstate_key: new_r_to_i_cipherstate_key,
        };

        if *is_initiator {
            Some(
                KeyPair {
                    local: Box::new(TransportPacketKey::from_cs_key(new_i_to_r_cipherstate_key)),
                    remote: Box::new(TransportPacketKey::from_cs_key(new_r_to_i_cipherstate_key)),
                }
            )
        } else {
            Some(
                KeyPair {
                    local: Box::new(TransportPacketKey::from_cs_key(new_r_to_i_cipherstate_key)),
                    remote: Box::new(TransportPacketKey::from_cs_key(new_i_to_r_cipherstate_key)),
                }
            )
        }
    }

    fn is_valid_retry(&self, orig_dst_cid: &ConnectionId, header: &[u8], payload: &[u8]) -> bool {
        // Verify retry integrity using ChaCha20-Poly1305 as an AEAD, instead of
        // AES-128-GCM as in the RFC 9001 spec, but otherwise, follow the RFC
        // 9001 spec. This needs to be paired with an implementation of
        // ServerConfig that uses ChaCha20-Poly1305 to generate the tag.

        let Some((retry_token, retry_tag)) = payload.split_last_chunk() else {
            return false;
        };

        let mut retry_pseudo_packet = vec![];
        retry_pseudo_packet.push(orig_dst_cid.len().try_into().expect("is_valid_retry: ODCID len must be u8"));
        retry_pseudo_packet.extend_from_slice(orig_dst_cid);
        retry_pseudo_packet.extend_from_slice(header);
        retry_pseudo_packet.extend_from_slice(retry_token);

        let cipher = ChaCha20Poly1305::new(&RETRY_KEY.into());
        cipher.decrypt_in_place_detached(&RETRY_NONCE.into(), &retry_pseudo_packet, &mut [], retry_tag.into())
            .is_ok()
    }

    fn export_keying_material(
        &self,
        output: &mut [u8],
        label: &[u8],
        context: &[u8],
    ) -> Result<(), ExportKeyingMaterialError> {
        // We need to have finished the handshake to export keying material.
        // Even though `ExportKeyingMaterialError` is meant to be used for
        // output length too large, quinn itself, for TLS, also uses it for the
        // case of not finished the handshake.
        let Self::Transport { current_secrets, .. } = self else { return Err(ExportKeyingMaterialError); };

        let ikm = [current_secrets.i_to_r_cipherstate_key, current_secrets.r_to_i_cipherstate_key].concat();

        let hk = Hkdf::<Sha256>::new(None, &ikm);
        hk.expand_multi_info(&[label, context], output).map_err(|_| ExportKeyingMaterialError)
    }
}

fn if_handshake_finished_then_get_keys(handshake_state: &mut HandshakeState) -> Option<(CurrentSecrets, Keys)> {
    if handshake_state.is_handshake_finished() {
        let (i_to_r_cipherstate_key, r_to_i_cipherstate_key) = handshake_state.dangerously_get_raw_split();

        if handshake_state.is_initiator() {
            Some((
                CurrentSecrets { i_to_r_cipherstate_key, r_to_i_cipherstate_key },
                Keys {
                    header: KeyPair {
                        local: Box::new(TransportHeaderKey::from_cs_key(i_to_r_cipherstate_key)),
                        remote: Box::new(TransportHeaderKey::from_cs_key(r_to_i_cipherstate_key)),
                    },
                    packet: KeyPair {
                        local: Box::new(TransportPacketKey::from_cs_key(i_to_r_cipherstate_key)),
                        remote: Box::new(TransportPacketKey::from_cs_key(r_to_i_cipherstate_key)),
                    },
                },
            ))
        } else {
            Some((
                CurrentSecrets { i_to_r_cipherstate_key, r_to_i_cipherstate_key },
                Keys {
                    header: KeyPair {
                        local: Box::new(TransportHeaderKey::from_cs_key(r_to_i_cipherstate_key)),
                        remote: Box::new(TransportHeaderKey::from_cs_key(i_to_r_cipherstate_key)),
                    },
                    packet: KeyPair {
                        local: Box::new(TransportPacketKey::from_cs_key(r_to_i_cipherstate_key)),
                        remote: Box::new(TransportPacketKey::from_cs_key(i_to_r_cipherstate_key)),
                    },
                },
            ))
        }
    } else {
        None
    }
}

struct TransportHeaderKey([u8; 32]);

impl TransportHeaderKey {
    fn from_initial_prk(hk: &Hkdf<Sha256>) -> Self {
        let mut hp_key = [0u8; 32];
        hk.expand(HP_KEY_INFO, &mut hp_key).expect("Length 32 should be a valid output");

        Self(hp_key)
    }

    fn from_cs_key(cs_key: [u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::from_prk(&cs_key).expect("Should be a valid PRK length");

        let mut hp_key = [0u8; 32];
        hk.expand(HP_KEY_INFO, &mut hp_key).expect("Length 32 should be a valid output");

        Self(hp_key)
    }

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
        packet.get(sample_offset..sample_end).ok_or(())?.try_into().map_err(|_| ())
    }

    /// The "sample algorithm for applying header protection" from RFC 9001
    /// (QUIC-TLS)
    fn encrypt_fallible(&self, pn_offset: usize, packet: &mut [u8]) -> Result<(), ()> {
        let sample = Self::get_sample(pn_offset, packet)?;
        let mask = Self::header_protection(self.0, sample);

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
        let mask = Self::header_protection(self.0, sample);

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
        if self.decrypt_fallible(pn_offset, packet).is_err() {
            packet.fill(0);
        }
    }

    fn encrypt(&self, pn_offset: usize, packet: &mut [u8]) {
        if self.encrypt_fallible(pn_offset, packet).is_err() {
            packet.fill(0);
        }
    }

    fn sample_size(&self) -> usize {
        16
    }
}

struct TransportPacketKey(ChaCha20Poly1305);

impl TransportPacketKey {
    fn from_initial_prk(hk: &Hkdf<Sha256>) -> Self {
        let mut key = [0u8; 32];
        hk.expand(KEY_INFO, &mut key).expect("Length 32 should be a valid output");

        Self(ChaCha20Poly1305::new(&key.into()))
    }

    fn from_cs_key(cs_key: [u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::from_prk(&cs_key).expect("Should be a valid PRK length");

        let mut key = [0u8; 32];
        hk.expand(KEY_INFO, &mut key).expect("Length 32 should be a valid output");

        Self(ChaCha20Poly1305::new(&key.into()))
    }

    fn encrypt_fallible(&self, packet: u64, buf: &mut [u8], header_len: usize) -> Result<(), ()> {
        let payload_len = buf.len()
            .checked_sub(self.tag_len())
            .and_then(|num| num.checked_sub(header_len))
            .ok_or(())?;

        let (header, payload_and_tag) = buf.split_at_mut(header_len);

        let mut fixed_buffer = FixedBuffer::new(payload_and_tag, payload_len);

        // Construct nonce as in the Noise specification, but using the packet
        // number for n instead of internal state n
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&packet.to_le_bytes());
        let nonce = nonce.into();

        self.0.encrypt_in_place(&nonce, header, &mut fixed_buffer).map_err(|_| ())
    }
}

impl PacketKey for TransportPacketKey {
    fn encrypt(&self, packet: u64, buf: &mut [u8], header_len: usize) {
        if self.encrypt_fallible(packet, buf, header_len).is_err() {
            buf.fill(0);
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
