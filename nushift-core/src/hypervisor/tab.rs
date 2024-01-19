// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use core::ops::DerefMut;
use std::collections::HashSet;
use std::sync::{mpsc, Arc, Mutex, Condvar};
use std::thread::{Builder, JoinHandle};

use reusable_id_pool::ArcId;

use crate::deferred_space::app_global_deferred_space::Task;
use crate::gfx_space::GfxOutput;
use crate::nushift_subsystem::NushiftSubsystem;
use crate::process_control_block::ProcessControlBlock;

use super::hypervisor_event::HypervisorEventHandler;
use super::tab_context::DefaultTabContext;

pub struct Tab {
    id: ArcId,
    gfx_output: Arc<Mutex<GfxOutput>>,
    hypervisor_thread: Option<JoinHandle<()>>,
}

impl Tab {
    pub fn new(id: ArcId, initial_gfx_output: GfxOutput) -> Self {
        let gfx_output = Arc::new(Mutex::new(initial_gfx_output));

        Self {
            id,
            gfx_output,
            hypervisor_thread: None,
        }
    }

    pub fn update_gfx_output(&mut self, gfx_output: GfxOutput) {
        *self.gfx_output.lock().unwrap() = gfx_output;
    }

    pub fn load_and_run(&mut self, image: Vec<u8>, hypervisor_event_handler: HypervisorEventHandler) {
        let tab_id = ArcId::clone(&self.id);
        let gfx_output = Arc::clone(&self.gfx_output);

        let thread_builder = Builder::new();
        let hypervisor_thread = thread_builder.spawn(move || Self::load_and_run_impl(tab_id, gfx_output, image, hypervisor_event_handler));

        // If an error occurred, log the error and return.
        let hypervisor_thread = match hypervisor_thread {
            Err(os_error) => {
                tracing::error!("Failed to create OS hypervisor thread: {:?}, tab ID {:?}", os_error, self.id);
                return;
            },
            Ok(hypervisor_thread) => hypervisor_thread,
        };

        self.hypervisor_thread = Some(hypervisor_thread);
    }

    fn load_and_run_impl(tab_id: ArcId, gfx_output: Arc<Mutex<GfxOutput>>, image: Vec<u8>, hypervisor_event_handler: HypervisorEventHandler) {
        let tab_context = Arc::new(DefaultTabContext::new(ArcId::clone(&tab_id), hypervisor_event_handler, gfx_output));
        let blocking_on_tasks = Arc::new((Mutex::new(HashSet::new()), Condvar::new()));
        let machine_nushift_subsystem = Arc::new(Mutex::new(NushiftSubsystem::new(tab_context, blocking_on_tasks)));

        let (syscall_enter_send, syscall_enter_receive) = mpsc::channel();
        let (syscall_return_send, syscall_return_receive) = mpsc::channel();
        let subsystem_cloned_for_machine = Arc::clone(&machine_nushift_subsystem);
        let mut machine = ProcessControlBlock::<u64>::new(syscall_enter_send, syscall_return_receive, subsystem_cloned_for_machine);

        let result = machine.load_machine(image);

        // If an error occurred, log the error and return.
        match result {
            Err(wrapper_error) => {
                tracing::error!("Failed to load machine: {:?}, tab ID: {:?}", wrapper_error, tab_id);
                return;
            },
            Ok(_) => {},
        }

        let thread_builder = Builder::new();
        let machine_thread = thread_builder.spawn(move || machine.run());
        let machine_thread = match machine_thread {
            Err(os_error) => {
                tracing::error!("Failed to create OS tab thread: {:?}, tab ID {:?}", os_error, tab_id);
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
                let mut subsystem = machine_nushift_subsystem.lock().unwrap();
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
            let mut guard = machine_nushift_subsystem.lock().unwrap();
            let subsystem = guard.deref_mut();
            let tasks = subsystem.app_global_deferred_space.finish_tasks();
            for (task_id, task) in tasks {
                match task {
                    Task::AccessibilityTreePublish { accessibility_tree_cap_id } => {
                        match subsystem.accessibility_tree_space.publish_accessibility_tree_ron_deferred(accessibility_tree_cap_id, &mut subsystem.shm_space) {
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
                cvar.notify_one(); // TODO: Should this change to `notify_all` when an app can have multiple threads? Is that even how the hypervisor architecture is going to work?
            }
        }

        let run_result = match machine_thread.join() {
            Err(join_error) => {
                tracing::error!("Thread panicked: {:?}, tab ID {:?}", join_error, tab_id);
                return;
            },
            Ok(run_result) => run_result,
        };

        match run_result {
            Ok(exit_reason) => tracing::info!("Exit reason: {exit_reason:?}"),
            Err(run_error) => tracing::error!("Run error: {:?}, tab ID {:?}", run_error, tab_id),
        }
    }

    pub fn close_tab(&mut self) {
        // TODO: Cooperatively terminate the thread. The cooperation will
        // probably need to be in the interpreter loop if the app is in a
        // CPU-bound loop with no yielding and no memory accesses (which yield).
        self.hypervisor_thread = None;
    }
}
