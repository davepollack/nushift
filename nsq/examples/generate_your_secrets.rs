// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fs::File};

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use snafu::{prelude::*, Report};

// This example generates secret files that used to be used by the server and
// client examples, but no longer are as messing with files made the examples
// too hard to run. This example is kept here as the secret generation code
// might help later. It might also be deleted later.

fn main() -> Report<MainError> {
    Report::capture(|| {
        generate_secret("your_server_secret.postcard")?;
        generate_secret("your_client_secret.postcard")?;

        println!("Secrets generated! Located at your_server_secret.postcard and your_client_secret.postcard in the current directory");

        Ok(())
    })
}

#[derive(Debug, Snafu)]
#[snafu(transparent)]
struct MainError {
    source: Box<dyn Error>,
}

fn generate_secret(secret_file_name: impl AsRef<str>) -> Result<(), Box<dyn Error>> {
    let mut your_secret = [0u8; 56];
    OsRng.fill_bytes(&mut your_secret);
    let secret_file = SecretFile { version: 1, secret: your_secret };

    let file = File::create_new(secret_file_name.as_ref())
        .map_err(|_| format!("The file {} already exists in the current directory. Please delete/move/rename it before running this again.", secret_file_name.as_ref()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o600);

        file.set_permissions(perms)?;
    }

    let file = postcard::to_io(&secret_file, file)
        .map_err(|postcard_error| format!("Could not serialise to secret file: {postcard_error}"))?;

    file.sync_all()
        .map_err(|io_error| format!("Could not sync secret file: {io_error:?}"))?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct SecretFile {
    version: u64,
    #[serde(with = "BigArray")]
    secret: [u8; 56],
}
