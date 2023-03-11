use reusable_id_pool::ArcId;

use super::process_control_block::ProcessControlBlock;

pub struct Tab {
    id: ArcId,
    title: String,
    emulated_machine: ProcessControlBlock<u64>,
}

impl Tab {
    pub fn new<S: Into<String>>(id: ArcId, title: S) -> Self {
        Tab {
            id,
            title: title.into(),
            emulated_machine: ProcessControlBlock::new(),
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

    pub fn load(&mut self, image: Vec<u8>) {
        let result = self.emulated_machine.load_machine(image);

        // If an error occurred, log the error.
        if let Err(wrapper_error) = result {
            log::error!("{wrapper_error}");
        }
    }

    pub fn run(&mut self) {
        let result = self.emulated_machine.run();

        match result {
            Ok(exit_reason) => log::info!("Exit reason: {exit_reason:?}"),
            Err(wrapper_error) => log::error!("{wrapper_error}"),
        }
    }
}
