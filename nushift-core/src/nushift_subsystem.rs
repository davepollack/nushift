use riscy_emulator::{
    subsystem::{Subsystem, SubsystemAction},
    machine::{RiscvMachine, RiscvMachineError},
};

#[derive(Default)]
struct NushiftSubsystem;

impl Subsystem for NushiftSubsystem {
    fn system_call(
        &mut self,
        _context: &mut RiscvMachine<Self>,
    ) -> Result<Option<SubsystemAction>, RiscvMachineError> {
        // Return immediately, because these system calls should be
        // asynchronous.

        // TODO: Actually queue something, though.
        Ok(None)
    }
}
