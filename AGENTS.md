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
- **Streaming length-delimited payloads are built by a closure.** `read_protobuf_fields_with` and `protobuf_fields_with` take `F: FnMut(usize) -> Result<L, ProtobufError>` where `L: AsMut<[u8]>`. The readers in `src/field/read.rs` store `F` by value and call it once per length-delimited field with the payload length. The closure returns at least `len` initialized bytes. `as_mut().len()` must be `>= len` (`assert!`, not `debug_assert!`). The reader writes the first `len` bytes and leaves any suffix. The closure's error type is `ProtobufError`. Callers convert another error with `map_err(Into::into)` inside the closure. Do not box the closure. An allocator, when there is one, is captured by the caller's closure. `read_protobuf_fields` and `protobuf_fields` pass `fn(usize) -> Result<Vec<u8>, ProtobufError>` and still yield `Field<Vec<u8>>`. The default closure zero-fills, including the byte-iterator path, because `AsMut<[u8]>` needs initialized bytes. If the payload bytes run out, the buffer is dropped before `UnexpectedEof`. A `Drop` that panics (`UnmanagedVec`) replaces that error. This closure builds a new buffer; it does not fill an existing unmanaged slot. `AsRefExtProtobuf` still returns `Field<&[u8]>` and stays zero-copy.
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
- **`IteratorExtVarint` takes `self` by value.** `read_varint`, `read_varint_partial`, and `read_varint_resume` consume the iterator. Tag methods take `&mut self`. `(&mut iter).read_varint()` keeps the unread tail, because `&mut I: Iterator`. Calling the method on an owned iterator drops the tail. Changing this signature is a breaking change.
- **Size constants are not enforced.** `MAX_MESSAGE_SIZE` and `MAX_STRING_SIZE` are public and unused by field readers. Streaming readers pass the length prefix to the payload closure. A slice read returns `UnexpectedEof` when the buffer is shorter than that prefix. Do not document the constants as a parse limit until a reader checks them.
- **`Varint::try_to_sint32` error payload.** On failure, `ProtobufError::VarintDowncastOutOfRange.value` is the ZigZag-decoded `i64` cast with `as u64`, not the raw varint. `try_to_uint32` and `try_to_int32` report the raw varint. Keep that difference in mind before changing the `value` field; callers may already match on it.
