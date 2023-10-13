use core::ops::DerefMut;
use std::collections::HashSet;
use std::sync::{mpsc, Arc, Mutex, Condvar};
use std::thread;

use reusable_id_pool::ArcId;
use serde::Serialize;

use crate::deferred_space::app_global_deferred_space::Task;
use crate::nushift_subsystem::NushiftSubsystem;
use crate::process_control_block::ProcessControlBlock;

use super::hypervisor_event::HypervisorEventHandler;
use super::tab_context::DefaultTabContext;

pub struct Tab {
    id: ArcId,
    machine_nushift_subsystem: Arc<Mutex<NushiftSubsystem>>,
    output: Arc<Mutex<Output>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Output {
    size_px: Vec<u64>,
    scale: Vec<f64>,
}

impl Output {
    pub fn new(size_px: Vec<u64>, scale: Vec<f64>) -> Self {
        Self { size_px, scale }
    }
}

impl Tab {
    pub fn new(id: ArcId, hypervisor_event_handler: HypervisorEventHandler, initial_output: Output) -> Self {
        let output = Arc::new(Mutex::new(initial_output));
        let tab_context = Arc::new(DefaultTabContext::new(ArcId::clone(&id), hypervisor_event_handler, Arc::clone(&output)));

        let blocking_on_tasks = Arc::new((Mutex::new(HashSet::new()), Condvar::new()));

        Self {
            id,
            machine_nushift_subsystem: Arc::new(Mutex::new(NushiftSubsystem::new(tab_context, blocking_on_tasks))),
            output,
        }
    }

    pub fn update_output(&mut self, output: Output) {
        *self.output.lock().unwrap() = output;
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
            syscall_return_send.send(syscall_return).expect("Since we just received, other thread should be waiting on our send");

            // Call the non-blocking bit of ecall here. Asynchronous tasks?
            // If this "non-blocking" bit locks the subsystem, it's not going to
            // be non-blocking with the bits of the app that access memory.
            //
            // This "non-blocking" bit does currently entirely lock the
            // subsystem.
            //
            // It must be made sure that a task being set up in the
            // AppGlobalDeferredSpace and the blocking part being processed in
            // the relevant space have both been done before dispatching tasks.
            // In other words, when we make the locking more fine-grained later,
            // those two things must still be locked together.
            let mut guard = self.machine_nushift_subsystem.lock().unwrap();
            let subsystem = guard.deref_mut();
            let tasks = subsystem.app_global_deferred_space.finish_tasks();
            for (task_id, task) in tasks {
                match task {
                    Task::AccessibilityTreePublish { accessibility_tree_cap_id } => {
                        match subsystem.accessibility_tree_space.publish_accessibility_tree_deferred(accessibility_tree_cap_id, &mut subsystem.shm_space) {
                            Ok(_) => {},
                            Err(_) => {}, // TODO: On internal error, terminate app (?)
                        }
                    },
                    Task::TitlePublish { title_cap_id } => {
                        match subsystem.title_space.publish_title_deferred(title_cap_id, &mut subsystem.shm_space) {
                            Ok(_) => {},
                            Err(_) => {}, // TODO: On internal error, terminate app (?)
                        }
                    },
                    Task::GfxGetOutputs { gfx_cap_id } => {
                        match subsystem.gfx_space.get_outputs_deferred(gfx_cap_id, &mut subsystem.shm_space) {
                            Ok(_) => {},
                            Err(_) => {}, // TODO: On internal error, terminate app (?)
                        }
                    },
                    Task::GfxCpuPresent { gfx_cpu_present_buffer_cap_id } => {
                        match subsystem.gfx_space.cpu_present_deferred(gfx_cpu_present_buffer_cap_id, &mut subsystem.shm_space) {
                            Ok(_) => {},
                            Err(_) => {}, // TODO: On internal error, terminate app (?)
                        }
                    },
                }

                let (lock, cvar) = &*subsystem.blocking_on_tasks;
                let mut guard = lock.lock().unwrap();
                guard.remove(&task_id);
                cvar.notify_one();
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
