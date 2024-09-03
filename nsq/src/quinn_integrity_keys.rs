// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use chacha20poly1305::{aead::{AeadInPlace, KeyInit}, ChaCha20Poly1305};
use hkdf::{hmac::{Hmac, Mac}, Hkdf};
use quinn_proto::crypto::{AeadKey, CryptoError, HandshakeTokenKey, HmacKey};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::fixed_buffer::FixedBuffer;

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
