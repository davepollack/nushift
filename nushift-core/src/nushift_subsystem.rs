use riscy_emulator::{
    subsystem::{Subsystem, SubsystemAction},
    machine::{RiscvMachine, RiscvMachineError},
};
use riscy_isa::Register;

const SYSCALL_EXIT: u64 = 0;

#[derive(Default)]
pub struct NushiftSubsystem;

impl Subsystem for NushiftSubsystem {
    fn system_call(
        &mut self,
        context: &mut RiscvMachine<Self>,
    ) -> Result<Option<SubsystemAction>, RiscvMachineError> {
        let registers = &context.state().registers;
        let syscall_number = registers.get(Register::A0);

        match syscall_number {
            SYSCALL_EXIT => Ok(Some(SubsystemAction::Exit { status_code: registers.get(Register::A1) })),
            _ => {
                // Return immediately, because these system calls should be
                // asynchronous.

                // TODO: Actually queue something, though.
                Ok(None)
            },
        }
    }
}
