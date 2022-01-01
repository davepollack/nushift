use riscy_emulator::{memory::Memory, machine::RiscvMachine};

use crate::nushift_subsystem::NushiftSubsystem;

pub struct RiscvMachineWrapper;

impl RiscvMachineWrapper {
    pub fn new() -> RiscvMachine<NushiftSubsystem> {
        let memory = Memory::new();
        let entry = 0u64; // TODO: Make real.
        RiscvMachine::new(memory, entry)
    }
}
