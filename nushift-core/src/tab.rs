use std::sync::Arc;
use elfloader::ElfBinary;
use reusable_id_pool::Id;

use super::riscv_machine_wrapper::RiscvMachineWrapper;

pub struct Tab {
    id: Arc<Id>,
    title: String,
    emulated_machine: RiscvMachineWrapper,
}

impl Tab {
    pub fn new<S: Into<String>>(id: Arc<Id>, title: S) -> Self {
        Tab {
            id,
            title: title.into(),
            emulated_machine: RiscvMachineWrapper::new(),
        }
    }

    pub fn id(&self) -> &Arc<Id> {
        &self.id
    }

    // TODO remove the below suppression when `title()` is used
    #[allow(dead_code)]
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn load(&mut self, binary: ElfBinary) {
        binary.load(&mut self.emulated_machine).expect("Can't load the binary?")
    }
}
