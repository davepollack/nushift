use num_enum::TryFromPrimitive;
use reusable_id_pool::{ReusableIdPool, ReusableIdPoolError, ArcId};
use riscy_emulator::{
    subsystem::{Subsystem, SubsystemAction},
    machine::{RiscvMachine, RiscvMachineError},
};
use riscy_isa::Register;
use snafu::Snafu;
use snafu_cli_debug::SnafuCliDebug;
use std::{convert::TryFrom, sync::{Arc, Mutex}, collections::BTreeMap};

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

type ShmCapId = ArcId;
struct ShmSpace {
    id_pool: Arc<Mutex<ReusableIdPool>>,
    space: BTreeMap<ShmCapId, ShmCap>,
}

impl ShmSpace {
    fn new() -> Self {
        ShmSpace {
            id_pool: ReusableIdPool::new(),
            space: BTreeMap::new(),
        }
    }

    fn new_shm_cap(&mut self, shm_type: ShmType) -> Result<(ShmCapId, &ShmCap), ShmSpaceError> {
        let id = ReusableIdPool::try_allocate(&self.id_pool)
            .map_err(|rip_err| match rip_err { ReusableIdPoolError::Exhausted => ExhaustedSnafu.build() })?;

        // I would really like `try_insert` to be available on stable. If it is
        // available by the time you are reading this, please replace this code.
        if self.space.contains_key(&id) {
            return DuplicateIdSnafu.fail();
        }
        let shm_cap = self.space.entry(ArcId::clone(&id)).or_insert(ShmCap::new(shm_type));
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
        let syscall = Syscall::try_from(registers.get(Register::A0));

        match syscall {
            Err(_) => {
                // TODO: Return an error to the program that it was an unknown
                // syscall. Don't stop the program.
                Ok(None)
            },
            Ok(Syscall::Exit) => Ok(Some(SubsystemAction::Exit { status_code: registers.get(Register::A1) })),
            _ => {
                // Return immediately, because these system calls should be
                // asynchronous.

                // TODO: Actually queue something, though.
                Ok(None)
            },
        }
    }
}
