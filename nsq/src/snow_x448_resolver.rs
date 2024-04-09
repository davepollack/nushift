// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use rand::rngs::OsRng;
use snow::{
    params::{CipherChoice, DHChoice, HashChoice},
    resolvers::CryptoResolver,
    types::{Cipher, Dh, Hash, Random},
    Error as SnowError,
};
use x448::{PublicKey, Secret};

pub(crate) struct SnowX448Resolver;

impl CryptoResolver for SnowX448Resolver {
    fn resolve_rng(&self) -> Option<Box<dyn Random>> {
        None
    }

    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<dyn Dh>> {
        match choice {
            DHChoice::Ed448 => Some(Box::new(SnowX448Keypair::new())),
            _ => None,
        }
    }

    fn resolve_hash(&self, _choice: &HashChoice) -> Option<Box<dyn Hash>> {
        None
    }

    fn resolve_cipher(&self, _choice: &CipherChoice) -> Option<Box<dyn Cipher>> {
        None
    }
}

pub(crate) struct SnowX448Keypair(Option<(PublicKey, Secret)>);

pub(crate) const TRAIT_METHODS_CALLED_INCORRECTLY: [u8; 56] = [1u8; 56];

impl SnowX448Keypair {
    fn new() -> Self {
        Self(None)
    }
}

impl Dh for SnowX448Keypair {
    fn name(&self) -> &'static str {
        "448"
    }

    fn pub_len(&self) -> usize {
        56
    }

    fn priv_len(&self) -> usize {
        56
    }

    fn set(&mut self, privkey: &[u8]) {
        // We need to store/regenerate the pubkey in the struct because of the
        // trait signature `fn pubkey(&self) -> &[u8]` which has to return a
        // reference to the current struct.
        self.0 = Secret::from_bytes(privkey)
            .and_then(|secret| Some((PublicKey::from(&secret), secret)));
    }

    fn generate(&mut self, _rng: &mut dyn Random) {
        // Ignore the passed-in rng due to type issues.
        let secret = Secret::new(&mut OsRng);
        // We need to store/regenerate the pubkey in the struct because of the
        // trait signature `fn pubkey(&self) -> &[u8]` which has to return a
        // reference to the current struct.
        self.0 = Some((PublicKey::from(&secret), secret));
    }

    fn pubkey(&self) -> &[u8] {
        match self.0 {
            Some((ref public_key, _)) => public_key.as_bytes(),
            // This is crap, IMO
            _ => &TRAIT_METHODS_CALLED_INCORRECTLY,
        }
    }

    fn privkey(&self) -> &[u8] {
        match self.0 {
            Some((_, ref secret)) => secret.as_bytes(),
            // This is crap, IMO
            _ => &TRAIT_METHODS_CALLED_INCORRECTLY,
        }
    }

    fn dh(&self, pubkey: &[u8], out: &mut [u8]) -> Result<(), SnowError> {
        let (_, our_secret) = self.0.as_ref().ok_or_else(|| SnowError::Dh)?;
        let their_pubkey = PublicKey::from_bytes(pubkey).ok_or_else(|| SnowError::Dh)?;

        let shared_secret = our_secret.as_diffie_hellman(&their_pubkey).ok_or_else(|| SnowError::Dh)?;
        let shared_secret_bytes = shared_secret.as_bytes();

        if shared_secret_bytes.len() != out.len() {
            Err(SnowError::Dh)
        } else {
            // Really wish there was a fallible version of copy_from_slice
            // instead of the len check happening twice
            out.copy_from_slice(shared_secret.as_bytes());
            Ok(())
        }
    }
}
