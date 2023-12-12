# reusable-id-pool

An RAII ID pool with O(1) acquire and O(1) release of opaque IDs.

This crate provides two structs, `ReusableIdPool` and `ReusableIdPoolManual`.

## ReusableIdPool

A `std`-only struct that hands out `ArcId`s, which are opaque to the user.

To assign an ID to multiple things, use `ArcId::clone(&id)` (uses `Arc::clone` under the hood) to get further instances of the ID. They compare (`PartialEq`) as equal.

An ID is released by dropping — when all its `ArcId`s are dropped. `ArcId` drop is constant time (reference count decrement, or appending to a free list for the final one).

Old IDs are reused due to the use of a free list, meaning your allocate/release workload can run indefinitely and never exhaust, provided you have an upper bound on the number of IDs live at any one time (no more than 2<sup>64</sup> - 1 concurrently live IDs are allowed).

The memory usage of this pool is bounded by your upper bound on the number of IDs live at any one time, the bound being achieved when you allocate that number of IDs then free all of them.

A `ReusableIdPool` can be dropped with `ArcId`s still active — it will not actually drop until all its `ArcId`s have been dropped.


## ReusableIdPoolManual

TODO
