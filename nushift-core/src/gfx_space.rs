// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use num_cmp::NumCmp;
use num_enum::{TryFromPrimitive, TryFromPrimitiveError, IntoPrimitive};
use postcard::Error as PostcardError;
use serde::{Deserialize, Serialize};
use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use crate::deferred_space::{self, DefaultDeferredSpace, DeferredSpace, DeferredSpaceError, DeferredSpaceGet, DefaultDeferredSpaceCapId, DeferredSpacePublish, DeferredError};
use crate::hypervisor::hypervisor_event::{UnboundHypervisorEvent, HypervisorEventError};
use crate::hypervisor::tab_context::TabContext;
use crate::shm_space::{ShmCap, ShmCapId, ShmSpace, ShmSpaceError};

pub type GfxCapId = u64;
pub type GfxCpuPresentBufferCapId = u64;
const GFX_CONTEXT: &str = "gfx";
const GFX_CPU_PRESENT_CONTEXT: &str = "gfx_cpu_present";

pub struct GfxSpace {
    root_deferred_space: DefaultDeferredSpace,
    root_tree: HashMap<GfxCapId, HashSet<GfxCpuPresentBufferCapId>>,
    cpu_present_buffer_deferred_space: DefaultDeferredSpace,
    get_outputs: GetOutputs,
    cpu_present: CpuPresent,
}

#[derive(Debug, Clone, Serialize)]
pub struct GfxOutput {
    id: u64,
    size_px: Vec<u64>,
    scale: Vec<f64>,
}

impl GfxOutput {
    pub fn new(id: u64, size_px: Vec<u64>, scale: Vec<f64>) -> Self {
        Self { id, size_px, scale }
    }

    pub fn size_px(&self) -> &Vec<u64> {
        &self.size_px
    }

    pub fn scale(&self) -> &Vec<f64> {
        &self.scale
    }
}

struct GetOutputs {
    tab_context: Arc<dyn TabContext>,
}

impl DeferredSpaceGet for GetOutputs {
    fn get(&mut self, output_shm_cap: &mut ShmCap) {
        let gfx_outputs = self.tab_context.get_gfx_outputs();
        let gfx_outputs_dereferenced: Vec<&GfxOutput> = gfx_outputs.iter().map(|guard| &**guard).collect();

        deferred_space::print_success(output_shm_cap, gfx_outputs_dereferenced);
    }
}

impl GetOutputs {
    fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self { tab_context }
    }
}

#[derive(TryFromPrimitive, IntoPrimitive, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum PresentBufferFormat {
    R8g8b8UintSrgb = 0,
}

impl PresentBufferFormat {
    fn bytes_per_pixel(&self) -> u8 {
        match self {
            Self::R8g8b8UintSrgb => 3,
        }
    }
}

struct CpuPresentBufferInfo {
    parent_gfx_cap_id: GfxCapId,
    present_buffer_format: PresentBufferFormat,
    present_buffer_size_px: Vec<u64>,
    present_buffer_shm_cap_id: ShmCapId,
}

#[derive(Deserialize)]
struct CpuPresentBufferArgs {
    present_buffer_format: u64,
    present_buffer_size_px: Vec<u64>,
    present_buffer_shm_cap_id: ShmCapId,
}

struct CpuPresent {
    tab_context: Arc<dyn TabContext>,
    space: HashMap<DefaultDeferredSpaceCapId, CpuPresentBufferInfo>,
}

impl DeferredSpacePublish for CpuPresent {
    type Payload<'de> = &'de [u8];

    fn publish_cap_payload(&mut self, payload: Self::Payload<'_>, output_shm_cap: &mut ShmCap, cap_id: u64) {
        let Some(cpu_present_buffer_info) = self.get_info(cap_id) else {
            let error_message = format!("Extra info no longer present. gfx_cpu_present_buffer_cap_id: {cap_id}");
            tracing::debug!(error_message);
            deferred_space::print_error(output_shm_cap, DeferredError::ExtraInfoNoLongerPresent, &error_message);
            return;
        };

        let dimensions_product_format_bytes = cpu_present_buffer_info.present_buffer_size_px
            .iter()
            .try_fold(1u64, |number_acc, &elem| number_acc.checked_mul(elem))
            .and_then(|dimensions_product| dimensions_product.checked_mul(cpu_present_buffer_info.present_buffer_format.bytes_per_pixel().into()));

        if !matches!(dimensions_product_format_bytes, Some(num) if num.num_eq(payload.len())) {
            let error_message = "The present buffer length was not consistent with the buffer dimensions and format. The length should be the bytes per pixel of the format multiplied by the product of the dimensions.";
            tracing::debug!(error_message);
            deferred_space::print_error(output_shm_cap, DeferredError::GfxInconsistentPresentBufferLength, &error_message);
            return;
        }

        match self.tab_context.send_hypervisor_event(UnboundHypervisorEvent::GfxCpuPresent(cpu_present_buffer_info.present_buffer_format, cpu_present_buffer_info.present_buffer_size_px.clone(), payload.into())) {
            Ok(_) => deferred_space::print_success(output_shm_cap, ()),

            Err(hypervisor_event_error) => match hypervisor_event_error {
                HypervisorEventError::SubmitCommandError => {
                    tracing::debug!("Submit failed: {hypervisor_event_error}");
                    deferred_space::print_error(output_shm_cap, DeferredError::SubmitFailed, &hypervisor_event_error);
                },
            },
        }
    }
}

impl CpuPresent {
    fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self { tab_context, space: HashMap::new() }
    }

    fn add_info(&mut self, cap_id: GfxCpuPresentBufferCapId, parent_gfx_cap_id: GfxCapId, present_buffer_format: PresentBufferFormat, present_buffer_size_px: Vec<u64>, present_buffer_shm_cap_id: ShmCapId) {
        self.space.insert(cap_id, CpuPresentBufferInfo { parent_gfx_cap_id, present_buffer_format, present_buffer_size_px, present_buffer_shm_cap_id });
    }

    fn get_info(&self, cap_id: GfxCpuPresentBufferCapId) -> Option<&CpuPresentBufferInfo> {
        self.space.get(&cap_id)
    }

    fn remove_info(&mut self, cap_id: GfxCpuPresentBufferCapId) -> Option<CpuPresentBufferInfo> {
        self.space.remove(&cap_id)
    }
}

impl GfxSpace {
    pub(crate) fn new(tab_context: Arc<dyn TabContext>) -> Self {
        Self {
            root_deferred_space: DefaultDeferredSpace::new(),
            root_tree: HashMap::new(),
            cpu_present_buffer_deferred_space: DefaultDeferredSpace::new(),
            get_outputs: GetOutputs::new(Arc::clone(&tab_context)),
            cpu_present: CpuPresent::new(Arc::clone(&tab_context)),
        }
    }

    pub fn new_gfx_cap(&mut self) -> Result<GfxCapId, GfxSpaceError> {
        self.root_deferred_space.new_cap(GFX_CONTEXT).context(DeferredSpaceSnafu)
    }

    pub fn get_outputs_blocking(&mut self, gfx_cap_id: GfxCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), GfxSpaceError> {
        self.root_deferred_space.get_blocking(GFX_CONTEXT, gfx_cap_id, output_shm_cap_id, shm_space).context(DeferredSpaceSnafu)
    }

    pub fn get_outputs_deferred(&mut self, gfx_cap_id: GfxCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.root_deferred_space.get_deferred(&mut self.get_outputs, gfx_cap_id, shm_space)
    }

    pub fn new_gfx_cpu_present_buffer_cap(&mut self, gfx_cap_id: GfxCapId, input_shm_cap_id: ShmCapId, shm_space: &ShmSpace) -> Result<GfxCpuPresentBufferCapId, GfxSpaceError> {
        // Get input SHM cap for parsing arguments
        let input_shm_cap = shm_space.get_shm_cap_app(input_shm_cap_id).map_err(|shm_space_error| match shm_space_error {
            ShmSpaceError::CapNotFound => ShmCapNotFoundSnafu { id: input_shm_cap_id }.build(),
            ShmSpaceError::PermissionDenied => ShmPermissionDeniedSnafu { id: input_shm_cap_id }.build(),
            _ => ShmUnexpectedSnafu.build(),
        })?;

        // Parse input SHM cap arguments
        let cpu_present_buffer_args = postcard::from_bytes(input_shm_cap.backing()).context(DeserializeCpuPresentBufferArgsSnafu)?;

        // Do rest of logic
        self.new_gfx_cpu_present_buffer_cap_impl(gfx_cap_id, cpu_present_buffer_args)
    }

    /// Separated `_impl` function for unit tests
    fn new_gfx_cpu_present_buffer_cap_impl(&mut self, gfx_cap_id: GfxCapId, cpu_present_buffer_args: CpuPresentBufferArgs) -> Result<GfxCpuPresentBufferCapId, GfxSpaceError> {
        // Parse present buffer format
        let present_buffer_format = PresentBufferFormat::try_from(cpu_present_buffer_args.present_buffer_format).context(UnknownPresentBufferFormatSnafu)?;

        // Check that gfx_cap_id is a valid cap
        self.root_deferred_space.contains_key(gfx_cap_id).then_some(()).ok_or_else(|| DeferredSpaceError::CapNotFound { context: GFX_CONTEXT.into(), id: gfx_cap_id }).context(DeferredSpaceSnafu)?;

        // Create the cpu present cap
        let gfx_cpu_present_buffer_cap_id = self.cpu_present_buffer_deferred_space.new_cap(GFX_CPU_PRESENT_CONTEXT).context(DeferredSpaceSnafu)?;

        // Store the additional info
        self.cpu_present.add_info(gfx_cpu_present_buffer_cap_id, gfx_cap_id, present_buffer_format, cpu_present_buffer_args.present_buffer_size_px, cpu_present_buffer_args.present_buffer_shm_cap_id);

        // Store tree-child association
        self.root_tree.entry(gfx_cap_id).or_default().insert(gfx_cpu_present_buffer_cap_id);

        Ok(gfx_cpu_present_buffer_cap_id)
    }

    pub fn cpu_present_blocking(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId, output_shm_cap_id: ShmCapId, shm_space: &mut ShmSpace) -> Result<(), GfxSpaceError> {
        let cpu_present_buffer_info = self.cpu_present.get_info(gfx_cpu_present_buffer_cap_id)
            .ok_or_else(|| DeferredSpaceError::CapNotFound { context: GFX_CPU_PRESENT_CONTEXT.into(), id: gfx_cpu_present_buffer_cap_id })
            .context(DeferredSpaceSnafu)?;

        self.cpu_present_buffer_deferred_space.publish_blocking(GFX_CPU_PRESENT_CONTEXT, gfx_cpu_present_buffer_cap_id, cpu_present_buffer_info.present_buffer_shm_cap_id, output_shm_cap_id, shm_space).context(DeferredSpaceSnafu)
    }

    pub fn cpu_present_deferred(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId, shm_space: &mut ShmSpace) -> Result<(), ()> {
        self.cpu_present_buffer_deferred_space.publish_deferred(&mut self.cpu_present, gfx_cpu_present_buffer_cap_id, shm_space)
    }

    pub fn destroy_gfx_cpu_present_buffer_cap(&mut self, gfx_cpu_present_buffer_cap_id: GfxCpuPresentBufferCapId) -> Result<(), GfxSpaceError> {
        let cpu_present_buffer_info = self.cpu_present.remove_info(gfx_cpu_present_buffer_cap_id).ok_or_else(|| DeferredSpaceError::CapNotFound { context: GFX_CPU_PRESENT_CONTEXT.into(), id: gfx_cpu_present_buffer_cap_id }).context(DeferredSpaceSnafu)?;
        let all_succeeded = drop_guard::guard(false, |all_succ| {
            if !all_succ {
                self.cpu_present.add_info(gfx_cpu_present_buffer_cap_id, cpu_present_buffer_info.parent_gfx_cap_id, cpu_present_buffer_info.present_buffer_format, cpu_present_buffer_info.present_buffer_size_px.clone(), cpu_present_buffer_info.present_buffer_shm_cap_id);
            }
        });

        // Remove tree-child association. The association is present because the
        // parent cap is not allowed to be destroyed while the child cap is
        // alive.
        self.root_tree.entry(cpu_present_buffer_info.parent_gfx_cap_id).or_default().remove(&gfx_cpu_present_buffer_cap_id);
        let mut all_succeeded = drop_guard::guard(all_succeeded, |all_succ| {
            if !*all_succ {
                self.root_tree.entry(cpu_present_buffer_info.parent_gfx_cap_id).or_default().insert(gfx_cpu_present_buffer_cap_id);
            }
        });

        self.cpu_present_buffer_deferred_space.destroy_cap(GFX_CPU_PRESENT_CONTEXT, gfx_cpu_present_buffer_cap_id).context(DeferredSpaceSnafu)?;

        **all_succeeded = true;
        Ok(())
    }

    pub fn destroy_gfx_cap(&mut self, gfx_cap_id: GfxCapId) -> Result<(), GfxSpaceError> {
        // Check that gfx_cap_id is a valid cap. While destroy_cap does do this
        // check, we want to have it before the root_tree check.
        self.root_deferred_space.contains_key(gfx_cap_id).then_some(()).ok_or_else(|| DeferredSpaceError::CapNotFound { context: GFX_CONTEXT.into(), id: gfx_cap_id }).context(DeferredSpaceSnafu)?;

        // If this root cap has children, you are not allowed to destroy it.
        let children = self.root_tree.entry(gfx_cap_id).or_default();
        if !children.is_empty() {
            return ChildCapsNotDestroyedSnafu { gfx_cap_id, children: children.clone() }.fail();
        }

        self.root_deferred_space.destroy_cap(GFX_CONTEXT, gfx_cap_id).context(DeferredSpaceSnafu)
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum GfxSpaceError {
    DeferredSpaceError { source: DeferredSpaceError },
    #[snafu(display("Not all CPU present buffer children caps were destroyed when trying to destroy this gfx_cap_id: {gfx_cap_id}. Please destroy its CPU present buffer children first: {children:?}"))]
    ChildCapsNotDestroyed { gfx_cap_id: GfxCapId, children: HashSet<GfxCpuPresentBufferCapId> },
    #[snafu(display("Could not deserialise the CPU present buffer args in input_shm_cap_id: {source}"))]
    DeserializeCpuPresentBufferArgsError { source: PostcardError },
    #[snafu(display("The value provided for the PresentBufferFormat enum was unrecognised."))]
    UnknownPresentBufferFormat { source: TryFromPrimitiveError<PresentBufferFormat> },
    #[snafu(display("The SHM cap with ID {id} was not found."))]
    ShmCapNotFound { id: ShmCapId },
    #[snafu(display("The SHM cap with ID {id} is not allowed to be used as an input cap, possibly because it is an ELF cap."))]
    ShmPermissionDenied { id: ShmCapId },
    ShmUnexpectedError,
}

#[cfg(test)]
mod tests {
    use std::sync::MutexGuard;

    use super::*;

    struct MockTabContext;
    impl TabContext for MockTabContext {
        fn send_hypervisor_event(&self, _unbound_hypervisor_event: UnboundHypervisorEvent) -> Result<(), HypervisorEventError> {
            unimplemented!("This is a mock, this method is not expected to be called")
        }

        fn get_gfx_outputs(&self) -> Vec<MutexGuard<'_, GfxOutput>> {
            unimplemented!("This is a mock, this method is not expected to be called")
        }
    }

    #[test]
    fn new_root_and_children_and_destroy_all_is_allowed() {
        let mut gfx_space = GfxSpace::new(Arc::new(MockTabContext));

        let gfx_cap_id = gfx_space.new_gfx_cap().expect("Should succeed");

        let gfx_cpu_present_buffer_cap_id_1 = gfx_space.new_gfx_cpu_present_buffer_cap_impl(gfx_cap_id, CpuPresentBufferArgs { present_buffer_format: PresentBufferFormat::R8g8b8UintSrgb.into(), present_buffer_size_px: vec![], present_buffer_shm_cap_id: 0 }).expect("Should succeed");
        let gfx_cpu_present_buffer_cap_id_2 = gfx_space.new_gfx_cpu_present_buffer_cap_impl(gfx_cap_id, CpuPresentBufferArgs { present_buffer_format: PresentBufferFormat::R8g8b8UintSrgb.into(), present_buffer_size_px: vec![], present_buffer_shm_cap_id: 1 }).expect("Should succeed");

        // Destroying children first should work
        gfx_space.destroy_gfx_cpu_present_buffer_cap(gfx_cpu_present_buffer_cap_id_1).expect("Should succeed");
        gfx_space.destroy_gfx_cpu_present_buffer_cap(gfx_cpu_present_buffer_cap_id_2).expect("Should succeed");

        // Now destroying the root should work
        gfx_space.destroy_gfx_cap(gfx_cap_id).expect("Should succeed");
    }

    #[test]
    fn destroying_root_before_destroying_children_is_not_allowed() {
        let mut gfx_space = GfxSpace::new(Arc::new(MockTabContext));

        let gfx_cap_id = gfx_space.new_gfx_cap().expect("Should succeed");

        let gfx_cpu_present_buffer_cap_id_1 = gfx_space.new_gfx_cpu_present_buffer_cap_impl(gfx_cap_id, CpuPresentBufferArgs { present_buffer_format: PresentBufferFormat::R8g8b8UintSrgb.into(), present_buffer_size_px: vec![], present_buffer_shm_cap_id: 0 }).expect("Should succeed");
        let gfx_cpu_present_buffer_cap_id_2 = gfx_space.new_gfx_cpu_present_buffer_cap_impl(gfx_cap_id, CpuPresentBufferArgs { present_buffer_format: PresentBufferFormat::R8g8b8UintSrgb.into(), present_buffer_size_px: vec![], present_buffer_shm_cap_id: 1 }).expect("Should succeed");

        // Destroy only one child
        gfx_space.destroy_gfx_cpu_present_buffer_cap(gfx_cpu_present_buffer_cap_id_1).expect("Should succeed");

        // Now destroying the root should fail
        let mut expected_remaining_children = HashSet::new();
        expected_remaining_children.insert(gfx_cpu_present_buffer_cap_id_2);
        assert!(matches!(gfx_space.destroy_gfx_cap(gfx_cap_id), Err(GfxSpaceError::ChildCapsNotDestroyed { gfx_cap_id: m_gfx_cap_id, children }) if m_gfx_cap_id == gfx_cap_id && children == expected_remaining_children));
    }

    #[test]
    fn destroy_gfx_cpu_present_buffer_cap_returns_error_when_not_found() {
        let mut gfx_space = GfxSpace::new(Arc::new(MockTabContext));

        assert!(matches!(gfx_space.destroy_gfx_cpu_present_buffer_cap(0), Err(GfxSpaceError::DeferredSpaceError { source: DeferredSpaceError::CapNotFound { id: 0, .. } })));
    }

    #[test]
    fn destroy_gfx_cap_returns_error_when_not_found() {
        let mut gfx_space = GfxSpace::new(Arc::new(MockTabContext));

        assert!(matches!(gfx_space.destroy_gfx_cap(0), Err(GfxSpaceError::DeferredSpaceError { source: DeferredSpaceError::CapNotFound { id: 0, .. } })));
    }
}
