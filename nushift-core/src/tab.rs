use core::ops::DerefMut;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use reusable_id_pool::ArcId;

use super::hypervisor_event::{HypervisorEvent, HypervisorEventHandler};
use super::nushift_subsystem::NushiftSubsystem;
use super::process_control_block::ProcessControlBlock;
use super::register_ipc::Task;

pub struct Tab {
    id: ArcId,
    machine_nushift_subsystem: Arc<Mutex<NushiftSubsystem>>,
}

impl Tab {
    pub fn new(id: ArcId, hypervisor_event_handler: HypervisorEventHandler) -> Self {
        // This isn't shared yet, but it will be when NushiftSubsystem hands it to multiple spaces
        let bound_hypervisor_event_handler = Arc::new({
            let cloned_id = ArcId::clone(&id);
            move |unbound_hyp_event| {
                hypervisor_event_handler(HypervisorEvent::from(ArcId::clone(&cloned_id), unbound_hyp_event))
            }
        });

        Tab {
            id,
            machine_nushift_subsystem: Arc::new(Mutex::new(NushiftSubsystem::new(bound_hypervisor_event_handler))),
        }
    }

    pub fn id(&self) -> &ArcId {
        &self.id
    }

    pub fn load_and_run(&mut self, image: Vec<u8>) {
        let (syscall_enter_send, syscall_enter_receive) = mpsc::channel();
        let (syscall_return_send, syscall_return_receive) = mpsc::channel();
        let subsystem_cloned_for_machine = Arc::clone(&self.machine_nushift_subsystem);
        let mut machine = ProcessControlBlock::<u64>::new(syscall_enter_send, syscall_return_receive, subsystem_cloned_for_machine);

        let result = machine.load_machine(image);

        // If an error occurred, log the error and return.
        match result {
            Err(wrapper_error) => {
                tracing::error!("Failed to load machine: {:?}, tab ID: {:?}", wrapper_error, self.id);
                return;
            },
            Ok(_) => {},
        }

        let builder = thread::Builder::new();
        let machine_thread = builder.spawn(move || machine.run());
        let machine_thread = match machine_thread {
            Err(os_error) => {
                tracing::error!("Failed to create OS thread: {:?}, tab ID {:?}", os_error, self.id);
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
                let mut subsystem = self.machine_nushift_subsystem.lock().unwrap();
                subsystem.ecall(receive)
            };
            syscall_return_send.send(syscall_return.0).expect("Since we just received, other thread should be waiting on our send");

            // Call the non-blocking bit of ecall here. Asynchronous tasks?
            // If this "non-blocking" bit locks the subsystem, it's not going to
            // be non-blocking with the bits of the app that access memory.
            match syscall_return.1 {
                Some(Task::AccessibilityTreePublish { accessibility_tree_cap_id }) => {
                    let mut guard = self.machine_nushift_subsystem.lock().unwrap();
                    let subsystem = guard.deref_mut();
                    match subsystem.accessibility_tree_space.publish_accessibility_tree_deferred(accessibility_tree_cap_id, &mut subsystem.shm_space) {
                        Ok(_) => {},
                        Err(_) => {}, // TODO: On internal error, terminate app (?)
                    }
                },
                Some(Task::TitlePublish { title_cap_id }) => {
                    let mut guard = self.machine_nushift_subsystem.lock().unwrap();
                    let subsystem = guard.deref_mut();
                    match subsystem.title_space.publish_title_deferred(title_cap_id, &mut subsystem.shm_space) {
                        Ok(_) => {},
                        Err(_) => {}, // TODO: On internal error, terminate app (?)
                    }
                },
                _ => {},
            }
        }

        let run_result = match machine_thread.join() {
            Err(join_error) => {
                tracing::error!("Thread panicked: {:?}, tab ID {:?}", join_error, self.id);
                return;
            },
            Ok(run_result) => run_result,
        };

        match run_result {
            Ok(exit_reason) => tracing::info!("Exit reason: {exit_reason:?}"),
            Err(run_error) => tracing::error!("Run error: {:?}, tab ID {:?}", run_error, self.id),
        }
    }
}
