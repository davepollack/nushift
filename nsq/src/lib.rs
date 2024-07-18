// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::any::Any;
use std::future::Future;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;

use quinn::{AsyncUdpSocket, ClientConfig, ConnectError, Connection, ConnectionError, Endpoint, EndpointConfig, Runtime, ServerConfig, VarInt};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

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
        Endpoint::new(
            NsqEndpointConfig::new().endpoint_config(),
            None,
            socket,
            runtime,
        ).map(|mut endpoint| {
            endpoint.set_default_client_config(NsqClientConfig::new(local_static_secret).client_config());
            Self(endpoint)
        })
    }

    pub fn new_with_abstract_socket<LS>(local_static_secret: LS, socket: Arc<dyn AsyncUdpSocket>, runtime: Arc<dyn Runtime>) -> io::Result<Self>
    where
        LS: AsRef<[u8]> + Send + Sync + 'static,
    {
        Endpoint::new_with_abstract_socket(
            NsqEndpointConfig::new().endpoint_config(),
            None,
            socket,
            runtime,
        ).map(|mut endpoint| {
            endpoint.set_default_client_config(NsqClientConfig::new(local_static_secret).client_config());
            Self(endpoint)
        })
    }

    pub fn endpoint(self) -> Endpoint {
        self.0
    }

    pub fn endpoint_as_ref(&self) -> &Endpoint {
        &self.0
    }

    pub async fn connect_with_tofu<TS: TofuStore>(&self, mut tofu_store: TS, addr: SocketAddr, server_name: &str) -> Result<Connection, ConnectWithTofuError> {
        let (connection, remote_static_key) = self.connect_and_get_key(addr, server_name).await?;

        let is_trusted_key_result = tofu_store.is_trusted_key(addr, server_name, &remote_static_key)
            .await
            .map_err(|err| Box::new(err) as Box<dyn Any>);

        Self::check_result_and_maybe_close_connection(is_trusted_key_result, connection, remote_static_key)
    }

    pub async fn connect_with_local_tofu<TS: LocalTofuStore>(&self, mut local_tofu_store: TS, addr: SocketAddr, server_name: &str) -> Result<Connection, ConnectWithTofuError> {
        let (connection, remote_static_key) = self.connect_and_get_key(addr, server_name).await?;

        let is_trusted_key_result = local_tofu_store.is_trusted_key(addr, server_name, &remote_static_key)
            .await
            .map_err(|err| Box::new(err) as Box<dyn Any>);

        Self::check_result_and_maybe_close_connection(is_trusted_key_result, connection, remote_static_key)
    }

    async fn connect_and_get_key(&self, addr: SocketAddr, server_name: &str) -> Result<(Connection, Vec<u8>), ConnectWithTofuError> {
        let connection = self.0.connect(addr, server_name)
            .context(ConnectSnafu)?
            .await
            .context(ConnectionSnafu)?;

        let remote_static_key = connection.peer_identity()
            .expect("Remote static key must exist after the handshake, otherwise a TransportError would have occurred before this")
            .downcast::<Vec<u8>>()
            .expect("Our Session peer_identity implementation returns a Vec<u8>");

        Ok((connection, *remote_static_key))
    }

    fn check_result_and_maybe_close_connection(is_trusted_key_result: Result<bool, Box<dyn Any>>, connection: Connection, remote_static_key: Vec<u8>) -> Result<Connection, ConnectWithTofuError> {
        match is_trusted_key_result {
            Ok(false) => {
                // Close the connection for the reason of untrusted key
                const APPLICATION_UNTRUSTED_KEY: VarInt = VarInt::from_u32(1u32);
                const APPLICATION_UNTRUSTED_KEY_REASON: &[u8] = b"Untrusted key";
                connection.close(APPLICATION_UNTRUSTED_KEY, APPLICATION_UNTRUSTED_KEY_REASON);

                Err(ConnectWithTofuError::UntrustedKey { remote_static_key })
            }

            Err(is_trusted_key_error) => {
                // Close the connection for the reason of internal error
                const APPLICATION_INTERNAL_ERROR_CHECKING_KEY: VarInt = VarInt::from_u32(2u32);
                const APPLICATION_INTERNAL_ERROR_CHECKING_KEY_REASON: &[u8] = b"Internal error checking key";
                connection.close(APPLICATION_INTERNAL_ERROR_CHECKING_KEY, APPLICATION_INTERNAL_ERROR_CHECKING_KEY_REASON);

                Err(ConnectWithTofuError::IsTrustedKeyError { error: is_trusted_key_error })
            }

            Ok(true) => Ok(connection),
        }
    }
}

#[trait_variant::make(TofuStore: Send)]
pub trait LocalTofuStore {
    type IsTrustedKeyError: Any;

    fn is_trusted_key(
        &mut self,
        addr: SocketAddr,
        server_name: &str,
        remote_static_key: &[u8],
    ) -> impl Future<Output = Result<bool, Self::IsTrustedKeyError>>;
}

#[derive(Snafu, SnafuCliDebug)]
pub enum ConnectWithTofuError {
    #[snafu(display("Pre-connect error: {source}"))]
    ConnectError { source: ConnectError },
    #[snafu(display("Connect error: {source}"))]
    ConnectionError { source: ConnectionError },
    #[snafu(display("Untrusted key: {remote_static_key:#x?}"))]
    UntrustedKey {
        remote_static_key: Vec<u8>,
    },
    #[snafu(display("An internal error occurred when running `is_trusted_key`"))]
    IsTrustedKeyError { error: Box<dyn Any> },
}
