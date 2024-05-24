// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use chacha20poly1305::{aead::{AeadInPlace, KeyInit}, ChaCha20Poly1305};
use hkdf::{hmac::{Hmac, Mac}, Hkdf};
use quinn_proto::{
    crypto::{AeadKey, ClientConfig, CryptoError, HandshakeTokenKey, HmacKey, Keys, ServerConfig, Session, UnsupportedVersion},
    transport_parameters::TransportParameters,
    ConnectError,
    ConnectionId,
    Side,
};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use snow::{
    resolvers::{BoxedCryptoResolver, DefaultResolver, FallbackResolver},
    Builder,
    HandshakeState,
};

use crate::snow_x448_resolver::SnowX448Resolver;

use super::{fixed_buffer::FixedBuffer, session::NoiseSession, RETRY_KEY, RETRY_NONCE};

const NSQ_PROTOCOL_STRING: &str = "Noise_XXhfs_448+Kyber1024_ChaChaPoly_BLAKE2b";

fn nsq_resolver() -> BoxedCryptoResolver {
    Box::new(FallbackResolver::new(Box::new(DefaultResolver), Box::new(SnowX448Resolver)))
}

pub(crate) struct NoiseConfig<LS> {
    local_static_secret: LS,
}

impl<LS> NoiseConfig<LS>
where
    LS: AsRef<[u8]>,
{
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
    LS: AsRef<[u8]> + Send + Sync,
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
    LS: AsRef<[u8]> + Send + Sync,
{
    fn initial_keys(&self, version: u32, dst_cid: &ConnectionId) -> Result<Keys, UnsupportedVersion> {
        NoiseSession::initial_keys(version, dst_cid, Side::Server)
    }

    fn retry_tag(&self, _version: u32, orig_dst_cid: &ConnectionId, packet: &[u8]) -> [u8; 16] {
        // Generate retry tag using ChaCha20-Poly1305 as an AEAD, instead of
        // AES-128-GCM as in the RFC 9001 spec, but otherwise, follow the RFC
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

pub(crate) struct NoiseHandshakeTokenKey(Hkdf<Sha256>);

impl NoiseHandshakeTokenKey {
    /// Initialises a handshake token key from random bytes.
    pub(crate) fn new() -> Self {
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

pub(crate) struct NoiseHmacKey([u8; 64]);

impl NoiseHmacKey {
    /// Initialises a reset key from random bytes.
    pub(crate) fn new() -> Self {
        let mut key = [0u8; 64];
        OsRng.fill_bytes(&mut key);
        Self(key)
    }
}

impl HmacKey for NoiseHmacKey {
    fn sign(&self, data: &[u8], signature_out: &mut [u8]) {
        let mut mac = <Hmac<Sha256> as Mac>::new(&self.0.into());
        mac.update(data);
        signature_out.copy_from_slice(&mac.finalize().into_bytes());
    }

    fn signature_len(&self) -> usize {
        Sha256::output_size()
    }

    fn verify(&self, data: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
        let mut mac = <Hmac<Sha256> as Mac>::new(&self.0.into());
        mac.update(data);
        mac.verify_slice(signature).map_err(|_| CryptoError)
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use x448::Secret;

    use super::*;

    #[test]
    fn round_trip_ok() {
        let initiator_static_secret = Secret::new(&mut OsRng);
        let responder_static_secret = Secret::new(&mut OsRng);

        let mut initiator = NoiseConfig::new(initiator_static_secret.as_bytes()).new_initiator_handshake_state();
        let mut responder = NoiseConfig::new(responder_static_secret.as_bytes()).new_responder_handshake_state();

        let mut noise_message = vec![0u8; 4096];
        let mut payload = vec![0u8; 32];

        let message_len = initiator.write_message(&[], &mut noise_message).expect("Should write");
        let payload_len = responder.read_message(&noise_message[..message_len], &mut payload).expect("Should read");
        assert_eq!(0, payload_len);

        let message_len = responder.write_message(&[], &mut noise_message).expect("Should write");
        let payload_len = initiator.read_message(&noise_message[..message_len], &mut payload).expect("Should read");
        assert_eq!(0, payload_len);

        let message_len = initiator.write_message(&[], &mut noise_message).expect("Should write");
        let payload_len = responder.read_message(&noise_message[..message_len], &mut payload).expect("Should read");
        assert_eq!(0, payload_len);

        assert!(initiator.is_handshake_finished(), "Initiator state should be handshake finished");
        assert!(responder.is_handshake_finished(), "Responder state should be handshake finished");

        let mut initiator = initiator.into_transport_mode().expect("Should convert");
        let mut responder = responder.into_transport_mode().expect("Should convert");

        let message_len = initiator.write_message(b"hello", &mut noise_message).expect("Should write");
        let payload_len = responder.read_message(&noise_message[..message_len], &mut payload).expect("Should read");
        assert_eq!(b"hello", &payload[..payload_len]);
    }
}
