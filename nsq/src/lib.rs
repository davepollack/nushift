// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::net::UdpSocket;
use std::sync::Arc;

use quinn::{AsyncUdpSocket, Endpoint, EndpointConfig, Runtime, ServerConfig};
use snow::resolvers::{BoxedCryptoResolver, DefaultResolver, FallbackResolver};
use snow_x448_resolver::SnowX448Resolver;

use crate::quinn_noise::config::{NoiseConfig, NoiseHandshakeTokenKey, NoiseHmacKey};
use crate::quinn_noise::NSQ_QUIC_VERSION;

mod quinn_noise;
mod snow_x448_resolver;

pub use quinn;

pub struct NsqEndpointConfig(EndpointConfig);

impl NsqEndpointConfig {
    pub fn new() -> Self {
        let mut endpoint_config = EndpointConfig::new(Arc::new(NoiseHmacKey::new()));
        endpoint_config.supported_versions(vec![NSQ_QUIC_VERSION]);
        Self(endpoint_config)
    }

    pub fn endpoint_config(self) -> EndpointConfig {
        self.0
    }

    pub fn endpoint_config_as_ref(&self) -> &EndpointConfig {
        &self.0
    }
}

pub struct NsqServerConfig(ServerConfig);

impl NsqServerConfig {
    pub fn new<LS>(local_static_secret: LS) -> Self
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        Self(
            ServerConfig::new(
                Arc::new(NoiseConfig::new(local_static_secret)),
                Arc::new(NoiseHandshakeTokenKey::new()),
            )
        )
    }

    pub fn server_config(self) -> ServerConfig {
        self.0
    }

    pub fn server_config_as_ref(&self) -> &ServerConfig {
        &self.0
    }
}

pub struct NsqServer(Endpoint);

impl NsqServer {
    pub fn new_v4<LS>(local_static_secret: LS, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        let socket = UdpSocket::bind("0.0.0.0:45777")?;
        Self::new_with_socket(local_static_secret, socket, runtime)
    }

    pub fn new_v6<LS>(local_static_secret: LS, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        let socket = UdpSocket::bind("[::]:45777")?;
        Self::new_with_socket(local_static_secret, socket, runtime)
    }

    pub fn new_with_socket<LS>(local_static_secret: LS, socket: UdpSocket, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        Endpoint::new(
            NsqEndpointConfig::new().endpoint_config(),
            Some(NsqServerConfig::new(local_static_secret).server_config()),
            socket,
            runtime,
        ).map(|endpoint| Self(endpoint))
    }

    pub fn new_with_abstract_socket<LS>(local_static_secret: LS, socket: Arc<dyn AsyncUdpSocket>, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        Endpoint::new_with_abstract_socket(
            NsqEndpointConfig::new().endpoint_config(),
            Some(NsqServerConfig::new(local_static_secret).server_config()),
            socket,
            runtime,
        ).map(|endpoint| Self(endpoint))
    }

    pub fn endpoint(self) -> Endpoint {
        self.0
    }

    pub fn endpoint_as_ref(&self) -> &Endpoint {
        &self.0
    }
}

// TODO: Move to config.rs?
fn nsq_resolver() -> BoxedCryptoResolver {
    Box::new(FallbackResolver::new(Box::new(DefaultResolver), Box::new(SnowX448Resolver)))
}

const NSQ_PROTOCOL_STRING: &str = "Noise_XXhfs_448+Kyber1024_ChaChaPoly_BLAKE2b";

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use snow::Builder;
    use x448::Secret;

    use super::*;

    #[test]
    fn nsq_round_trip_ok() {
        let initiator_static_secret = Secret::new(&mut OsRng);
        let responder_static_secret = Secret::new(&mut OsRng);

        let mut initiator = Builder::with_resolver(NSQ_PROTOCOL_STRING.parse().expect("Should parse"), nsq_resolver())
            .local_private_key(initiator_static_secret.as_bytes())
            .build_initiator()
            .expect("Should build");

        let mut responder = Builder::with_resolver(NSQ_PROTOCOL_STRING.parse().expect("Should parse"), nsq_resolver())
            .local_private_key(responder_static_secret.as_bytes())
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
