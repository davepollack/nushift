// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, future::Future, net::UdpSocket, pin::Pin, sync::Arc, time::Instant};

use bytes::Bytes;
use h3::error::ErrorLevel;
use http::{Response, StatusCode};
use nsq::NsqServer;
use quinn::{AsyncTimer, AsyncUdpSocket, Runtime, SmolRuntime};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use smol::{fs::File, prelude::*, Executor};

/// Use a global executor, because a `quinn::Runtime` must be `'static`. We
/// explicitly specify it (rather than using `smol::spawn` that uses an implicit
/// default global executor) just to be more explicit.
static EXECUTOR: Executor<'_> = Executor::new();

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    smol::block_on(EXECUTOR.run(async {
        let mut file = File::open("your_server_secret.postcard")
            .await
            .map_err(|_| "A secret file your_server_secret.postcard was not found or couldn't be opened. Please generate it using generate_your_secrets.rs.")?;

        // Use postcard::from_bytes, not postcard::from_io, because the latter
        // requires a buffer anyway (that is as large as the file).
        let mut contents = vec![];
        file.read_to_end(&mut contents).await?;
        let secret_file: SecretFile = postcard::from_bytes(&contents)?;

        let runtime = Arc::new(SmolExplicitRuntime);

        let endpoint = NsqServer::new_with_socket(
            secret_file.secret,
            UdpSocket::bind("[::1]:45777")?, // Only listen on localhost for this example
            runtime,
        )?.endpoint();

        println!("Server started. Waiting for connections...");

        // Accept connections.
        while let Some(incoming) = endpoint.accept().await {
            // Start a task to handle this connection.
            EXECUTOR.spawn(async {
                let connection = match incoming.await {
                    Ok(connection) => connection,
                    Err(connection_error) => {
                        eprintln!("Error accepting connection: {connection_error}");
                        return;
                    }
                };

                println!("Connection accepted");

                let mut connection = match h3::server::Connection::new(h3_quinn::Connection::new(connection)).await {
                    Ok(connection) => connection,
                    Err(h3_error) => {
                        eprintln!("Error creating h3 connection: {h3_error}");
                        return;
                    }
                };

                // Accept streams.
                loop {
                    match connection.accept().await {
                        Ok(Some((_req, mut stream))) => {
                            // Start a task to handle this request, so more
                            // requests (on the same connection) can be
                            // processed while this one is being processed.
                            EXECUTOR.spawn(async move {
                                let resp = Response::builder()
                                    .status(StatusCode::OK)
                                    .header("Content-Type", "text/plain")
                                    .body(())
                                    .expect("Should be a valid response");

                                match stream.send_response(resp).await {
                                    Ok(_) => {}
                                    Err(err) => {
                                        eprintln!("Failed to send response header to client: {err}");
                                    }
                                }

                                match stream.send_data(Bytes::from_static(b"Hello, world!\n")).await {
                                    Ok(_) => {}
                                    Err(err) => {
                                        eprintln!("Failed to send response body to client: {err}");
                                    }
                                }

                                match stream.finish().await {
                                    Ok(_) => {}
                                    Err(err) => {
                                        eprintln!("Failed to finish stream: {err}");
                                    }
                                }

                                println!("Response sent");
                            }).detach();
                        }

                        // Connection closed, break out of loop and allow
                        // connection to end
                        Ok(None) => {
                            break;
                        }

                        Err(stream_err) => {
                            eprintln!("Error accepting stream: {stream_err}");
                            match stream_err.get_error_level() {
                                ErrorLevel::ConnectionError => break,
                                ErrorLevel::StreamError => continue,
                            }
                        }
                    }
                }

                println!("Connection closed");
            }).detach();
        }

        println!("Server shutting down...");

        // Wait for all connections to be cleanly shut down
        endpoint.wait_idle().await;

        println!("Server shut down.");

        Ok(())
    }))
}

#[derive(Debug)]
struct SmolExplicitRuntime;

impl Runtime for SmolExplicitRuntime {
    fn new_timer(&self, i: Instant) -> Pin<Box<dyn AsyncTimer>> {
        SmolRuntime.new_timer(i)
    }

    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        EXECUTOR.spawn(future).detach()
    }

    fn wrap_udp_socket(&self, t: UdpSocket) -> std::io::Result<Arc<dyn AsyncUdpSocket>> {
        SmolRuntime.wrap_udp_socket(t)
    }
}

#[derive(Serialize, Deserialize)]
struct SecretFile {
    version: u64,
    #[serde(with = "BigArray")]
    secret: [u8; 56],
}
