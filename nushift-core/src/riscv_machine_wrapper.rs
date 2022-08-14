use riscy_emulator::{memory::{Memory, Region}, machine::RiscvMachine};
use riscy_isa::Register;

use crate::nushift_subsystem::NushiftSubsystem;

pub struct RiscvMachineWrapper(RiscvMachine<NushiftSubsystem>);

impl RiscvMachineWrapper {
    pub fn new() -> RiscvMachineWrapper {
        let mut memory = Memory::new();

        // The stack. 256 KiB.
        //
        // Should the location and size be determined by app metadata? Should
        // the location be randomised?
        const STACK_BASE: u64 = 0x80000000;
        const STACK_SIZE: u64 = 0x40000;
        let stack = Region::readwrite_memory(STACK_BASE, STACK_SIZE);

        memory.add_region(stack);

        let entry = 0u64; // TODO: Make real.
        let mut machine = RiscvMachine::new(memory, entry);
        machine.state_mut().registers.set(Register::StackPointer, STACK_BASE + STACK_SIZE);

        RiscvMachineWrapper(machine)
    }
}
