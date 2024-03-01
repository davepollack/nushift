// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

pub struct RollbackChain<'a, T> {
    rollbacks: Vec<Box<dyn FnOnce(&mut T)>>,
    target: &'a mut T,
    all_succeeded: bool,
}

impl<'a, T> RollbackChain<'a, T> {
    pub fn new(target: &'a mut T) -> Self {
        RollbackChain {
            rollbacks: Vec::new(),
            target,
            all_succeeded: false,
        }
    }

    pub fn exec<F, U>(&mut self, func: F) -> U
    where
        F: FnOnce(&mut T) -> U,
    {
        func(self.target)
    }

    pub fn add_rollback<F>(&mut self, rollback: F)
    where
        F: FnOnce(&mut T) + 'static,
    {
        self.rollbacks.push(Box::new(rollback));
    }

    pub fn all_succeeded(&mut self) {
        self.all_succeeded = true;
    }
}

impl<'a, T> Drop for RollbackChain<'a, T> {
    fn drop(&mut self) {
        if !self.all_succeeded {
            for rollback in self.rollbacks.drain(..).rev() {
                rollback(self.target);
            }
        }
    }
}
