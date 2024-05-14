// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::{any::Any, sync::Arc};

use chacha20::{
    cipher::{typenum::U10, KeyIvInit, StreamCipherCore, StreamCipherSeekCore},
    ChaChaCore,
};
use chacha20poly1305::{
    aead::{bytes::BytesMut, generic_array::typenum::Unsigned, AeadInPlace, Buffer, Error as AeadError, KeyInit, Result as AeadResult},
    AeadCore,
    ChaCha20Poly1305,
};
use hex_literal::hex;
use hkdf::Hkdf;
use quinn_proto::{
    crypto::{AeadKey, ClientConfig, CryptoError, ExportKeyingMaterialError, HandshakeTokenKey, HeaderKey, KeyPair, Keys, PacketKey, ServerConfig, Session, UnsupportedVersion},
    transport_parameters::TransportParameters,
    ConnectError,
    ConnectionId,
    Side,
    TransportError,
    TransportErrorCode,
};
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use snow::{Builder, Error as SnowError, HandshakeState};

use crate::{nsq_resolver, NSQ_PROTOCOL_STRING};

// TODO: Ensure a wrapper function that creates a client endpoint config for the
// user, uses this version, and if appropriate, server configs too.
// TODO: Register range with QUIC WG.
const NSQ_QUIC_VERSION: u32 = 0x6e737100; // "nsq" + 0[0-f]

const RFC_9001_INITIAL_SALT: [u8; 20] = hex!("38762cf7f55934b34d179ae6a4c80cadccbb7f0a");
const CLIENT_INITIAL_INFO: &str = "client in";
const SERVER_INITIAL_INFO: &str = "server in";
const KEY_INFO: &str = "quic key";
const HP_KEY_INFO: &str = "quic hp";
const KEY_UPDATE_INFO: &str = "quic ku";

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

pub(crate) struct NoiseConfig<LS> {
    local_static_secret: LS,
}

impl<LS> NoiseConfig<LS>
where
    LS: AsRef<[u8]>,
{
    // TODO: Remove when this is used
    #[allow(dead_code)]
    pub(crate) fn new(local_static_secret: LS) -> Self {
        Self { local_static_secret }
    }

    fn new_initiator_handshake_state(&self) -> HandshakeState {
        Builder::with_resolver(NSQ_PROTOCOL_STRING.parse().expect("Protocol string should be valid"), nsq_resolver())
            .local_private_key(self.local_static_secret.as_ref())
            .build_initiator()
            .expect("Builder configuration should be valid")
    }

    fn new_responder_handshake_state(&self) -> HandshakeState {
        Builder::with_resolver(NSQ_PROTOCOL_STRING.parse().expect("Protocol string should be valid"), nsq_resolver())
            .local_private_key(self.local_static_secret.as_ref())
            .build_responder()
            .expect("Builder configuration should be valid")
    }
}

impl<LS> ClientConfig for NoiseConfig<LS>
where
    LS: Send + Sync + AsRef<[u8]>,
{
    fn start_session(
        self: Arc<Self>,
        _version: u32,
        _server_name: &str,
        params: &TransportParameters,
    ) -> Result<Box<dyn Session>, ConnectError> {
        Ok(Box::new(NoiseSession::new_handshaking(self.new_initiator_handshake_state(), *params)))
    }
}

impl<LS> ServerConfig for NoiseConfig<LS>
where
    LS: Send + Sync + AsRef<[u8]>,
{
    fn initial_keys(&self, version: u32, dst_cid: &ConnectionId, side: Side) -> Result<Keys, UnsupportedVersion> {
        NoiseSession::initial_keys(version, dst_cid, side)
    }

    fn retry_tag(&self, _version: u32, orig_dst_cid: &ConnectionId, packet: &[u8]) -> [u8; 16] {
        // Generate retry tag using ChaCha20-Poly1305 as an AEAD, instead of
        // AES-256-GCM as in the RFC 9001 spec, but otherwise, follow the RFC
        // 9001 spec. This needs to be paired with an implementation of
        // is_valid_retry that uses ChaCha20-Poly1305 to verify the tag.

        let mut retry_pseudo_packet = vec![];
        retry_pseudo_packet.push(orig_dst_cid.len().try_into().expect("retry_tag: ODCID len must be u8"));
        retry_pseudo_packet.extend_from_slice(orig_dst_cid);
        retry_pseudo_packet.extend_from_slice(packet);

        let cipher = ChaCha20Poly1305::new(&RETRY_KEY.into());
        cipher.encrypt_in_place_detached(&RETRY_NONCE.into(), &retry_pseudo_packet, &mut [])
            .expect("Should succeed, as the packet constructed by us (server) should not be pathologically long")
            .into()
    }

    fn start_session(self: Arc<Self>, _version: u32, params: &TransportParameters) -> Box<dyn Session> {
        Box::new(NoiseSession::new_handshaking(self.new_responder_handshake_state(), *params))
    }
}

enum NoiseSession {
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
enum LocalTransportParameters {
    Unsent(TransportParameters),
    Sent,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum RemoteTransportParameters {
    Received(TransportParameters),
    NotReceived,
}

struct CurrentSecrets {
    i_to_r_cipherstate_key: [u8; 32],
    r_to_i_cipherstate_key: [u8; 32],
}

impl NoiseSession {
    pub fn new_handshaking(handshake_state: HandshakeState, local_transport_parameters: TransportParameters) -> Self {
        Self::SnowHandshaking {
            handshake_state,
            local_transport_parameters: LocalTransportParameters::Unsent(local_transport_parameters),
            remote_transport_parameters: RemoteTransportParameters::NotReceived,
        }
    }

    fn initial_keys(version: u32, dst_cid: &ConnectionId, side: Side) -> Result<Keys, UnsupportedVersion> {
        if version != NSQ_QUIC_VERSION {
            return Err(UnsupportedVersion);
        }

        let hk = Hkdf::<Sha256>::new(Some(&RFC_9001_INITIAL_SALT), dst_cid);
        let mut client_initial_secret = [0u8; 32];
        let mut server_initial_secret = [0u8; 32];
        hk.expand(CLIENT_INITIAL_INFO.as_bytes(), &mut client_initial_secret).expect("Length 32 should be a valid output");
        hk.expand(SERVER_INITIAL_INFO.as_bytes(), &mut server_initial_secret).expect("Length 32 should be a valid output");

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
            _ => panic!("An internal error occurred when reading handshake"),
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
        // Even though at intermediate points in this handshake we have better
        // keys, we can't really get them from the Snow state without calling
        // dangerously_get_raw_split, which we shouldn't call until the end. So
        // continue to return None until then.

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
        }
    }

    fn next_1rtt_keys(&mut self) -> Option<KeyPair<Box<dyn PacketKey>>> {
        let Self::Transport { current_secrets, is_initiator, .. } = self else { return None; };

        let mut new_i_to_r_cipherstate_key = [0u8; 32];
        let mut new_r_to_i_cipherstate_key = [0u8; 32];
        let hk_i_to_r = Hkdf::<Sha256>::from_prk(&current_secrets.i_to_r_cipherstate_key).expect("Should be a valid PRK length");
        let hk_r_to_i = Hkdf::<Sha256>::from_prk(&current_secrets.r_to_i_cipherstate_key).expect("Should be a valid PRK length");
        hk_i_to_r.expand(KEY_UPDATE_INFO.as_bytes(), &mut new_i_to_r_cipherstate_key).expect("Length 32 should be a valid output");
        hk_r_to_i.expand(KEY_UPDATE_INFO.as_bytes(), &mut new_r_to_i_cipherstate_key).expect("Length 32 should be a valid output");

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
        // AES-256-GCM as in the RFC 9001 spec, but otherwise, follow the RFC
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
        hk.expand(HP_KEY_INFO.as_bytes(), &mut hp_key).expect("Length 32 should be a valid output");

        Self(hp_key)
    }

    fn from_cs_key(cs_key: [u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::from_prk(&cs_key).expect("Should be a valid PRK length");

        let mut hp_key = [0u8; 32];
        hk.expand(HP_KEY_INFO.as_bytes(), &mut hp_key).expect("Length 32 should be a valid output");

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
        hk.expand(KEY_INFO.as_bytes(), &mut key).expect("Length 32 should be a valid output");

        Self(ChaCha20Poly1305::new(&key.into()))
    }

    fn from_cs_key(cs_key: [u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::from_prk(&cs_key).expect("Should be a valid PRK length");

        let mut key = [0u8; 32];
        hk.expand(KEY_INFO.as_bytes(), &mut key).expect("Length 32 should be a valid output");

        Self(ChaCha20Poly1305::new(&key.into()))
    }

    fn encrypt_fallible(&self, packet: u64, buf: &mut [u8], header_len: usize) -> Result<(), ()> {
        let payload_end_index = buf.len().checked_sub(self.tag_len()).ok_or(())?;

        // TODO: Change to split_at_mut_checked when that is stabilised
        if header_len > buf.len() {
            return Err(());
        }
        let (header, payload_and_tag) = buf.split_at_mut(header_len);

        let mut fixed_buffer = FixedBuffer::new(payload_and_tag, payload_end_index);

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

struct NoiseHandshakeTokenKey(Hkdf<Sha256>);

impl NoiseHandshakeTokenKey {
    /// Initialises a handshake token key from random bytes.
    // TODO: Remove when this is used
    #[allow(dead_code)]
    fn new() -> Self {
        let mut secret = [0u8; 64];
        OsRng.fill_bytes(&mut secret);
        Self(Hkdf::<Sha256>::new(None, &secret))
    }
}

impl HandshakeTokenKey for NoiseHandshakeTokenKey {
    fn aead_from_hkdf(&self, random_bytes: &[u8]) -> Box<dyn AeadKey> {
        let mut key = [0u8; 32];
        self.0.expand(random_bytes, &mut key).expect("Length 32 should be a valid output");
        Box::new(NoiseHandshakeTokenAeadKey(ChaCha20Poly1305::new(&key.into())))
    }
}

struct NoiseHandshakeTokenAeadKey(ChaCha20Poly1305);

impl AeadKey for NoiseHandshakeTokenAeadKey {
    fn seal(&self, data: &mut Vec<u8>, additional_data: &[u8]) -> Result<(), CryptoError> {
        self.0.encrypt_in_place(&[0u8; 12].into(), additional_data, data).map_err(|_| CryptoError)
    }

    fn open<'a>(&self, data: &'a mut [u8], additional_data: &[u8]) -> Result<&'a mut [u8], CryptoError> {
        let mut fixed_buffer = FixedBuffer::new(data, data.len());
        self.0.decrypt_in_place(&[0u8; 12].into(), additional_data, &mut fixed_buffer)
            .map(|_| fixed_buffer.into_mut_slice())
            .map_err(|_| CryptoError)
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

    fn into_mut_slice(self) -> &'buf mut [u8] {
        &mut self.buffer[..self.end]
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
