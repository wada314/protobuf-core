# AGENTS.md

Instructions for coding agents working in this repository. This file is the [AGENTS.md](https://agents.md/) entry point. [`README.md`](README.md) is for humans.

## Overview

`protobuf-core` is a **wire-format primitives** crate: tags, varints, field numbers, and raw field I/O. It is not a message library and does not implement `google.protobuf.*`.

Edition 2024. Field, tag, and varint I/O ship together. Do not reintroduce `read` / `write` Cargo features.

## Build, test, lint

```sh
cargo test
cargo fmt
cargo clippy --all-targets
```

## Constraints (do not reverse these)

- **Sync only.** No async public API, no `StreamExt*` traits. Callers buffer bytes and use sync parse (including tag/varint `*_partial` / `*_resume`).
- **No field-level partial/resume** (`read_field_partial`). Compose from tag + varint primitives if needed.
- **No full message types.** Keep the surface to constants, `Varint`, `Tag`, `Field` / `FieldValue`, and extension traits.
- **Do not confuse Rust types with protobuf types.** Name conversions after the protobuf type (`from_sint32`, `to_uint64`), not the Rust integer.
- Minimize dependencies. `thiserror` is the expected exception.
- **Streaming length-delimited payloads use `LenBuf`.** `IteratorExtProtobuf`, `TryIteratorExtProtobuf`, and `ReadExtProtobuf` are generic over `L: LenBuf`. `AsRefExtProtobuf` still returns `Field<&[u8]>` and stays zero-copy. The payload length is known before its bytes are read. The required method is `with_len`: allocate exactly `len` initialized bytes. `LenBuf: AsMut<[u8]>`, and `as_mut` returns that whole slice. Do not add an `as_mut_slice` method, an append method, or a fill-cursor method. `Vec<u8>` and `Box<[u8]>` implement `LenBuf` with no extra feature. `Vec<u8, A>` and `Box<[u8], A>` (`A: Allocator + Default`) are the `nightly` feature only; that feature enables `allocator_api` and does not build on stable rustc. On nightly `Vec<u8>` is `Vec<u8, Global>` and `Box<[u8]>` is `Box<[u8], Global>`, so a normal build keeps only the generic impl of each. `[package.metadata.docs.rs]` enables `nightly`, so the published docs show those allocator signatures with `doc(cfg(feature = "nightly"))`. The badge marks the allocator signature. The impl docs say the feature is not required for `Vec<u8>` or `Box<[u8]>` when the caller does not use an allocator. `Box<[u8]>` is `into_boxed_slice` of a vector whose length equals its capacity, then the payload is written into that box. Do not copy the payload into a second buffer. Default `cargo test` stays off `nightly`.
- `FieldNumber::try_new(u32) -> Result<Self, u32>` returns the bad input (not `ProtobufError`) so it can stay `const fn`. Map to `ProtobufError::FieldNumberOutOfRange` at I/O boundaries.

## Code style

- Imports: `crate::` / `self::` for this crate; other paths start with `::`.
- Module files use `foo.rs`, not `foo/mod.rs`.
- Comments in English.

## Docs

- User API: [`README.md`](README.md), [`CHANGELOG.md`](CHANGELOG.md), rustdoc on `src/lib.rs`.
- Put durable instructions in this file. Do not add a second design-history document.

## Known follow-ups

Left out of 0.3.0 on purpose. Do not treat them as conformant behavior, and do not sneak a fix into a patch release if it changes public types or error variants.

- **Groups.** `WireType::SGroup` (3) and `WireType::EGroup` (4) are real wire types. `parse_field` still returns `ProtobufError::InvalidWireType` for both, and that variant's message says the value must be 0-5. There is no group reader, writer, or skip helper. A later change needs an explicit group scan or a distinct error. Do not relabel the current rejection as spec-compliant skipping.
- **Iterator types callers cannot name.** `IteratorExtVarint::read_varints` and `TryIteratorExtVarint::read_varints` return `VarintIterator`, and the `Item = u8` path wraps the input in `ToResultIterator`. `TryIteratorExtProtobuf::protobuf_fields` returns `ProtobufFieldIteratorFromTryBytes`. Those types sit in private modules and are not re-exported from the crate root. `ProtobufFieldIteratorFromBytes`, `ProtobufFieldIterator`, and `ProtobufFieldSliceIterator` are. Re-export the missing types together when a caller needs to name them.
- **`IteratorExtVarint` takes `self` by value.** `read_varint`, `read_varint_partial`, and `read_varint_resume` consume the iterator. Tag methods take `&mut self`. `(&mut iter).read_varint()` keeps the unread tail, because `&mut I: Iterator`. Calling the method on an owned iterator drops the tail. Changing this signature is a breaking change.
- **Size constants are not enforced.** `MAX_MESSAGE_SIZE` and `MAX_STRING_SIZE` are public and unused by field readers. `LenBuf::with_len` allocates from the length prefix. A slice read returns `UnexpectedEof` when the buffer is shorter than that prefix. Do not document the constants as a parse limit until a reader checks them.
- **`Varint::try_to_sint32` error payload.** On failure, `ProtobufError::VarintDowncastOutOfRange.value` is the ZigZag-decoded `i64` cast with `as u64`, not the raw varint. `try_to_uint32` and `try_to_int32` report the raw varint. Keep that difference in mind before changing the `value` field; callers may already match on it.
