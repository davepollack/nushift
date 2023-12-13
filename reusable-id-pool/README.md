# reusable-id-pool

A pool for RAII IDs.

This crate provides two structs, `ReusableIdPool` and `ReusableIdPoolManual`.

## Example

```rust
let reusable_id_pool = ReusableIdPool::new();

let id = reusable_id_pool.allocate();

// Do something with the `id`, like move it into a struct. It will be returned to the pool when it is dropped.
```

## ReusableIdPool

A `std`-only struct that hands out `ArcId`s, which are opaque to the user.

To assign an ID to multiple things, use `ArcId::clone(&id)` (uses `Arc::clone` under the hood) to get further instances of the ID. They compare (`PartialEq`) as equal.

An ID is released by dropping — when all its `ArcId`s are dropped. `ArcId` drop is constant time (decrementing a reference count, or appending to a free list for the final one).

## ReusableIdPoolManual

A struct that hands out `u64` IDs. This should be used instead of `ReusableIdPool` when the ID needs to be serialised, for example over a binary ABI, as `nushift-core` needs to do.

`#![no_std]` is supported (set `default-features = false`), but `alloc` is required.

## Time complexity

`ReusableIdPool` (`std`-only):

Allocate: O(1)\
Release: O(1)

`ReusableIdPoolManual` (`std`):

Allocate: O(1)\
Release: O(1)

`ReusableIdPoolManual` (`#![no_std]`):

Allocate: O(log n)\
Release: O(log n)

The `id-pool` crate has more functionality than `ReusableIdPoolManual`, is always `#![no_std]`, and has O(1) allocate and O(log n) release, so it probably should be used instead of `ReusableIdPoolManual` for this case.
