use std::collections::{HashMap, hash_map::Entry};

use num_enum::IntoPrimitive;
use reusable_id_pool::{ReusableIdPoolManual, ReusableIdPoolError};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use self::accessibility_tree::AccessibilityTree;
use super::shm_space::{OwnedShmIdAndCap, ShmSpace, ShmCapId, ShmCap, ShmSpaceError, ShmType};
use super::usize_or_u64::UsizeOrU64;

mod accessibility_tree;

struct InProgressCap {
    input: OwnedShmIdAndCap,
    output: OwnedShmIdAndCap,
}
impl InProgressCap {
    fn new(input: OwnedShmIdAndCap, output: OwnedShmIdAndCap) -> Self {
        Self { input, output }
    }
}

pub struct AccessibilityTreeCap {
    in_progress_cap: Option<InProgressCap>,
}
impl AccessibilityTreeCap {
    pub fn new() -> Self {
        Self { in_progress_cap: None }
    }
}

pub type AccessibilityTreeCapId = u64;

pub struct AccessibilityTreeSpace {
    id_pool: ReusableIdPoolManual,
    space: HashMap<AccessibilityTreeCapId, AccessibilityTreeCap>,
}

impl AccessibilityTreeSpace {
    pub fn new() -> Self {
        AccessibilityTreeSpace { id_pool: ReusableIdPoolManual::new(), space: HashMap::new() }
    }

    // TODO: Should new and destroy also be part blocking, part deferred?

    pub fn new_accessibility_tree_cap(&mut self) -> Result<AccessibilityTreeCapId, AccessibilityTreeSpaceError> {
        let id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyLiveIDs => ExhaustedSnafu.build() })?;

        match self.space.entry(id) {
            Entry::Occupied(_) => return DuplicateIdSnafu.fail(),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(AccessibilityTreeCap::new()),
        };

        Ok(id)
    }

    /// Releases SHM cap, but does not do further processing yet.
    pub fn publish_accessibility_tree_blocking(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId, input_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), AccessibilityTreeSpaceError> {
        let accessibility_tree_cap = self.space.get_mut(&accessibility_tree_cap_id).ok_or_else(|| CapNotFoundSnafu { id: accessibility_tree_cap_id }.build())?;

        // Currently, you can't queue/otherwise process an accessibility tree
        // while one is being processed.
        match accessibility_tree_cap.in_progress_cap {
            Some(_) => return InProgressSnafu.fail(),
            None => {},
        };

        shm_space.release_shm_cap(input_shm_cap_id).map_err(|shm_space_error| match shm_space_error {
            ShmSpaceError::CapNotFound => ShmCapNotFoundSnafu { id: input_shm_cap_id }.build(),
            err => AccessibilityTreeSpaceError::ShmSpaceInternalError { source: err },
        })?;

        // Move out of the SHM space for the duration of us processing it.
        let input_shm_cap = shm_space.move_shm_cap_to_other_space(input_shm_cap_id).ok_or_else(|| PublishInternalSnafu.build())?; // Internal error because presence was already checked in release
        // Create an output cap
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1).map_err(|shm_new_error| match shm_new_error {
            ShmSpaceError::CapacityNotAvailable
            | ShmSpaceError::BackingCapacityNotAvailable { .. }
            | ShmSpaceError::BackingCapacityNotAvailableOverflows => ShmCapacityNotAvailableSnafu.build(),
            ShmSpaceError::Exhausted => ShmExhaustedSnafu.build(),
            err => AccessibilityTreeSpaceError::ShmSpaceInternalError { source: err },
        })?;
        let output_shm_cap = shm_space.move_shm_cap_to_other_space(output_shm_cap_id).ok_or_else(|| PublishInternalSnafu.build())?; // Internal error because we just created it

        accessibility_tree_cap.in_progress_cap = Some(InProgressCap::new((input_shm_cap_id, input_shm_cap), (output_shm_cap_id, output_shm_cap)));
        Ok(())
    }

    /// The Err(()) variant is only used for an internal error where the output
    /// cap is not available. All other errors should be reported through the
    /// output cap.
    pub fn publish_accessibility_tree_deferred(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        // If cap has been deleted after progress is started, that is valid and
        // now do nothing here.
        let accessibility_tree_cap = match self.space.get_mut(&accessibility_tree_cap_id) {
            Some(accessibility_tree_cap) => accessibility_tree_cap,
            None => return Ok(()),
        };

        // It should not be possible for in_progress_cap to be empty. This is an
        // internal error.
        let (input_shm_cap, output_shm_cap) = accessibility_tree_cap.in_progress_cap
            .as_mut()
            .ok_or(())
            .map(|InProgressCap { input: (_, input_shm_cap), output: (_, output_shm_cap) }| (input_shm_cap, output_shm_cap))?;

        Self::process_cap_content(input_shm_cap, output_shm_cap);

        // It should still not be possible for in_progress_cap to be empty. This
        // is an internal error.
        let in_progress_cap = accessibility_tree_cap.in_progress_cap.take().ok_or(())?;
        shm_space.move_shm_cap_back_into_space(in_progress_cap.input.0, in_progress_cap.input.1);
        shm_space.move_shm_cap_back_into_space(in_progress_cap.output.0, in_progress_cap.output.1);

        Ok(())
    }

    fn process_cap_content(input_shm_cap: &mut ShmCap, output_shm_cap: &mut ShmCap) {
        let untrusted_length = u64::from_le_bytes(input_shm_cap.backing()[0..8].try_into().unwrap());
        if untrusted_length == 0
            || untrusted_length > input_shm_cap.shm_type().page_bytes() - 8
            || UsizeOrU64::u64(untrusted_length) > UsizeOrU64::usize(usize::MAX - 8)
        {
            log::debug!("Invalid length: {untrusted_length}");
            output_shm_cap.backing_mut()[0..8].copy_from_slice(&u64::from(AccessibilityTreeError::InvalidLength).to_le_bytes());
            return;
        }

        // The length can still be garbage/not represent the length of the data
        // that follows it.
        //
        // The length is required as a hint for correct parsing, but all other
        // cases should be robustly handled.

        let ron_serialized = match core::str::from_utf8(&input_shm_cap.backing()[8..8+(untrusted_length as usize)]) {
            Ok(str) => str,
            Err(utf8_error) => {
                log::debug!("from_utf8 error: {utf8_error}");
                Self::print_error(output_shm_cap, AccessibilityTreeError::InvalidDataUtf8, utf8_error);
                return;
            },
        };

        let accessibility_tree: AccessibilityTree = match ron::from_str(ron_serialized) {
            Ok(accessibility_tree) => accessibility_tree,
            Err(spanned_error) => {
                log::debug!("Deserialisation error: {spanned_error}");
                Self::print_error(output_shm_cap, AccessibilityTreeError::InvalidDataRon, spanned_error);
                return;
            },
        };
        // TODO: Where do we store accessibility_tree?
        log::info!("{accessibility_tree:?}");
    }

    pub fn destroy_accessibility_tree_cap(&mut self, accessibility_tree_cap_id: AccessibilityTreeCapId) -> Result<(), AccessibilityTreeSpaceError> {
        // TODO: You should not be allowed to destroy it if it's in progress. Or
        // the destroy should be deferred. And all deferred tasks should be
        // executed on app shutdown (?), both when program ends or when user
        // terminates the tab while running (?)
        self.space.contains_key(&accessibility_tree_cap_id).then_some(()).ok_or_else(|| CapNotFoundSnafu { id: accessibility_tree_cap_id }.build())?;

        self.space.remove(&accessibility_tree_cap_id);
        self.id_pool.release(accessibility_tree_cap_id);

        Ok(())
    }

    fn print_error<E: core::fmt::Display>(output_shm_cap: &mut ShmCap, accessibility_tree_error: AccessibilityTreeError, error: E) {
        let error_message = format!("{accessibility_tree_error:?}: {error}");
        // Write error code
        output_shm_cap.backing_mut()[0..8].copy_from_slice(&u64::from(accessibility_tree_error).to_le_bytes());
        // Only write the error message and length to the output cap if there's actually room
        if UsizeOrU64::usize(error_message.len()) <= UsizeOrU64::u64(output_shm_cap.shm_type().page_bytes() - 16) {
            // If it is <= a u64, which was checked in the if condition, it can be casted to one
            let error_message_len = error_message.len() as u64;
            output_shm_cap.backing_mut()[8..16].copy_from_slice(&error_message_len.to_le_bytes());
            output_shm_cap.backing_mut()[16..(16+error_message.len())].copy_from_slice(error_message.as_bytes());
        }
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum AccessibilityTreeSpaceError {
    #[snafu(display("The new pool ID was already present in the space. This should never happen, and indicates a bug in Nushift's code."))]
    DuplicateId,
    #[snafu(display("The maximum amount of accessibility tree capabilities have been used for this app. Please destroy some capabilities."))]
    Exhausted,
    #[snafu(display("The accessibility tree cap with ID {id} was not found."))]
    CapNotFound { id: AccessibilityTreeCapId },
    #[snafu(display("Another accessibility tree is currently being processed."))]
    InProgress,
    #[snafu(display("The SHM cap with ID {id} was not found."))]
    ShmCapNotFound { id: ShmCapId },
    ShmExhausted,
    ShmCapacityNotAvailable,
    ShmSpaceInternalError { source: ShmSpaceError }, // Should never occur, indicates a bug in Nushift's code
    PublishInternalError, // Should never occur, indicates a bug in Nushift's code
}

#[derive(IntoPrimitive, Debug)]
#[repr(u64)]
#[non_exhaustive]
enum AccessibilityTreeError {
    InvalidLength = 0,
    InvalidDataUtf8 = 1,
    InvalidDataRon = 2,
}
