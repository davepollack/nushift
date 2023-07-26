use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use reusable_id_pool::ArcId;

use super::nushift_subsystem::NushiftSubsystem;
use super::process_control_block::ProcessControlBlock;

pub struct Tab {
    id: ArcId,
    title: String,
}

impl Tab {
    pub fn new<S: Into<String>>(id: ArcId, title: S) -> Self {
        Tab {
            id,
            title: title.into(),
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

    pub fn load_and_run(&mut self, image: Vec<u8>) {
        let (syscall_enter_send, syscall_enter_receive) = mpsc::channel();
        let (syscall_return_send, syscall_return_receive) = mpsc::channel();
        let subsystem = Arc::new(Mutex::new(NushiftSubsystem::new()));
        let subsystem_cloned_for_machine = Arc::clone(&subsystem);
        let mut machine = ProcessControlBlock::<u64>::new();
        machine.set_syscall_enter(syscall_enter_send);
        machine.set_syscall_return(syscall_return_receive);
        machine.set_locked_subsystem(subsystem_cloned_for_machine);

        let result = machine.load_machine(image);

        // If an error occurred, log the error and return.
        match result {
            Err(wrapper_error) => {
                log::error!("Failed to load machine: {:?}, tab ID: {:?}", wrapper_error, self.id);
                return;
            },
            Ok(_) => {},
        }

        let builder = thread::Builder::new();
        let machine_thread = builder.spawn(move || machine.run());
        let machine_thread = match machine_thread {
            Err(os_error) => {
                log::error!("Failed to create OS thread: {:?}, tab ID {:?}", os_error, self.id);
                return;
            },
            Ok(machine_thread) => machine_thread,
        };

        // recv can end in one of two ways. First, an error because the sender
        // disconnected because the thread ended. This is totally normal, and
        // continue to join the thread in this case. Otherwise, process the
        // message.
        while let Ok(receive) = syscall_enter_receive.recv() {
            let syscall_return = {
                let mut subsystem = subsystem.lock().unwrap();
                subsystem.ecall(receive)
            };
            syscall_return_send.send(syscall_return).expect("Since we just received, other thread should be waiting on our send");

            // Call the non-blocking bit of ecall here. Asynchronous tasks?
            // If this "non-blocking" bit locks the subsystem, it's not going to
            // be non-blocking with the bits of the app that access memory.
        }

        let run_result = match machine_thread.join() {
            Err(join_error) => {
                log::error!("Thread panicked: {:?}, tab ID {:?}", join_error, self.id);
                return;
            },
            Ok(run_result) => run_result,
        };

        match run_result {
            Ok(exit_reason) => log::info!("Exit reason: {exit_reason:?}"),
            Err(run_error) => log::error!("Run error: {:?}, tab ID {:?}", run_error, self.id),
        }
    }
}
