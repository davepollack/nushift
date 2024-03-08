// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, hash_map::Entry};

use num_enum::IntoPrimitive;
use reusable_id_pool::{ReusableIdPoolManual, ReusableIdPoolError};
use serde::{Deserialize, Serialize};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;
#[cfg(test)]
use mockall::automock;

use crate::rollback_chain::RollbackChain;
use crate::shm_space::{OwnedShmIdAndCap, ShmCapId, ShmCap, ShmSpace, ShmSpaceError};

pub(super) mod app_global_deferred_space;

pub enum PrologueReturn<'deferred_space> {
    ContinueCapsPublish(&'deferred_space ShmCap, &'deferred_space mut ShmCap),
    ContinueCapsGet(&'deferred_space mut ShmCap),
    ReturnErr,
}

// This trait may not be necessary. I'm only implementing it for
// DefaultDeferredSpace, and I'm currently composing DefaultDeferredSpace with
// other things, rather than using generics.
pub trait DeferredSpace {
    type SpaceError;
    type Cap;
    type CapId;
    type IdPool;

    fn new_with_id_pool(id_pool: Self::IdPool) -> Self;
    fn new_cap(&mut self, context: &str) -> Result<u64, Self::SpaceError>;
    fn get_mut(&mut self, cap_id: u64) -> Option<&mut Self::Cap>;
    fn contains_key(&self, cap_id: u64) -> bool;
    fn destroy_cap(&mut self, context: &str, cap_id: u64) -> Result<(), Self::SpaceError>;
    fn get_or_publish_deferred_prologue(&mut self, cap_id: Self::CapId) -> PrologueReturn<'_>;
    fn get_or_publish_deferred_epilogue(&mut self, cap_id: Self::CapId, shm_space: &mut ShmSpace) -> Result<(), ()>;
}

// In contrast to the `DeferredSpace` trait, this one is used by multiple impls.
pub trait DeferredSpacePublish {
    type Payload<'de>: Deserialize<'de>;

    fn publish_cap_payload(&mut self, payload: Self::Payload<'_>, output_shm_cap: &mut ShmCap, cap_id: u64);
}

// In contrast to the `DeferredSpace` trait, this one is used by multiple impls.
pub trait DeferredSpaceGet {
    fn get(&mut self, output_shm_cap: &mut ShmCap);
}

pub type DefaultDeferredSpaceCapId = u64;

struct InProgressCap {
    input: Option<OwnedShmIdAndCap>,
    output: OwnedShmIdAndCap,
}

impl InProgressCap {
    fn new<OptionalInput>(input: OptionalInput, output: OwnedShmIdAndCap) -> Self
    where
        OptionalInput: Into<Option<OwnedShmIdAndCap>>,
    {
        Self { input: input.into(), output }
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

#[cfg_attr(test, automock)]
pub(crate) trait IdPoolBacking {
    fn try_allocate(&mut self) -> Result<u64, ReusableIdPoolError>;
    fn release(&mut self, id: u64);
}

impl IdPoolBacking for ReusableIdPoolManual {
    fn try_allocate(&mut self) -> Result<u64, ReusableIdPoolError> {
        self.try_allocate()
    }

    fn release(&mut self, id: u64) {
        self.release(id);
    }
}

pub struct DefaultDeferredSpace<I = ReusableIdPoolManual> {
    id_pool: I,
    space: HashMap<DefaultDeferredSpaceCapId, DefaultDeferredCap>,
}

impl<I> DeferredSpace for DefaultDeferredSpace<I>
where
    I: IdPoolBacking,
{
    type SpaceError = DeferredSpaceError;
    type Cap = DefaultDeferredCap;
    type CapId = DefaultDeferredSpaceCapId;
    type IdPool = I;

    fn new_with_id_pool(id_pool: I) -> Self {
        Self {
            id_pool,
            space: HashMap::new(),
        }
    }

    fn new_cap(&mut self, context: &str) -> Result<u64, Self::SpaceError> {
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

    fn contains_key(&self, cap_id: u64) -> bool {
        self.space.contains_key(&cap_id)
    }

    fn destroy_cap(&mut self, context: &str, cap_id: u64) -> Result<(), Self::SpaceError> {
        // Check if it exists
        let Entry::Occupied(entry) = self.space.entry(cap_id) else {
            return CapNotFoundSnafu { context, id: cap_id }.fail();
        };

        // You're not allowed to destroy it if it's in progress
        if entry.get().in_progress_cap.is_some() {
            return InProgressSnafu { context }.fail();
        }

        entry.remove();
        self.id_pool.release(cap_id);

        Ok(())
    }

    fn get_or_publish_deferred_prologue(&mut self, cap_id: Self::CapId) -> PrologueReturn<'_> {
        // It should not be possible for the cap to not exist (because you're
        // not allowed to delete it if it's in progress). And it should not be
        // possible for in_progress_cap to be empty. These are internal errors.
        match self.get_mut(cap_id) {
            // Publish
            Some(DefaultDeferredCap { in_progress_cap: Some(InProgressCap { input: Some((_, ref input_shm_cap)), output: (_, ref mut output_shm_cap) }) }) => PrologueReturn::ContinueCapsPublish(input_shm_cap, output_shm_cap),
            // Get
            Some(DefaultDeferredCap { in_progress_cap: Some(InProgressCap { input: None, output: (_, ref mut output_shm_cap) }) }) => PrologueReturn::ContinueCapsGet(output_shm_cap),

            None | Some(DefaultDeferredCap { in_progress_cap: None }) => PrologueReturn::ReturnErr,
        }
    }

    fn get_or_publish_deferred_epilogue(&mut self, cap_id: Self::CapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        // It should still not be possible for in_progress_cap to be empty. This
        // is an internal error.
        let default_deferred_cap = self.get_mut(cap_id).ok_or(())?;
        let in_progress_cap = default_deferred_cap.in_progress_cap.take().ok_or(())?;
        match in_progress_cap {
            InProgressCap { input: Some(input), output } => {
                shm_space.move_shm_cap_back_into_space(input.0, input.1);
                shm_space.move_shm_cap_back_into_space(output.0, output.1);
            }
            InProgressCap { input: None, output } => {
                shm_space.move_shm_cap_back_into_space(output.0, output.1);
            }
        }

        Ok(())
    }
}

impl DefaultDeferredSpace {
    pub fn new() -> Self {
        Self::new_with_id_pool(ReusableIdPoolManual::new())
    }
}

impl<I: IdPoolBacking> DefaultDeferredSpace<I> {
    pub fn publish_blocking(&mut self, context: &str, cap_id: DefaultDeferredSpaceCapId, input_shm_cap_id: ShmCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.get_or_publish_blocking(context, cap_id, Some(input_shm_cap_id), output_shm_cap_id, shm_space)
    }

    pub fn get_blocking(&mut self, context: &str, cap_id: DefaultDeferredSpaceCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        self.get_or_publish_blocking(context, cap_id, None, output_shm_cap_id, shm_space)
    }

    /// Releases SHM caps, but does not do further processing yet.
    fn get_or_publish_blocking(&mut self, context: &str, cap_id: DefaultDeferredSpaceCapId, input_shm_cap_id: Option<ShmCapId>, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), DeferredSpaceError> {
        let default_deferred_cap = self.get_mut(cap_id).ok_or_else(|| CapNotFoundSnafu { context, id: cap_id }.build())?;

        // Currently, you can't queue/otherwise process an [accessibility tree/other thing]
        // while one is being processed.
        if default_deferred_cap.in_progress_cap.is_some() {
            return InProgressSnafu { context }.fail();
        }

        struct RollbackTarget<'a> {
            default_deferred_cap: &'a mut DefaultDeferredCap,
            shm_space: &'a mut ShmSpace,
        }

        let mut rollback_target = RollbackTarget { default_deferred_cap, shm_space };

        let mut chain = RollbackChain::new(&mut rollback_target);

        // Release the input SHM cap for the duration of us processing it.
        if let Some(input_shm_cap_id) = input_shm_cap_id {
            let input_address = chain.exec(|target| {
                target.shm_space.release_shm_cap_app(input_shm_cap_id).map_err(|shm_space_error| match shm_space_error {
                    ShmSpaceError::CapNotFound => ShmCapNotFoundSnafu { id: input_shm_cap_id }.build(),
                    ShmSpaceError::PermissionDenied => ShmPermissionDeniedSnafu { id: input_shm_cap_id }.build(),
                    err => DeferredSpaceError::ShmSpaceInternalError { source: err },
                })
            })?;

            if let Some(input_address) = input_address {
                chain.add_rollback(move |target| {
                    // Since this executes synchronously with the transaction
                    // that is being rolled back, all returned errors are an
                    // internal error. Dunno what to do with those errors.
                    let _ = target.shm_space.acquire_shm_cap_app(input_shm_cap_id, input_address);
                });
            }
        }

        // Release the output SHM cap for the duration of us processing it.
        let output_address = chain.exec(|target| {
            target.shm_space.release_shm_cap_app(output_shm_cap_id).map_err(|shm_space_error| match shm_space_error {
                ShmSpaceError::CapNotFound => ShmCapNotFoundSnafu { id: output_shm_cap_id }.build(),
                ShmSpaceError::PermissionDenied => ShmPermissionDeniedSnafu { id: output_shm_cap_id }.build(),
                err => DeferredSpaceError::ShmSpaceInternalError { source: err },
            })
        })?;

        if let Some(output_address) = output_address {
            chain.add_rollback(move |target| {
                // Since this executes synchronously with the transaction
                // that is being rolled back, all returned errors are an
                // internal error. Dunno what to do with those errors.
                let _ = target.shm_space.acquire_shm_cap_app(output_shm_cap_id, output_address);
            });
        }

        // Move out of the SHM space for the duration of us processing it. (to
        // prevent the app from re-acquiring them, which would be a violation of
        // security)
        match (input_shm_cap_id, output_shm_cap_id) {
            // Publish
            (Some(input_shm_cap_id), output_shm_cap_id) => {
                chain.exec(|target| {
                    let Some(input_shm_cap) = target.shm_space.move_shm_cap_to_other_space(input_shm_cap_id) else {
                        // Internal error because presence was already checked in release
                        return GetOrPublishInternalSnafu.fail();
                    };

                    let Some(output_shm_cap) = target.shm_space.move_shm_cap_to_other_space(output_shm_cap_id) else {
                        // Roll back taking of input cap
                        target.shm_space.move_shm_cap_back_into_space(input_shm_cap_id, input_shm_cap);
                        // Internal error because presence was already checked in release
                        return GetOrPublishInternalSnafu.fail();
                    };

                    target.default_deferred_cap.in_progress_cap = Some(InProgressCap::new((input_shm_cap_id, input_shm_cap), (output_shm_cap_id, output_shm_cap)));

                    Ok(())
                })?;

                chain.add_rollback(|target| {
                    let Some(InProgressCap { input: Some(input), output }) = target.default_deferred_cap.in_progress_cap.take() else {
                        // An internal error occurred because this is the shape
                        // of the data that we placed into it, synchronously in
                        // the transaction that is being rolled back.
                        return;
                    };
                    target.shm_space.move_shm_cap_back_into_space(input.0, input.1);
                    target.shm_space.move_shm_cap_back_into_space(output.0, output.1);
                });
            }

            // Get
            (None, output_shm_cap_id) => {
                chain.exec(|target| {
                    let Some(output_shm_cap) = target.shm_space.move_shm_cap_to_other_space(output_shm_cap_id) else {
                        // Internal error because presence was already checked in release
                        return GetOrPublishInternalSnafu.fail();
                    };

                    target.default_deferred_cap.in_progress_cap = Some(InProgressCap::new(None, (output_shm_cap_id, output_shm_cap)));

                    Ok(())
                })?;

                chain.add_rollback(|target| {
                    let Some(InProgressCap { input: None, output }) = target.default_deferred_cap.in_progress_cap.take() else {
                        // An internal error occurred because this is the shape
                        // of the data that we placed into it, synchronously in
                        // the transaction that is being rolled back.
                        return;
                    };
                    target.shm_space.move_shm_cap_back_into_space(output.0, output.1);
                });
            }
        }

        chain.all_succeeded();

        Ok(())
    }

    /// The Err(()) variant is only used for internal errors. All other errors
    /// should be reported through the output cap.
    pub fn publish_deferred<S>(&mut self, deferred_space_specific: &mut S, cap_id: DefaultDeferredSpaceCapId, shm_space: &mut ShmSpace) -> Result<(), ()>
    where
        S: DeferredSpacePublish,
    {
        let (input_shm_cap, output_shm_cap) = match self.get_or_publish_deferred_prologue(cap_id) {
            PrologueReturn::ContinueCapsPublish(input_shm_cap, output_shm_cap) => (input_shm_cap, output_shm_cap),
            PrologueReturn::ContinueCapsGet(..) => return Err(()), // Internal error. We must have started with a publish.
            PrologueReturn::ReturnErr => return Err(()),
        };

        match postcard::from_bytes(input_shm_cap.backing()) {
            Ok(payload) => {
                deferred_space_specific.publish_cap_payload(payload, output_shm_cap, cap_id);
            }
            Err(postcard_error) => {
                tracing::debug!("Postcard deserialise error: {postcard_error}");
                print_error(output_shm_cap, DeferredError::DeserializeError, &postcard_error);
            }
        }

        self.get_or_publish_deferred_epilogue(cap_id, shm_space)
    }

    /// The Err(()) variant is only used for internal errors. All other errors
    /// should be reported through the output cap.
    pub fn get_deferred<S>(&mut self, deferred_space_specific: &mut S, cap_id: DefaultDeferredSpaceCapId, shm_space: &mut ShmSpace) -> Result<(), ()>
    where
        S: DeferredSpaceGet,
    {
        let output_shm_cap = match self.get_or_publish_deferred_prologue(cap_id) {
            PrologueReturn::ContinueCapsPublish(..) => return Err(()), // Internal error. We must have started with a get.
            PrologueReturn::ContinueCapsGet(output_shm_cap) => output_shm_cap,
            PrologueReturn::ReturnErr => return Err(()),
        };

        deferred_space_specific.get(output_shm_cap);

        self.get_or_publish_deferred_epilogue(cap_id, shm_space)
    }
}

pub fn print_success<T: Serialize>(output_shm_cap: &mut ShmCap, payload: T) {
    let output = DeferredOutput::Success(payload);

    match postcard::to_slice(&output, output_shm_cap.backing_mut()) {
        Ok(_) => {}
        Err(postcard_error) => {
            tracing::debug!("Postcard serialise error: {postcard_error}");
            print_error(output_shm_cap, DeferredError::SerializeError, &postcard_error);
        }
    }
}

pub fn print_error(output_shm_cap: &mut ShmCap, deferred_error: DeferredError, error: &dyn core::fmt::Display) {
    let output = DeferredOutput::Error::<()>(DeferredErrorWithMessage::new(deferred_error, error.to_string()));

    // The below might fail if, for example, the serialize buffer is full. Just
    // do nothing in this case.
    let _ = postcard::to_slice(&output, output_shm_cap.backing_mut());
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
    ShmSpaceInternalError { source: ShmSpaceError }, // Should never occur, indicates a bug in Nushift's code
    GetOrPublishInternalError, // Should never occur, indicates a bug in Nushift's code
}

#[derive(Debug, Serialize)]
enum DeferredOutput<T> {
    Success(T),
    Error(DeferredErrorWithMessage),
}

#[derive(Debug, Serialize)]
struct DeferredErrorWithMessage {
    deferred_error: DeferredError,
    message: String,
}

impl DeferredErrorWithMessage {
    fn new(deferred_error: DeferredError, message: String) -> Self {
        Self { deferred_error, message }
    }
}

#[derive(IntoPrimitive, Debug, Serialize)]
#[repr(u64)]
pub enum DeferredError {
    DeserializeError = 0,
    DeserializeRonError = 1,
    SubmitFailed = 2,
    ExtraInfoNoLongerPresent = 3,
    SerializeError = 4,
    GfxInconsistentPresentBufferLength = 5,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use memmap2::MmapMut;
    use mockall::predicate;

    use crate::shm_space::{acquisitions_and_page_table::PageTableError, CapType, ShmType};

    use super::*;

    #[test]
    fn new_cap_ok() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        assert!(default_deferred_space.space.contains_key(&cap_id));
        assert!(matches!(default_deferred_space.space.get(&cap_id), Some(DefaultDeferredCap { in_progress_cap: None })));
    }

    #[test]
    fn new_cap_exhausted() {
        let mut mock = MockIdPoolBacking::new();
        mock.expect_try_allocate()
            .times(1)
            .returning(|| Err(ReusableIdPoolError::TooManyLiveIDs));

        let mut default_deferred_space = DefaultDeferredSpace::new_with_id_pool(mock);

        assert!(matches!(default_deferred_space.new_cap("test"), Err(DeferredSpaceError::Exhausted { context }) if context == "test"));
    }

    #[test]
    fn new_cap_internal_error_if_duplicate_id() {
        let mut mock = MockIdPoolBacking::new();
        mock.expect_try_allocate()
            .times(2)
            .returning(|| Ok(0));

        let mut default_deferred_space = DefaultDeferredSpace::new_with_id_pool(mock);

        default_deferred_space.new_cap("test").expect("Should succeed");

        // Internal error, there should never be a duplicate ID except when we
        // locked the mock to do so above
        assert!(matches!(default_deferred_space.new_cap("test"), Err(DeferredSpaceError::DuplicateId)));
    }

    #[test]
    fn get_mut_ok() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        assert!(matches!(default_deferred_space.get_mut(cap_id), Some(DefaultDeferredCap { in_progress_cap: None })));
    }

    #[test]
    fn get_mut_not_present() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        assert!(matches!(default_deferred_space.get_mut(0), None));
    }

    #[test]
    fn contains_key_ok() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        assert!(default_deferred_space.contains_key(cap_id));
    }

    #[test]
    fn contains_key_does_not_contain() {
        let default_deferred_space = DefaultDeferredSpace::new();

        assert!(!default_deferred_space.contains_key(0));
    }

    #[test]
    fn destroy_cap_ok() {
        let mut mock = MockIdPoolBacking::new();
        mock.expect_try_allocate()
            .times(1)
            .returning(|| Ok(0));
        mock.expect_release()
            .with(predicate::eq(0))
            .times(1)
            .return_const(());

        let mut default_deferred_space = DefaultDeferredSpace::new_with_id_pool(mock);

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        assert!(matches!(default_deferred_space.destroy_cap("test", cap_id), Ok(())));
        assert!(!default_deferred_space.contains_key(cap_id));
        // Assert that release was called
        default_deferred_space.id_pool.checkpoint();
    }

    #[test]
    fn destroy_cap_cap_not_found_error_if_not_found() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        assert!(matches!(default_deferred_space.destroy_cap("test", 0), Err(DeferredSpaceError::CapNotFound { context, id: 0 }) if context == "test"));
    }

    #[test]
    fn destroy_cap_in_progress_error_if_in_progress() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let output_shm_cap = ShmCap::new(ShmType::FourKiB, NonZeroU64::new(1).expect("Should work"), MmapMut::map_anon(8).expect("Should work"), CapType::AppCap);
        *default_deferred_space.space.get_mut(&cap_id).expect("Should exist") = DefaultDeferredCap { in_progress_cap: Some(InProgressCap::new(None, (0, output_shm_cap))) };

        assert!(matches!(default_deferred_space.destroy_cap("test", cap_id), Err(DeferredSpaceError::InProgress { context }) if context == "test"));
    }

    #[test]
    fn get_or_publish_deferred_prologue_ok_publish() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let output_shm_cap = ShmCap::new(ShmType::FourKiB, NonZeroU64::new(1).expect("Should work"), MmapMut::map_anon(8).expect("Should work"), CapType::AppCap);
        let input_shm_cap = ShmCap::new(ShmType::FourKiB, NonZeroU64::new(2).expect("Should work"), MmapMut::map_anon(8).expect("Should work"), CapType::AppCap);
        *default_deferred_space.space.get_mut(&cap_id).expect("Should exist") = DefaultDeferredCap { in_progress_cap: Some(InProgressCap::new((1, input_shm_cap), (0, output_shm_cap))) };

        assert!(matches!(default_deferred_space.get_or_publish_deferred_prologue(cap_id), PrologueReturn::ContinueCapsPublish(_, _)));
    }

    #[test]
    fn get_or_publish_deferred_prologue_ok_get() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let output_shm_cap = ShmCap::new(ShmType::FourKiB, NonZeroU64::new(1).expect("Should work"), MmapMut::map_anon(8).expect("Should work"), CapType::AppCap);
        *default_deferred_space.space.get_mut(&cap_id).expect("Should exist") = DefaultDeferredCap { in_progress_cap: Some(InProgressCap::new(None, (0, output_shm_cap))) };

        assert!(matches!(default_deferred_space.get_or_publish_deferred_prologue(cap_id), PrologueReturn::ContinueCapsGet(_)));
    }

    #[test]
    fn get_or_publish_deferred_epilogue_ok_publish() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();
        let (input_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");

        let input_shm_cap = shm_space.move_shm_cap_to_other_space(input_shm_cap_id).expect("Should succeed");
        let output_shm_cap = shm_space.move_shm_cap_to_other_space(output_shm_cap_id).expect("Should succeed");

        *default_deferred_space.space.get_mut(&cap_id).expect("Should exist") = DefaultDeferredCap { in_progress_cap: Some(InProgressCap::new((input_shm_cap_id, input_shm_cap), (output_shm_cap_id, output_shm_cap))) };

        assert!(matches!(default_deferred_space.get_or_publish_deferred_epilogue(cap_id, &mut shm_space), Ok(())));
        // Assert they were moved back into space
        assert!(matches!(shm_space.get_shm_cap_app(input_shm_cap_id), Ok(_)));
        assert!(matches!(shm_space.get_shm_cap_app(output_shm_cap_id), Ok(_)));
    }

    #[test]
    fn get_or_publish_deferred_epilogue_ok_get() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");

        let output_shm_cap = shm_space.move_shm_cap_to_other_space(output_shm_cap_id).expect("Should succeed");

        *default_deferred_space.space.get_mut(&cap_id).expect("Should exist") = DefaultDeferredCap { in_progress_cap: Some(InProgressCap::new(None, (output_shm_cap_id, output_shm_cap))) };

        assert!(matches!(default_deferred_space.get_or_publish_deferred_epilogue(cap_id, &mut shm_space), Ok(())));
        // Assert it was moved back into space
        assert!(matches!(shm_space.get_shm_cap_app(output_shm_cap_id), Ok(_)));
    }

    #[test]
    fn get_or_publish_deferred_epilogue_internal_error_does_not_exist() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let mut shm_space = ShmSpace::new();

        assert!(matches!(default_deferred_space.get_or_publish_deferred_epilogue(0, &mut shm_space), Err(())));
    }

    #[test]
    fn get_or_publish_deferred_epilogue_internal_error_not_in_progress() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        *default_deferred_space.space.get_mut(&cap_id).expect("Should exist") = DefaultDeferredCap { in_progress_cap: None };

        let mut shm_space = ShmSpace::new();

        assert!(matches!(default_deferred_space.get_or_publish_deferred_epilogue(cap_id, &mut shm_space), Err(())));
    }

    #[test]
    fn publish_blocking_ok() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();
        let (input_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");
        shm_space.acquire_shm_cap_app(input_shm_cap_id, 0x1000).expect("Should succeed");
        shm_space.acquire_shm_cap_app(output_shm_cap_id, 0x2000).expect("Should succeed");

        // Assert publish_blocking succeeds
        assert!(matches!(default_deferred_space.publish_blocking("test", cap_id, input_shm_cap_id, output_shm_cap_id, &mut shm_space), Ok(())));

        // Assert caps released
        assert!(matches!(shm_space.walk(0x1000), Err(PageTableError::PageNotFound)));
        assert!(matches!(shm_space.walk(0x2000), Err(PageTableError::PageNotFound)));

        // Assert caps moved out
        assert!(matches!(shm_space.get_shm_cap_app(input_shm_cap_id), Err(ShmSpaceError::CapNotFound)));
        assert!(matches!(shm_space.get_shm_cap_app(output_shm_cap_id), Err(ShmSpaceError::CapNotFound)));
        assert!(matches!(
            default_deferred_space.space.get(&cap_id),
            Some(DefaultDeferredCap { in_progress_cap: Some(InProgressCap { input: Some((m_input_shm_cap_id, _)), output: (m_output_shm_cap_id, _) }) }) if *m_input_shm_cap_id == input_shm_cap_id && *m_output_shm_cap_id == output_shm_cap_id,
        ));
    }

    #[test]
    fn publish_blocking_ok_already_released() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();
        let (input_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");

        // Assert publish_blocking succeeds even though caps not acquired
        assert!(matches!(default_deferred_space.publish_blocking("test", cap_id, input_shm_cap_id, output_shm_cap_id, &mut shm_space), Ok(())));

        // Assert caps moved out
        assert!(matches!(shm_space.get_shm_cap_app(input_shm_cap_id), Err(ShmSpaceError::CapNotFound)));
        assert!(matches!(shm_space.get_shm_cap_app(output_shm_cap_id), Err(ShmSpaceError::CapNotFound)));
        assert!(matches!(
            default_deferred_space.space.get(&cap_id),
            Some(DefaultDeferredCap { in_progress_cap: Some(InProgressCap { input: Some((m_input_shm_cap_id, _)), output: (m_output_shm_cap_id, _) }) }) if *m_input_shm_cap_id == input_shm_cap_id && *m_output_shm_cap_id == output_shm_cap_id,
        ));
    }

    #[test]
    fn get_blocking_ok() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");
        shm_space.acquire_shm_cap_app(output_shm_cap_id, 0x2000).expect("Should succeed");

        // Assert get_blocking succeeds
        assert!(matches!(default_deferred_space.get_blocking("test", cap_id, output_shm_cap_id, &mut shm_space), Ok(())));

        // Assert cap released
        assert!(matches!(shm_space.walk(0x2000), Err(PageTableError::PageNotFound)));

        // Assert cap moved out
        assert!(matches!(shm_space.get_shm_cap_app(output_shm_cap_id), Err(ShmSpaceError::CapNotFound)));
        assert!(matches!(
            default_deferred_space.space.get(&cap_id),
            Some(DefaultDeferredCap { in_progress_cap: Some(InProgressCap { input: None, output: (m_output_shm_cap_id, _) }) }) if *m_output_shm_cap_id == output_shm_cap_id,
        ));
    }

    #[test]
    fn get_blocking_ok_already_released() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();
        let (output_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");

        // Assert get_blocking succeeds even though cap not acquired
        assert!(matches!(default_deferred_space.get_blocking("test", cap_id, output_shm_cap_id, &mut shm_space), Ok(())));

        // Assert cap moved out
        assert!(matches!(shm_space.get_shm_cap_app(output_shm_cap_id), Err(ShmSpaceError::CapNotFound)));
        assert!(matches!(
            default_deferred_space.space.get(&cap_id),
            Some(DefaultDeferredCap { in_progress_cap: Some(InProgressCap { input: None, output: (m_output_shm_cap_id, _) }) }) if *m_output_shm_cap_id == output_shm_cap_id,
        ));
    }

    #[test]
    fn publish_blocking_shm_caps_invalid() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();

        // Assert publish_blocking fails, when provided with invalid SHM cap IDs
        assert!(matches!(default_deferred_space.publish_blocking("test", cap_id, 123, 456, &mut shm_space), Err(DeferredSpaceError::ShmCapNotFound { id }) if id == 123));
    }

    #[test]
    fn publish_blocking_output_cap_invalid_rolls_back() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();
        let (input_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");
        shm_space.acquire_shm_cap_app(input_shm_cap_id, 0x1000).expect("Should succeed");

        // Assert publish_blocking fails, when provided with invalid output SHM cap ID
        assert!(matches!(default_deferred_space.publish_blocking("test", cap_id, input_shm_cap_id, 456, &mut shm_space), Err(DeferredSpaceError::ShmCapNotFound { id }) if id == 456));

        // Assert input cap is still acquired, i.e. the release that temporarily occurred was rolled back.
        assert!(matches!(shm_space.walk(0x1000), Ok(_)));
    }

    #[test]
    fn publish_blocking_output_cap_invalid_rolls_back_noop() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let mut shm_space = ShmSpace::new();
        let (input_shm_cap_id, _) = shm_space.new_shm_cap(ShmType::FourKiB, 1, CapType::AppCap).expect("Should succeed");

        // Assert publish_blocking fails with the expected error that the output
        // cap was not found. As the input cap was not acquired, there is no
        // input cap release to roll back.
        assert!(matches!(default_deferred_space.publish_blocking("test", cap_id, input_shm_cap_id, 456, &mut shm_space), Err(DeferredSpaceError::ShmCapNotFound { id }) if id == 456));
    }

    #[test]
    fn get_or_publish_blocking_error_if_not_found() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let mut shm_space = ShmSpace::new();

        assert!(matches!(default_deferred_space.get_or_publish_blocking("test", 0, Some(123), 456, &mut shm_space), Err(DeferredSpaceError::CapNotFound { context, id }) if context == "test" && id == 0));
    }

    #[test]
    fn get_or_publish_blocking_error_if_in_progress() {
        let mut default_deferred_space = DefaultDeferredSpace::new();

        let cap_id = default_deferred_space.new_cap("test").expect("Should succeed");

        let output_shm_cap = ShmCap::new(ShmType::FourKiB, NonZeroU64::new(1).expect("Should work"), MmapMut::map_anon(8).expect("Should work"), CapType::AppCap);
        *default_deferred_space.space.get_mut(&cap_id).expect("Should exist") = DefaultDeferredCap { in_progress_cap: Some(InProgressCap::new(None, (0, output_shm_cap))) };

        let mut shm_space = ShmSpace::new();

        assert!(matches!(default_deferred_space.get_or_publish_blocking("test", 0, Some(123), 456, &mut shm_space), Err(DeferredSpaceError::InProgress { context }) if context == "test"));
    }
}
