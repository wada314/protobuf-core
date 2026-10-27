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
- **`len` is only on `ReadExtProtobuf` field methods.** `read_protobuf_fields(self, len)` / `read_protobuf_fields_with(self, len, make)` take a message byte budget and bound the reader with `std::io::Take`. `ReadExtVarint` / `ReadExtTag` methods do not take `len`; reader EOF (or that existing `Take`) is the bound. `AsRefExtProtobuf` has no extra `len` argument; the slice length is the bound. Do not reintroduce byte-iterator input traits (`IteratorExt*` / `TryIteratorExt*`).
- **Streaming length-delimited payloads are built by a closure.** `ReadExtProtobuf::read_protobuf_fields_with` takes `F: FnMut(usize) -> Result<L, ProtobufError>` where `L: AsMut<[u8]>`. The readers in `src/field/read.rs` store `F` by value and call it once per length-delimited field with the payload length. The closure returns at least `len` initialized bytes. `as_mut().len()` must be `>= len` (`assert!`, not `debug_assert!`). The reader writes the first `len` bytes and leaves any suffix. The closure's error type is `ProtobufError`. Callers convert another error with `map_err(Into::into)` inside the closure. Do not box the closure. An allocator, when there is one, is captured by the caller's closure. `read_protobuf_fields` passes `fn(usize) -> Result<Vec<u8>, ProtobufError>` and still yields `Field<Vec<u8>>`. The default closure zero-fills because `AsMut<[u8]>` needs initialized bytes. If the payload bytes run out, the buffer is dropped before `UnexpectedEof`. A `Drop` that panics (`UnmanagedVec`) replaces that error. This closure builds a new buffer; it does not fill an existing unmanaged slot. `AsRefExtProtobuf` still returns `Field<&[u8]>` and stays zero-copy.
- `FieldNumber::try_new(u32) -> Result<Self, u32>` returns the bad input (not `ProtobufError`) so it can stay `const fn`. Map to `ProtobufError::FieldNumberOutOfRange` at I/O boundaries.

## Code style

- Imports: `crate::` / `self::` for this crate; other paths start with `::`.
- Module files use `foo.rs`, not `foo/mod.rs`.
- Comments in English.

## Docs

- User API: [`README.md`](README.md), [`CHANGELOG.md`](CHANGELOG.md), rustdoc on `src/lib.rs`.
- Put durable instructions in this file. Do not add a second design-history document.

## Known follow-ups

Deferred on purpose. Do not treat them as conformant behavior, and do not sneak a fix into a patch release if it changes public types or error variants.

- **Groups.** `WireType::SGroup` (3) and `WireType::EGroup` (4) are real wire types. Field readers return `FieldValue::SGroup` / `FieldValue::EGroup` (tag only; the body is not bundled into the start value). After `SGroup`, discard with `skip_until` + `until_group_ends`, preserve with slice `take_until` or Read `copy_until` + the same predicate (body through matching `EGroup`; start tag already consumed), parse body fields with `group_body` / `GroupBody` (depth-only nesting; nested end field numbers are the caller's responsibility), or read nested fields until `EGroup` by hand. Writers emit the tag alone for those variants. Internally: `WireCursor` is fragment-level only (`remaining` / `read_varint_with_raw` / fixed / `skip_bytes` / `copy_bytes`; small defaults `read_varint` / `read_tag` / `skip_field`). Multi-field skip/copy are module helpers, not trait methods (`wire_skip_until` / `wire_copy_until`). No `Option` — empty is `UnexpectedEof`; soft end is `remaining() == 0`. `BufWireCursor<L>` adds `read_bytes` and default `parse_field`. Do not treat skipping or preserving as automatically done by `parse_field`.
- **Size constants are not enforced.** `MAX_MESSAGE_SIZE` and `MAX_STRING_SIZE` are public and unused by field readers. Streaming readers pass the length prefix to the payload closure. A slice read returns `UnexpectedEof` when the buffer is shorter than that prefix. Do not document the constants as a parse limit until a reader checks them.
- **`Varint::try_to_sint32` error payload.** On failure, `ProtobufError::VarintDowncastOutOfRange.value` is the ZigZag-decoded `i64` cast with `as u64`, not the raw varint. `try_to_uint32` and `try_to_int32` report the raw varint. Keep that difference in mind before changing the `value` field; callers may already match on it.
