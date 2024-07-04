// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fs::File};

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

fn main() -> Result<(), Box<dyn Error>> {
    let mut your_secret = [0u8; 56];
    OsRng.fill_bytes(&mut your_secret);
    let secret_file = SecretFile { version: 1, secret: your_secret.into() };

    let file = File::create_new("your_secret.postcard")
        .map_err(|_| "The file your_secret.postcard already exists in the current directory. Please delete/move/rename it before running this again.")?;

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

    println!("Secret generated! Located at your_secret.postcard in the current directory");

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct SecretFile {
    version: u64,
    secret: Vec<u8>,
}
