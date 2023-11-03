use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;
use postcard::Error as PostcardError;

use crate::shm_space::{ShmCapId, ShmSpace, ShmSpaceError};

pub struct DebugPrint;

impl DebugPrint {
    pub fn new() -> Self {
        Self
    }

    pub fn debug_print(&self, input_shm_cap_id: ShmCapId, shm_space: &ShmSpace) -> Result<(), DebugPrintError> {
        let input_shm_cap = shm_space.get_shm_cap_user(input_shm_cap_id).map_err(|shm_space_error| match shm_space_error {
            ShmSpaceError::CapNotFound => ShmCapNotFoundSnafu { id: input_shm_cap_id }.build(),
            ShmSpaceError::PermissionDenied => ShmPermissionDeniedSnafu { id: input_shm_cap_id }.build(),
            _ => ShmUnexpectedSnafu.build(),
        })?;

        let debug_message: &str = postcard::from_bytes(input_shm_cap.backing()).context(DeserializeStringSnafu)?;

        tracing::debug!("DebugPrint: {}", debug_message);

        Ok(())
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum DebugPrintError {
    #[snafu(display("Error deserialising string: {source}"))]
    DeserializeStringError { source: PostcardError },
    #[snafu(display("The SHM cap with ID {id} was not found."))]
    ShmCapNotFound { id: ShmCapId },
    #[snafu(display("The SHM cap with ID {id} is not allowed to be used as an input cap, possibly because it is an ELF cap."))]
    ShmPermissionDenied { id: ShmCapId },
    ShmUnexpectedError,
}
