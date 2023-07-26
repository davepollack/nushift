use std::sync::{Arc, Mutex, Condvar};
use std::thread;

use reusable_id_pool::ArcId;

use super::nushift_subsystem::NushiftSubsystem;
use super::process_control_block::{ProcessControlBlock, ThreadState};

pub struct Tab {
    id: ArcId,
    title: String,
    // TODO: Do either emulated_machine nor subsystem need to be stored here?
    emulated_machine: Arc<(Mutex<ProcessControlBlock<u64>>, Condvar)>,
    subsystem: Arc<Mutex<NushiftSubsystem>>,
}

impl Tab {
    pub fn new<S: Into<String>>(id: ArcId, title: S) -> Self {
        Tab {
            id,
            title: title.into(),
            emulated_machine: Arc::new((Mutex::new(ProcessControlBlock::new()), Condvar::new())),
            subsystem: Arc::new(Mutex::new(NushiftSubsystem::new())),
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
        let emulated_machine_cloned_for_self = Arc::clone(&self.emulated_machine);
        let emulated_machine_cloned_for_thread = Arc::clone(&self.emulated_machine);
        let subsystem_cloned_for_self = Arc::clone(&self.subsystem);
        let (locked, cvar) = self.emulated_machine.as_ref();
        {
            let mut emulated_mac = locked.lock().unwrap();
            emulated_mac.set_locked_self(emulated_machine_cloned_for_self);
            emulated_mac.set_locked_subsystem(subsystem_cloned_for_self);
            let result = emulated_mac.load_machine(image);

            // If an error occurred, log the error and return.
            match result {
                Err(wrapper_error) => {
                    log::error!("Failed to load machine: {:?}, tab ID: {:?}", wrapper_error, self.id);
                    return;
                },
                Ok(_) => {},
            }
        }

        let builder = thread::Builder::new();
        let machine_thread = builder.spawn(move || {
            let (locked, _cvar) = emulated_machine_cloned_for_thread.as_ref();
            let mut machine = locked.lock().unwrap();
            machine.run()
        });
        let machine_thread = match machine_thread {
            Err(os_error) => {
                log::error!("Failed to create OS thread: {:?}, tab ID {:?}", os_error, self.id);
                return;
            },
            Ok(machine_thread) => machine_thread,
        };

        loop {
            let mut machine = cvar.wait(locked.lock().unwrap()).unwrap();

            match machine.thread_state() {
                ThreadState::Running => {
                    // Spurious wakeup, keep waiting.
                },
                ThreadState::Ecall => {
                    // This is the blocking bit of the ecall. How will we continue with the non-blocking bit?
                    {
                        let mut subsystem = self.subsystem.lock().unwrap();
                        // TODO: When/after testing the functionality, continue resolving this warning.
                        subsystem.ecall(&mut *machine);
                    }

                    machine.set_thread_state(ThreadState::Running);
                    cvar.notify_one();
                },
                ThreadState::Exited => break,
            }
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
