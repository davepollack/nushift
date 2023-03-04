use ckb_vm::{SupportMachine, Syscalls, Error as CKBVMError, registers::{A0, A1}, Register, DefaultMachine, CoreMachine};
use num_enum::TryFromPrimitive;
use reusable_id_pool::{ReusableIdPoolError, ReusableIdPoolManual};
use riscy_emulator::{
    subsystem::{Subsystem, SubsystemAction},
    machine::{RiscvMachine, RiscvMachineError},
};
use snafu::Snafu;
use snafu_cli_debug::SnafuCliDebug;
use std::{convert::TryFrom, collections::{BTreeMap, btree_map::Entry}};

use crate::process_control_block::ProcessControlBlock;

// Regarding the use of `u64`s in this file:
//
// It is intended that riscv32 should use the same numbers. I don't want to have
// duplicate enums/duplicate code for riscv32 apps.
//
// My idea is that riscv32 apps can encode the numbers in a multi-register
// encoding. But at the same time, I don't want that to use as many as 3
// registers, so these numbers should be u63, not u64.
//
// (Technically, 2 registers can encode slighly more than a u63, but a u63
// fits.)

#[derive(TryFromPrimitive)]
#[repr(u64)]
enum Syscall {
    Exit = 0,
    ShmNew = 1,
    ShmAcquire = 2,
    ShmRelease = 3,
    ShmDestroy = 4,
}

#[derive(TryFromPrimitive)]
#[repr(u64)]
enum ShmType {
    FourKiB = 0,
    TwoMiB = 1,
    FourMiB = 2,
    OneGiB = 3,
    FiveTwelveGiB = 4,
}

struct ShmCap {
    shm_type: ShmType,
}

impl ShmCap {
    fn new(shm_type: ShmType) -> Self {
        ShmCap { shm_type }
    }
}

type ShmCapId = u64;
struct ShmSpace {
    id_pool: ReusableIdPoolManual,
    space: BTreeMap<ShmCapId, ShmCap>,
}

impl ShmSpace {
    fn new() -> Self {
        ShmSpace {
            id_pool: ReusableIdPoolManual::new(),
            space: BTreeMap::new(),
        }
    }

    fn new_shm_cap(&mut self, shm_type: ShmType) -> Result<(ShmCapId, &ShmCap), ShmSpaceError> {
        let id = self.id_pool.try_allocate()
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::TooManyConcurrentIDs => ExhaustedSnafu.build() })?;

        let entry = self.space.entry(id);
        let shm_cap = match entry {
            Entry::Occupied(_) => return DuplicateIdSnafu.fail(),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(ShmCap::new(shm_type)),
        };

        Ok((id, shm_cap))
    }

    fn destroy_shm_cap(&mut self, shm_cap_id: ShmCapId) {
        // TODO: There needs to be checks here that the SHM has been released, etc.
        self.space.remove(&shm_cap_id);
    }
}

#[derive(Snafu, SnafuCliDebug)]
enum ShmSpaceError {
    #[snafu(display("The new pool ID was already present in the space. This should never happen, and indicates a bug in Nushift's code."))]
    DuplicateId,
    #[snafu(display("The maximum amount of SHM capabilities have been used for this app. Please destroy some capabilities."))]
    Exhausted,
}

#[derive(Default)]
pub struct NushiftSubsystem;

impl Subsystem for NushiftSubsystem {
    fn system_call(
        &mut self,
        context: &mut RiscvMachine<Self>,
    ) -> Result<Option<SubsystemAction>, RiscvMachineError> {
        let registers = &context.state().registers;
        let syscall = Syscall::try_from(registers.get(riscy_isa::Register::A0));

        match syscall {
            Err(_) => {
                // TODO: Return an error to the program that it was an unknown
                // syscall. Don't stop the program.
                Ok(None)
            },
            Ok(Syscall::Exit) => Ok(Some(SubsystemAction::Exit { status_code: registers.get(riscy_isa::Register::A1) })),
            _ => {
                // Return immediately, because these system calls should be
                // asynchronous.

                // TODO: Actually queue something, though.
                Ok(None)
            },
        }
    }
}

// TODO: Probably don't have this in this file.
impl<T: SupportMachine> Syscalls<T> for ProcessControlBlock {
    fn initialize(&mut self, _machine: &mut T) -> Result<(), CKBVMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut T) -> Result<bool, CKBVMError> {
        // TODO: When 32-bit apps are supported, convert into u64 from multiple
        // registers, instead of `.to_u64()` which can only act on a single
        // register here)
        let syscall = Syscall::try_from(machine.registers()[A0].to_u64());

        match syscall {
            Err(_) => {
                // TODO: Return an error to the program that it was an unknown
                // syscall. Don't stop the program.
                Ok(false)
            },
            Ok(Syscall::Exit) => {
                self.user_exit(machine.registers()[A1].to_u64());
                Ok(true)
            },
            _ => {
                // Return immediately, because these system calls should be
                // asynchronous.

                // TODO: Actually queue something, though.
                Ok(true)
            },
        }
    }
}
