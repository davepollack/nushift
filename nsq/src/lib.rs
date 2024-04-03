// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use snow::resolvers::{BoxedCryptoResolver, DefaultResolver, FallbackResolver};
use x448_resolver::X448Resolver;

mod quinn_snow;
mod x448_resolver;

// TODO: Remove warning suppression when this is used by other parts of the lib code
#[allow(dead_code)]
fn nsq_resolver() -> BoxedCryptoResolver {
    Box::new(FallbackResolver::new(Box::new(DefaultResolver), Box::new(X448Resolver)))
}

// TODO: Remove warning suppression when this is used by other parts of the lib code
#[allow(dead_code)]
const NSQ_PROTOCOL_STRING: &str = "Noise_XXhfs_448+Kyber1024_ChaChaPoly_BLAKE2b";

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use snow::Builder;
    use x448::{PublicKey, Secret};

    use super::*;

    #[test]
    fn nsq_round_trip_ok() {
        let initiator_static_secret = Secret::new(&mut OsRng);
        let responder_static_secret = Secret::new(&mut OsRng);

        let mut initiator = Builder::with_resolver(NSQ_PROTOCOL_STRING.parse().expect("Should parse"), nsq_resolver())
            .local_private_key(initiator_static_secret.as_bytes())
            .remote_public_key(PublicKey::from(&responder_static_secret).as_bytes())
            .build_initiator()
            .expect("Should build");

        let mut responder = Builder::with_resolver(NSQ_PROTOCOL_STRING.parse().expect("Should parse"), nsq_resolver())
            .local_private_key(responder_static_secret.as_bytes())
            .remote_public_key(PublicKey::from(&initiator_static_secret).as_bytes())
            .build_responder()
            .expect("Should build");

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
