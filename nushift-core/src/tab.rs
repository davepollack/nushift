use elfloader::ElfBinary;
use reusable_id_pool::ArcId;

use super::riscv_machine_wrapper::RiscvMachineWrapper;

pub struct Tab {
    id: ArcId,
    title: String,
    emulated_machine: RiscvMachineWrapper,
}

impl Tab {
    pub fn new<S: Into<String>>(id: ArcId, title: S) -> Self {
        Tab {
            id,
            title: title.into(),
            emulated_machine: Default::default(),
        }
    }

    pub fn id(&self) -> &ArcId {
        &self.id
    }

    // TODO remove the below suppression when `title()` is used
    #[allow(dead_code)]
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn load(&mut self, binary: ElfBinary) {
        self.emulated_machine = RiscvMachineWrapper::load(binary);
    }

    pub fn run(&mut self) {
        let result = self.emulated_machine.run();

        // If an error occurred, log the error.
        if let Err(wrapper_error) = result {
            log::error!("{wrapper_error}");
        }
    }
}
