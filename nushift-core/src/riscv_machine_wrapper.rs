// TODO: Centralise lint configuration.
#![deny(unused_qualifications)]

use elfloader::{ElfLoader, ElfLoaderErr, LoadableHeaders, Flags, VAddr, RelocationEntry, ElfBinary};
use riscy_emulator::{memory::{Memory, Region, Permissions}, machine::RiscvMachine};
use riscy_isa::Register;

use crate::nushift_subsystem::NushiftSubsystem;

#[derive(Default)]
pub struct RiscvMachineWrapper(Option<RiscvMachine<NushiftSubsystem>>);

impl RiscvMachineWrapper {
    pub fn load(binary: ElfBinary) -> RiscvMachineWrapper {
        let mut memory = Memory::new();

        let mut loader = RiscvMachineLoader(&mut memory);
        if let Err(_) = binary.load(&mut loader) {
            return RiscvMachineWrapper(None);
        }

        // The stack. 256 KiB.
        //
        // Should the location and size be determined by app metadata? Should
        // the location be randomised?
        const STACK_BASE: u64 = 0x80000000;
        const STACK_SIZE: u64 = 0x40000;
        let stack = Region::readwrite_memory(STACK_BASE, STACK_SIZE);
        memory.add_region(stack);

        let entry = binary.entry_point();

        let mut machine = RiscvMachine::new(memory, entry);
        machine.state_mut().registers.set(Register::StackPointer, STACK_BASE + STACK_SIZE);

        RiscvMachineWrapper(Some(machine))
    }
}

struct RiscvMachineLoader<'a>(&'a mut Memory);

impl ElfLoader for RiscvMachineLoader<'_> {
    fn allocate(&mut self, load_headers: LoadableHeaders) -> Result<(), ElfLoaderErr> {
        for header in load_headers {
            let flags = header.flags();

            // Do not support sections which are both writable and executable, for now.
            if flags.is_write() && flags.is_execute() {
                log::error!(
                    "Section at vaddr {:#x} is both writable and executable, not supported at the moment, aborting loading program.",
                    header.virtual_addr(),
                );
                return Err(ElfLoaderErr::UnsupportedSectionData);
            }

            let mut region = Region::readwrite_memory(header.virtual_addr(), header.mem_size());
            region = region.change_permissions(Permissions::custom(flags.is_write(), flags.is_execute()));

            self.0.add_region(region);
        }

        Ok(())
    }

    fn load(&mut self, flags: Flags, base: VAddr, region: &[u8]) -> Result<(), ElfLoaderErr> {
        log::debug!(
            "Loading region with base {:?} and length {}, flags {:?}",
            base,
            region.len(),
            flags,
        );

        for (offset, byte) in region.iter().enumerate() {
            if let Err(_) = self.0.store_byte(base + (offset as u64), *byte as u64) {
                return Err(ElfLoaderErr::UnsupportedElfFormat);
            }
        }

        Ok(())
    }

    fn relocate(&mut self, _entry: RelocationEntry) -> Result<(), ElfLoaderErr> {
        // Unimplemented
        Err(ElfLoaderErr::UnsupportedRelocationEntry)
    }
}
