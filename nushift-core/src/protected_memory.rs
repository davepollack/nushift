// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use core::mem;

use snafu::prelude::*;
use snafu_cli_debug::SnafuCliDebug;

use super::shm_space::{ShmSpace, SV39_BITS, acquisitions_and_page_table::{PageTableError, WalkResult}};

pub struct ProtectedMemory;

impl ProtectedMemory {
    fn check_within_sv39(addr: u64, word_bytes: usize) -> Result<(), ProtectedMemoryError> {
        if addr > (1 << SV39_BITS) - (word_bytes as u64) {
            OutOfSv39Snafu.fail()
        } else {
            Ok(())
        }
    }

    pub fn load8(shm_space: &ShmSpace, addr: u64) -> Result<u8, ProtectedMemoryError> {
        Self::check_within_sv39(addr, 1)?;
        let walked = shm_space.walk(addr).context(WalkSnafu)?;
        Ok(walked.space_slice[walked.byte_offset_in_space_slice])
    }

    pub fn load16(shm_space: &ShmSpace, addr: u64) -> Result<u16, ProtectedMemoryError> {
        Self::load_multi_byte(shm_space, addr, ShmSpace::walk)
    }

    pub fn load32(shm_space: &ShmSpace, addr: u64) -> Result<u32, ProtectedMemoryError> {
        Self::load_multi_byte(shm_space, addr, ShmSpace::walk)
    }

    pub fn load64(shm_space: &ShmSpace, addr: u64) -> Result<u64, ProtectedMemoryError> {
        Self::load_multi_byte(shm_space, addr, ShmSpace::walk)
    }

    pub fn execute_load16(shm_space: &ShmSpace, addr: u64) -> Result<u16, ProtectedMemoryError> {
        Self::load_multi_byte(shm_space, addr, ShmSpace::walk_execute)
    }

    pub fn execute_load32(shm_space: &ShmSpace, addr: u64) -> Result<u32, ProtectedMemoryError> {
        Self::load_multi_byte(shm_space, addr, ShmSpace::walk_execute)
    }

    fn load_multi_byte<T, W, const N: usize>(shm_space: &ShmSpace, addr: u64, walk: W) -> Result<T, ProtectedMemoryError>
    where
        T: Numeric<N>,
        W: Fn(&ShmSpace, u64) -> Result<WalkResult<'_>, PageTableError>,
    {
        Self::check_within_sv39(addr, N)?;
        let walked = walk(shm_space, addr).context(WalkSnafu)?;

        // Fits in page (all aligned accesses are this, and some unaligned accesses).
        let diff_to_end_of_space_slice = walked.space_slice.len() - walked.byte_offset_in_space_slice;
        if diff_to_end_of_space_slice >= N {
            let bytes = walked.space_slice[walked.byte_offset_in_space_slice..(walked.byte_offset_in_space_slice + N)].try_into().unwrap();
            let word = T::from_le_bytes(bytes);
            return Ok(word);
        }

        // Goes onto next page. Not common, only unaligned accesses can do this.

        // Doesn't overflow, and is a valid argument to `walk`, because we
        // called `check_within_sv39` at the beginning.
        //
        // Casting to u64 is OK because a page size can't be more than u64 as
        // long as `addr` is still u64.
        let next_page = addr + (diff_to_end_of_space_slice as u64);
        let walked_next_page = walk(shm_space, next_page).context(WalkSnafu)?;

        let bytes = [&walked.space_slice[walked.byte_offset_in_space_slice..], &walked_next_page.space_slice[..(N - diff_to_end_of_space_slice)]]
            .concat().try_into().unwrap();
        let word = T::from_le_bytes(bytes);
        Ok(word)
    }

    pub fn store8(shm_space: &mut ShmSpace, addr: u64, value: u8) -> Result<(), ProtectedMemoryError> {
        Self::check_within_sv39(addr, 1)?;
        let walked_mut = shm_space.walk_mut(addr).context(WalkSnafu)?;
        walked_mut.space_slice[walked_mut.byte_offset_in_space_slice] = value;
        Ok(())
    }

    pub fn store16(shm_space: &mut ShmSpace, addr: u64, value: u16) -> Result<(), ProtectedMemoryError> {
        Self::store_multi_byte(shm_space, addr, value)
    }

    pub fn store32(shm_space: &mut ShmSpace, addr: u64, value: u32) -> Result<(), ProtectedMemoryError> {
        Self::store_multi_byte(shm_space, addr, value)
    }

    pub fn store64(shm_space: &mut ShmSpace, addr: u64, value: u64) -> Result<(), ProtectedMemoryError> {
        Self::store_multi_byte(shm_space, addr, value)
    }

    fn store_multi_byte<T, const N: usize>(shm_space: &mut ShmSpace, addr: u64, value: T) -> Result<(), ProtectedMemoryError>
    where
        T: Numeric<N>,
    {
        Self::check_within_sv39(addr, N)?;
        let le_bytes = value.to_le_bytes();

        // Non-generic inner function. We can't quite do this with
        // `load_multi_byte`, because there we can't call `from_le_bytes` at the
        // beginning of the routine like we can with `to_le_bytes` here.
        fn inner(shm_space: &mut ShmSpace, addr: u64, le_bytes_slice: &[u8], word_bytes: usize) -> Result<(), ProtectedMemoryError> {
            let walked_mut = shm_space.walk_mut(addr).context(WalkSnafu)?;

            // Fits in page (all aligned accesses are this, and some unaligned accesses).
            let diff_to_end_of_space_slice = walked_mut.space_slice.len() - walked_mut.byte_offset_in_space_slice;
            if diff_to_end_of_space_slice >= word_bytes {
                walked_mut.space_slice[walked_mut.byte_offset_in_space_slice..walked_mut.byte_offset_in_space_slice+word_bytes]
                    .copy_from_slice(le_bytes_slice);
                return Ok(());
            }

            // Goes onto next page. Not common, only unaligned accesses can do this.

            // Doesn't overflow, and is a valid argument to `walk_mut`, because we
            // called `check_within_sv39` at the beginning.
            //
            // Casting to u64 is OK because a page size can't be more than u64 as
            // long as `addr` is still u64.
            let next_page = addr + (diff_to_end_of_space_slice as u64);

            // Check walking of the next page is OK. After this point, all
            // operations will be infallible.
            //
            // We have to do it this way because we can't save the results from
            // both walks like we did in `load_multi_byte` because we can't
            // mutably borrow at the same time, and we can't just do a write to
            // the first page and then do a fallible walk of the next page
            // because that might fail resulting in a partial write. So we have
            // to do it this way.
            shm_space.walk_mut(next_page).context(WalkSnafu)?;

            // Infallible because it succeeded before
            let walked_mut = shm_space.walk_mut(addr).expect("First page walk: infallible because it succeeded before");
            // Write first part
            walked_mut.space_slice[walked_mut.byte_offset_in_space_slice..].copy_from_slice(&le_bytes_slice[..diff_to_end_of_space_slice]);

            // Infallible because it succeeded before
            let walked_next_page_mut = shm_space.walk_mut(next_page).expect("Next page walk: infallible because it succeeded before");
            // Write second part
            walked_next_page_mut.space_slice[..(word_bytes - diff_to_end_of_space_slice)].copy_from_slice(&le_bytes_slice[diff_to_end_of_space_slice..]);

            Ok(())
        }
        inner(shm_space, addr, &le_bytes, N)?;
        Ok(())
    }
}

#[derive(Snafu, SnafuCliDebug)]
pub enum ProtectedMemoryError {
    OutOfSv39,
    WalkError { source: PageTableError },
}

/// Alternatively, could use the `funty` crate for this.
trait Numeric<const N: usize> {
    fn from_le_bytes(bytes: [u8; N]) -> Self;
    fn to_le_bytes(self) -> [u8; N];
}
macro_rules! impl_numeric {
    ($($t:ty),+) => { $(
        impl Numeric<{mem::size_of::<Self>()}> for $t {
            fn from_le_bytes(bytes: [u8; mem::size_of::<Self>()]) -> Self { Self::from_le_bytes(bytes) }
            fn to_le_bytes(self) -> [u8; mem::size_of::<Self>()] { self.to_le_bytes() }
        }
    )+ };
}
impl_numeric!(u16, u32, u64);
