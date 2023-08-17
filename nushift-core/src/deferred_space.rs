use std::collections::{HashMap, hash_map::Entry};

use num_enum::IntoPrimitive;
use reusable_id_pool::{ReusableIdPoolManual, ReusableIdPoolError};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use super::shm_space::{CapType, OwnedShmIdAndCap, ShmCapId, ShmCap, ShmSpace, ShmSpaceError, ShmType};
use super::usize_or_u64::UsizeOrU64;

pub trait DeferredSpace {
    type SpaceError;
    type Cap;

    fn new() -> Self;
    fn new_cap(&mut self, context: &str) -> Result<u64, Self::SpaceError>;
    fn get_mut(&mut self, cap_id: u64) -> Option<&mut Self::Cap>;
    fn destroy_cap(&mut self, context: &str, cap_id: u64) -> Result<(), Self::SpaceError>;
}

pub trait DeferredSpaceSpecific {
    fn process_cap_str(&mut self, str: &str, output_shm_cap: &mut ShmCap);
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
        DefaultDeferredSpace {
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

        shm_space.release_shm_cap(input_shm_cap_id, CapType::UserCap).map_err(|shm_space_error| match shm_space_error {
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
        enum PrologueReturn<'space> {
            ReturnOk,
            ReturnErr,
            ContinueCaps(&'space mut ShmCap, &'space mut ShmCap),
        }

        /// A lifetime annotation doesn't cause this to be monomorphised, so for
        /// our purposes it's still a non-generic inner function
        fn prologue<'space>(this: &'space mut DefaultDeferredSpace, cap_id: DefaultDeferredSpaceCapId) -> PrologueReturn<'space> {
            // If cap has been deleted after progress is started, that is valid and
            // now do nothing here.
            let default_deferred_cap = match this.get_mut(cap_id) {
                Some(default_deferred_cap) => default_deferred_cap,
                None => return PrologueReturn::ReturnOk,
            };

            // It should not be possible for in_progress_cap to be empty. This is an
            // internal error.
            match default_deferred_cap.in_progress_cap {
                Some(InProgressCap { input: (_, ref mut input_shm_cap), output: (_, ref mut output_shm_cap) }) => PrologueReturn::ContinueCaps(input_shm_cap, output_shm_cap),
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

    /// TODO: Change this weird format to something that's been implemented for us, like Postcard.
    fn process_cap_content<S>(deferred_space_specific: &mut S, input_shm_cap: &mut ShmCap, output_shm_cap: &mut ShmCap)
    where
        S: DeferredSpaceSpecific,
    {
        /// A lifetime annotation doesn't cause this to be monomorphised, so for
        /// our purposes it's still a non-generic inner function
        fn inner<'input>(input_shm_cap: &'input mut ShmCap, output_shm_cap: &mut ShmCap) -> Result<&'input str, ()> {
            let untrusted_length = u64::from_le_bytes(input_shm_cap.backing()[0..8].try_into().unwrap());
            if untrusted_length == 0
                || untrusted_length > input_shm_cap.shm_type().page_bytes() - 8
                || UsizeOrU64::u64(untrusted_length) > UsizeOrU64::usize(usize::MAX - 8)
            {
                log::debug!("Invalid length: {untrusted_length}");
                output_shm_cap.backing_mut()[0..8].copy_from_slice(&u64::from(DeferredError::InvalidLength).to_le_bytes());
                return Err(());
            }

            // The length can still be garbage/not represent the length of the data
            // that follows it.
            //
            // The length is required as a hint for correct parsing, but all other
            // cases should be robustly handled.

            core::str::from_utf8(&input_shm_cap.backing()[8..8+(untrusted_length as usize)])
                .map_err(|utf8_error| {
                    log::debug!("from_utf8 error: {utf8_error}");
                    print_error(output_shm_cap, DeferredError::InvalidDataUtf8, &utf8_error);
                    ()
                })
        }

        let Ok(str) = inner(input_shm_cap, output_shm_cap) else { return; };

        deferred_space_specific.process_cap_str(str, output_shm_cap);
    }
}

pub fn print_error(output_shm_cap: &mut ShmCap, deferred_error: DeferredError, error: &dyn core::fmt::Display) {
    let error_message = format!("{deferred_error:?}: {error}");

    // Write error code
    output_shm_cap.backing_mut()[0..8].copy_from_slice(&u64::from(deferred_error).to_le_bytes());

    // Only write the error message and length to the output cap if there's actually room
    if UsizeOrU64::usize(error_message.len()) <= UsizeOrU64::u64(output_shm_cap.shm_type().page_bytes() - 16) {
        // If it is <= a u64, which was checked in the if condition, it can be casted to one
        let error_message_len = error_message.len() as u64;
        output_shm_cap.backing_mut()[8..16].copy_from_slice(&error_message_len.to_le_bytes());
        output_shm_cap.backing_mut()[16..(16+error_message.len())].copy_from_slice(error_message.as_bytes());
    }
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

#[derive(IntoPrimitive, Debug)]
#[repr(u64)]
#[non_exhaustive]
pub enum DeferredError {
    InvalidLength = 0,
    InvalidDataUtf8 = 1,
    InvalidDataRon = 2,
}
