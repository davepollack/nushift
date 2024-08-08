// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, io::{self, Read, Write}, sync::Arc};

use bytes::Buf;
use http::Request;
use nsq::NsqClient;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use smol::{fs::File, future, prelude::*};
use snafu::prelude::*;

use self::memory_inefficient_tofu_store::MemoryInefficientTofuStore;
use self::smol_explicit_runtime::{EXECUTOR, SmolExplicitRuntime};

#[snafu::report]
fn main() -> Result<(), MainError> {
    tracing_subscriber::fmt::init();

    smol::block_on(EXECUTOR.run(async {
        let mut file = File::open("your_client_secret.postcard")
            .await
            .map_err(|_| "A secret file your_client_secret.postcard was not found or couldn't be opened. Please generate it using generate_your_secrets.rs.")?;

        // Use postcard::from_bytes, not postcard::from_io, because the latter
        // requires a buffer anyway (that is as large as the file).
        let mut contents = vec![];
        file.read_to_end(&mut contents).await?;
        let secret_file: SecretFile = postcard::from_bytes(&contents)?;

        let runtime = Arc::new(SmolExplicitRuntime);

        let client = NsqClient::new_v6(secret_file.secret, runtime)?;

        println!("Connecting to [::1]:45777...");

        let connection = client.connect_with_tofu(MemoryInefficientTofuStore, "[::1]:45777".parse()?, "localhost").await?;
        let (mut connection, mut send_request) = h3::client::new(h3_quinn::Connection::new(connection)).await?;

        println!("Connected. Sending request...");
        println!();

        let drive = async move {
            future::poll_fn(|cx| connection.poll_close(cx))
                .await
                .map_err(|err| Box::new(err) as Box<dyn Error>)
        };

        let request = async move {
            let request = Request::builder().uri("nsq://localhost/").body(())?;
            let mut stream = send_request.send_request(request).await?;
            stream.finish().await?;

            let response = stream.recv_response().await?;

            println!("{:?} {}", response.version(), response.status());
            println!("{:#?}", response.headers());
            println!();

            while let Some(chunk) = stream.recv_data().await? {
                let mut contents = vec![];
                chunk.reader().read_to_end(&mut contents)?;
                io::stdout().write_all(&contents)?;
            }

            Ok::<(), Box<dyn Error>>(())
        };

        future::try_zip(drive, request).await?;

        // Wait for connection to be cleanly shut down
        client.endpoint().wait_idle().await;

        Ok(())
    })).map_err(|err: Box<dyn Error>| err.into())
}

#[derive(Debug, Snafu)]
#[snafu(transparent)]
struct MainError {
    source: Box<dyn Error>,
}

#[derive(Serialize, Deserialize)]
struct SecretFile {
    version: u64,
    #[serde(with = "BigArray")]
    secret: [u8; 56],
}

mod smol_explicit_runtime {
    use std::{net::UdpSocket, pin::Pin, sync::Arc, time::Instant};

    use quinn::{AsyncTimer, AsyncUdpSocket, Runtime, SmolRuntime};
    use smol::{prelude::*, Executor};

    /// Use a global executor, because a `quinn::Runtime` must be `'static`. We
    /// explicitly specify it (rather than using `smol::spawn` that uses an implicit
    /// default global executor) just to be more explicit.
    pub static EXECUTOR: Executor<'_> = Executor::new();

    #[derive(Debug)]
    pub struct SmolExplicitRuntime;

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
}

mod memory_inefficient_tofu_store {
    use std::{
        collections::{hash_map::Entry, HashMap},
        io::{self, SeekFrom},
        net::SocketAddr,
    };

    use nsq::TofuStore;
    use serde::{Deserialize, Serialize};
    use serde_big_array::BigArray;
    use smol::{fs::OpenOptions, prelude::*};
    use snafu::prelude::*;

    /// In this TOFU store, the entire database is loaded into memory. That is not great.
    pub struct MemoryInefficientTofuStore;

    impl TofuStore for MemoryInefficientTofuStore {
        type IsTrustedKeyError = MemoryInefficientTofuStoreError;

        async fn is_trusted_key(&mut self, addr: SocketAddr, server_name: &str, remote_static_key: &[u8]) -> Result<bool, Self::IsTrustedKeyError> {
            let remote_static_key = remote_static_key
                .try_into()
                .map_err(|_| KeyLengthSnafu { received_key_length: remote_static_key.len() }.build())?;

            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open("tofu_store.postcard")
                .await
                .context(IoSnafu)?;

            // It is not atomic to do this check then write the file later. This
            // could be solved by using a real database.
            let mut tofu_db = if file.metadata().await.context(IoSnafu)?.len() == 0 {
                MemoryInefficientTofuDb { version: 1, db: HashMap::new() }
            } else {
                let mut contents = vec![];
                file.read_to_end(&mut contents).await.context(IoSnafu)?;
                postcard::from_bytes(&contents).context(PostcardSnafu)?
            };

            match tofu_db.db.entry(Host::new(addr, server_name)) {
                Entry::Vacant(entry) => {
                    // Haven't seen this host before, key is trusted
                    entry.insert(RemoteStaticKey(remote_static_key));

                    // Write to file
                    let tofu_db_bytes = postcard::to_stdvec(&tofu_db).context(PostcardSnafu)?;
                    file.set_len(0).await.context(IoSnafu)?;
                    file.seek(SeekFrom::Start(0)).await.context(IoSnafu)?;
                    file.write_all(&tofu_db_bytes).await.context(IoSnafu)?;
                    file.sync_all().await.context(IoSnafu)?;

                    Ok(true)
                }
                Entry::Occupied(entry) => {
                    // Does it match trusted key for this host?
                    Ok(entry.get().0 == remote_static_key)
                }
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    struct MemoryInefficientTofuDb {
        version: u64,
        db: HashMap<Host, RemoteStaticKey>,
    }

    #[derive(Serialize, Deserialize, PartialEq, Eq, Hash)]
    struct Host {
        server_name: String,
        port: u64,
    }

    impl Host {
        fn new(addr: SocketAddr, server_name: &str) -> Self {
            Self { server_name: server_name.into(), port: addr.port() as u64 }
        }
    }

    #[derive(Serialize, Deserialize)]
    struct RemoteStaticKey(#[serde(with = "BigArray")] [u8; 56]);

    #[derive(Debug, Snafu)]
    pub enum MemoryInefficientTofuStoreError {
        IoError { source: io::Error },
        PostcardError { source: postcard::Error },
        #[snafu(display("Expected a remote key length of 56 bytes, got length {received_key_length} bytes"))]
        KeyLengthError { received_key_length: usize },
    }
}
