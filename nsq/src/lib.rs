// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::net::UdpSocket;
use std::sync::Arc;

use quinn::{AsyncUdpSocket, ClientConfig, Endpoint, EndpointConfig, Runtime, ServerConfig};

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
        ).map(Self)
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
        ).map(Self)
    }

    pub fn endpoint(self) -> Endpoint {
        self.0
    }

    pub fn endpoint_as_ref(&self) -> &Endpoint {
        &self.0
    }
}

pub struct NsqClientConfig(ClientConfig);

impl NsqClientConfig {
    pub fn new<LS>(local_static_secret: LS) -> Self
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        let mut client_config = ClientConfig::new(Arc::new(NoiseConfig::new(local_static_secret)));
        client_config.version(NSQ_QUIC_VERSION);
        Self(client_config)
    }

    pub fn client_config(self) -> ClientConfig {
        self.0
    }

    pub fn client_config_as_ref(&self) -> &ClientConfig {
        &self.0
    }
}

pub struct NsqClient(Endpoint);

impl NsqClient {
    pub fn new_v4<LS>(local_static_secret: LS, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        Self::new_with_socket(local_static_secret, socket, runtime)
    }

    pub fn new_v6<LS>(local_static_secret: LS, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        let socket = UdpSocket::bind("[::]:0")?;
        Self::new_with_socket(local_static_secret, socket, runtime)
    }

    pub fn new_with_socket<LS>(local_static_secret: LS, socket: UdpSocket, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        let mut endpoint = Endpoint::new(
            NsqEndpointConfig::new().endpoint_config(),
            None,
            socket,
            runtime,
        )?;
        endpoint.set_default_client_config(NsqClientConfig::new(local_static_secret).client_config());
        Ok(Self(endpoint))
    }

    pub fn new_with_abstract_socket<LS>(local_static_secret: LS, socket: Arc<dyn AsyncUdpSocket>, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        let mut endpoint = Endpoint::new_with_abstract_socket(
            NsqEndpointConfig::new().endpoint_config(),
            None,
            socket,
            runtime,
        )?;
        endpoint.set_default_client_config(NsqClientConfig::new(local_static_secret).client_config());
        Ok(Self(endpoint))
    }

    pub fn endpoint(self) -> Endpoint {
        self.0
    }

    pub fn endpoint_as_ref(&self) -> &Endpoint {
        &self.0
    }
}
