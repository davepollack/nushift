// Copyright 2024 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

mod x448_resolver;

pub fn add(left: usize, right: usize) -> Option<usize> {
    left.checked_add(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, Some(4));
    }
}
