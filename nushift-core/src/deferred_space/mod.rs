use std::collections::{HashMap, hash_map::Entry};

use num_enum::IntoPrimitive;
use reusable_id_pool::{ReusableIdPoolManual, ReusableIdPoolError};
use serde::{Deserialize, Serialize};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use super::shm_space::{CapType, OwnedShmIdAndCap, ShmCapId, ShmCap, ShmSpace, ShmSpaceError, ShmType};

pub(super) mod app_global_deferred_space;

// This trait may not be necessary. I'm only implementing it for
// DefaultDeferredSpace, and I'm currently composing DefaultDeferredSpace with
// other things, rather than using generics.
pub trait DeferredSpace {
    type SpaceError;
    type Cap;

    fn new() -> Self;
    fn new_cap(&mut self, context: &str) -> Result<u64, Self::SpaceError>;
    fn get_mut(&mut self, cap_id: u64) -> Option<&mut Self::Cap>;
    fn destroy_cap(&mut self, context: &str, cap_id: u64) -> Result<(), Self::SpaceError>;
}

// In contrast to the above trait, this one is used by multiple impls.
pub trait DeferredSpaceSpecific {
    type Payload<'de>: Deserialize<'de>;

    fn process_cap_payload(&mut self, payload: Self::Payload<'_>, output_shm_cap: &mut ShmCap);
}

pub type DefaultDeferredSpaceCapId = u64;

struct InProgressCap {
    input: OwnedShmIdAndCap,
    output: OwnedShmIdAndCap,
}
impl InProgressCap {
    fn new(input: OwnedShmIdAndCap, output: OwnedShmIdAndCap) -> Self {
        Self { input, output }
    }
}

pub struct DefaultDeferredCap {
    in_progress_cap: Option<InProgressCap>,
}
impl DefaultDeferredCap {
    fn new() -> Self {
        Self { in_progress_cap: None }
    }
}

pub struct DefaultDeferredSpace {
    id_pool: ReusableIdPoolManual,
    space: HashMap<DefaultDeferredSpaceCapId, DefaultDeferredCap>,
}

impl DeferredSpace for DefaultDeferredSpace {
    type SpaceError = DeferredSpaceError;
    type Cap = DefaultDeferredCap;

    fn new() -> Self {
        Self {
            id_pool: ReusableIdPoolManual::new(),
            space: HashMap::new(),
        }
    }

    fn new_cap(&mut self, context: &str) -> Result<u64, DeferredSpaceError> {
        let id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyLiveIDs => ExhaustedSnafu { context }.build() })?;

        match self.space.entry(id) {
            Entry::Occupied(_) => return DuplicateIdSnafu.fail(),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(DefaultDeferredCap::new()),
        };

        Ok(id)
    }

    fn get_mut(&mut self, cap_id: u64) -> Option<&mut Self::Cap> {
        self.space.get_mut(&cap_id)
    }

    fn destroy_cap(&mut self, context: &str, cap_id: u64) -> Result<(), Self::SpaceError> {
        // TODO: You should not be allowed to destroy it if it's in progress. Or
        // the destroy should be deferred. And all deferred tasks should be
        // executed on app shutdown (?), both when program ends or when user
        // terminates the tab while running (?)
        self.space.contains_key(&cap_id).then_some(()).ok_or_else(|| CapNotFoundSnafu { context, id: cap_id }.build())?;

        self.space.remove(&cap_id);
        self.id_pool.release(cap_id);

        Ok(())
    }
}

impl DefaultDeferredSpace {
    /// Releases SHM cap, but does not do further processing yet.
    pub fn publish_blocking(&mut self, context: &str, cap_id: DefaultDeferredSpaceCapId, input_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        let default_deferred_cap = self.get_mut(cap_id).ok_or_else(|| CapNotFoundSnafu { context, id: cap_id }.build())?;

        // Currently, you can't queue/otherwise process an [accessibility tree/other thing]
        // while one is being processed.
        matches!(default_deferred_cap.in_progress_cap, None).then_some(()).ok_or_else(|| InProgressSnafu { context }.build())?;

        shm_space.release_shm_cap(input_shm_cap_id).map_err(|shm_space_error| match shm_space_error {
            ShmSpaceError::CapNotFound => ShmCapNotFoundSnafu { id: input_shm_cap_id }.build(),
            ShmSpaceError::PermissionDenied => ShmPermissionDeniedSnafu { id: input_shm_cap_id }.build(),
            err => DeferredSpaceError::ShmSpaceInternalError { source: err },
        })?;

        // Move out of the SHM space for the duration of us processing it.
        let input_shm_cap = shm_space.move_shm_cap_to_other_space(input_shm_cap_id).ok_or_else(|| PublishInternalSnafu.build())?; // Internal error because presence was already checked in release
        // Create an output cap
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::UserCap).map_err(|shm_new_error| match shm_new_error {
            ShmSpaceError::CapacityNotAvailable
            | ShmSpaceError::BackingCapacityNotAvailable { .. }
            | ShmSpaceError::BackingCapacityNotAvailableOverflows => ShmCapacityNotAvailableSnafu.build(),
            ShmSpaceError::Exhausted => ShmExhaustedSnafu.build(),
            err => DeferredSpaceError::ShmSpaceInternalError { source: err },
        })?;
        let output_shm_cap = shm_space.move_shm_cap_to_other_space(output_shm_cap_id).ok_or_else(|| PublishInternalSnafu.build())?; // Internal error because we just created it

        default_deferred_cap.in_progress_cap = Some(InProgressCap::new((input_shm_cap_id, input_shm_cap), (output_shm_cap_id, output_shm_cap)));
        Ok(())
    }

    /// The Err(()) variant is only used for an internal error where the output
    /// cap is not available. All other errors should be reported through the
    /// output cap.
    pub fn publish_deferred<S>(&mut self, deferred_space_specific: &mut S, cap_id: DefaultDeferredSpaceCapId, shm_space: &mut ShmSpace) -> Result<(), ()>
    where
        S: DeferredSpaceSpecific,
    {
        enum PrologueReturn<'deferred_space> {
            ReturnOk,
            ReturnErr,
            ContinueCaps(&'deferred_space ShmCap, &'deferred_space mut ShmCap),
        }

        fn prologue(this: &mut DefaultDeferredSpace, cap_id: DefaultDeferredSpaceCapId) -> PrologueReturn<'_> {
            // If cap has been deleted after progress is started, that is valid
            // and now do nothing here.
            //
            // TODO: This comment contradicts the comment in destroy_cap, and
            // it's only valid if destroy_cap moved the in-progress caps back
            // into the SHM space (so the stats are not corrupted), which it
            // currently does not! And probably other things I'm forgetting. So
            // consider going with the comment in destroy_cap and say it's
            // actually not valid.
            let default_deferred_cap = match this.get_mut(cap_id) {
                Some(default_deferred_cap) => default_deferred_cap,
                None => return PrologueReturn::ReturnOk,
            };

            // It should not be possible for in_progress_cap to be empty. This is an
            // internal error.
            match default_deferred_cap.in_progress_cap {
                Some(InProgressCap { input: (_, ref input_shm_cap), output: (_, ref mut output_shm_cap) }) => PrologueReturn::ContinueCaps(input_shm_cap, output_shm_cap),
                _ => PrologueReturn::ReturnErr,
            }
        }

        let (input_shm_cap, output_shm_cap) = match prologue(self, cap_id) {
            PrologueReturn::ReturnOk => return Ok(()),
            PrologueReturn::ReturnErr => return Err(()),
            PrologueReturn::ContinueCaps(input_shm_cap, output_shm_cap) => (input_shm_cap, output_shm_cap),
        };

        Self::process_cap_content(deferred_space_specific, input_shm_cap, output_shm_cap);

        fn epilogue(this: &mut DefaultDeferredSpace, cap_id: DefaultDeferredSpaceCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
            // It should still not be possible for in_progress_cap to be empty. This
            // is an internal error.
            let default_deferred_cap = this.get_mut(cap_id).ok_or(())?;
            let in_progress_cap = default_deferred_cap.in_progress_cap.take().ok_or(())?;
            shm_space.move_shm_cap_back_into_space(in_progress_cap.input.0, in_progress_cap.input.1);
            shm_space.move_shm_cap_back_into_space(in_progress_cap.output.0, in_progress_cap.output.1);

            Ok(())
        }

        epilogue(self, cap_id, shm_space)
    }

    fn process_cap_content<S>(deferred_space_specific: &mut S, input_shm_cap: &ShmCap, output_shm_cap: &mut ShmCap)
    where
        S: DeferredSpaceSpecific,
    {
        let payload = match postcard::from_bytes(input_shm_cap.backing()) {
            Ok(payload) => payload,
            Err(postcard_error) => {
                tracing::debug!("Postcard deserialise error: {postcard_error}");
                print_error(output_shm_cap, DeferredError::DeserializeError, &postcard_error);
                return;
            },
        };

        deferred_space_specific.process_cap_payload(payload, output_shm_cap);
    }
}

pub fn print_error(output_shm_cap: &mut ShmCap, deferred_error: DeferredError, error: &dyn core::fmt::Display) {
    let error = DeferredErrorWithMessage::new(deferred_error, error.to_string());

    // The below might fail if, for example, the serialize buffer is full. Just
    // do nothing in this case.
    let _ = postcard::to_slice(&error, output_shm_cap.backing_mut());
}

#[derive(Snafu, SnafuCliDebug)]
pub enum DeferredSpaceError {
    #[snafu(display("The new pool ID was already present in the space. This should never happen, and indicates a bug in Nushift's code."))]
    DuplicateId,
    #[snafu(display("The maximum amount of {context} capabilities have been used for this app. Please destroy some capabilities."))]
    Exhausted { context: String },
    #[snafu(display("The {context} cap with ID {id} was not found."))]
    CapNotFound { context: String, id: DefaultDeferredSpaceCapId },
    #[snafu(display("Another {context} is currently being processed."))]
    InProgress { context: String },
    #[snafu(display("The SHM cap with ID {id} was not found."))]
    ShmCapNotFound { id: ShmCapId },
    #[snafu(display("The SHM cap with ID {id} is not allowed to be used as an input cap, possibly because it is an ELF cap."))]
    ShmPermissionDenied { id: ShmCapId },
    ShmExhausted,
    ShmCapacityNotAvailable,
    ShmSpaceInternalError { source: ShmSpaceError }, // Should never occur, indicates a bug in Nushift's code
    PublishInternalError, // Should never occur, indicates a bug in Nushift's code
}

#[derive(IntoPrimitive, Debug, Serialize)]
#[repr(u64)]
pub enum DeferredError {
    DeserializeError = 0,
    DeserializeRonError = 1,
    SubmitFailed = 2,
}

#[derive(Serialize)]
struct DeferredErrorWithMessage {
    deferred_error: DeferredError,
    message: String,
}

impl DeferredErrorWithMessage {
    fn new(deferred_error: DeferredError, message: String) -> Self {
        Self { deferred_error, message }
    }
}
