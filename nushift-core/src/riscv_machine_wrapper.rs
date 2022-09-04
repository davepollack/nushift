// TODO: Centralise lint configuration.
#![deny(unused_qualifications)]

use std::{collections::BTreeMap, mem};

use elfloader::{ElfLoader, ElfLoaderErr, LoadableHeaders, Flags, VAddr, RelocationEntry, ElfBinary};
use riscy_emulator::{memory::{Memory, Region, Permissions}, machine::RiscvMachine};
use riscy_isa::Register;

use crate::nushift_subsystem::NushiftSubsystem;

#[derive(Default)]
pub struct RiscvMachineWrapper(Option<RiscvMachine<NushiftSubsystem>>);

impl RiscvMachineWrapper {
    pub fn load(binary: ElfBinary) -> RiscvMachineWrapper {
        let mut memory = Memory::new();

        let mut loader = RiscvMachineLoader(BTreeMap::new());
        if let Err(_) = binary.load(&mut loader) {
            return RiscvMachineWrapper(None);
        }

        let regions: Vec<Region> = loader.0.into_values().collect();
        for region in regions {
            memory.add_region(region);
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

struct RiscvMachineLoader(BTreeMap<u64, Region>);

impl ElfLoader for RiscvMachineLoader {
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

            let region = Region::readwrite_memory(header.virtual_addr(), header.mem_size());
            // Don't set the permissions of the region yet, because we need to
            // load (write) data into it.

            if let Some(_) = self.0.insert(header.virtual_addr(), region) {
                // Two regions with the same vaddr, error (for now. I'm not
                // familiar with why this could be valid).
                log::error!(
                    "Binary contains more than one section with the same base vaddr: {:#x}. This is not currently supported and may never be.",
                    header.virtual_addr(),
                );
                return Err(ElfLoaderErr::UnsupportedSectionData);
            }
        }

        Ok(())
    }

    fn load(&mut self, flags: Flags, base: VAddr, region_bytes: &[u8]) -> Result<(), ElfLoaderErr> {
        log::debug!(
            "Loading region with base {:#x} and length {}, flags [{}]",
            base,
            region_bytes.len(),
            flags,
        );

        let region = self.0.get_mut(&base).ok_or_else(|| {
            log::error!("discrepancy between allocated regions and calls to load");
            return ElfLoaderErr::UnsupportedSectionData;
        })?;

        for (offset, byte) in region_bytes.iter().enumerate() {
            if let Err(memory_error) = region.write(offset as u64, *byte) {
                log::error!("error when writing to region: {:?}", memory_error);
                return Err(ElfLoaderErr::UnsupportedSectionData);
            }
        }

        // Now, set the permissions.
        let taken_region = mem::replace(region, Region::readwrite_memory(0, 4));
        let permissioned_region = taken_region.change_permissions(Permissions::custom(flags.is_write(), flags.is_execute()));
        self.0.insert(base, permissioned_region);

        Ok(())
    }

    fn relocate(&mut self, _entry: RelocationEntry) -> Result<(), ElfLoaderErr> {
        // Unimplemented
        Err(ElfLoaderErr::UnsupportedRelocationEntry)
    }
}
