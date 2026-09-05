# Contributing

## Code organization conventions

These apply project-wide. They mostly align with the Rust API Guidelines
("C-STRUCTURE") plus kernel-code conventions.

### File layout

1. One primary type per file; the file is named after it (snake_case). Supporting types
   (small enums, index wrappers, flag types) live in the same file, right above the
   first type that uses them (e.g. `EndpointSpeed` above `EndpointCharacteristics`).
2. Order within a file: type definitions -> inherent impls -> trait impls -> statics ->
   tests.
3. Raw MMIO structs (the `VolatileValue`-based ones) keep their accessor impl next to
   the struct, before any higher-level wrapper type.

### Impls

4. One impl block per trait (or one per inherent group), in order: conversion traits
   (`From`/`Into`), formatting (`Display`/`Debug`), operators (`Deref`, `BitOr`, ...),
   iterators.
5. Within an inherent impl, in order: associated constants, constructors, getters,
   setters, other methods. Getters/setters stay in field order.

### Enforcement

- rustfmt's `reorder_impl_items` covers rule 5 inside an impl; enable it in
  `rustfmt.toml`.
- There is no off-the-shelf tool for rules 1-3 (type-per-file, impl-block order): those
  either get a small CI script check (e.g. `queue_head.rs` must define `QueueHead`) or
  stay review-enforced.

## Error handling

Errors are `error::Error` triplets of `Fault` (what happened) + `Context` (what were
you doing) + `Facility` (where did it happen). Prefer a new `Fault`/`Context` variant
over panicking: asserts are only acceptable for internal invariants that are provably
unreachable, with a comment explaining why (see the `expect("the 2-bit field ...")`
pattern in `queue_head.rs`).

## Unsafe code

- Every `unsafe` block needs a `SAFETY:` comment explaining how the safety contract is
  upheld (the repo forbids undocumented unsafe blocks).
- `mmio::VolatilePtr` concentrates volatile-access unsafety at construction
  (`VolatilePtr::from_raw`): constructing one certifies the volatile conditions
  (mapped, valid `T` bit patterns, non-cacheable, no concurrent access) for the value's
  whole lifetime, and all accessors are safe. Audit point: only the `from_raw` call
  sites.
