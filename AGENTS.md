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

- **Sync only.** No async public API, no `StreamExt*` traits. Callers buffer bytes and use sync parse. There is no tag/varint `*_partial` / `*_resume`; a varint is at most 10 bytes, so chunked callers buffer until a terminator or `MAX_VARINT_BYTES` then call the complete-read APIs (same model as fixed-width wire types).
- **No field-level partial/resume** (`read_field_partial`).
- **No full message types.** Keep the surface to constants, `Varint`, `Tag`, `Field` / `FieldValue`, and extension traits.
- **Do not confuse Rust types with protobuf types.** Name conversions after the protobuf type (`from_sint32`, `to_uint64`), not the Rust integer.
- Minimize dependencies. `thiserror` is the expected exception.
- **`len` is only on `ReadExtProtobuf` field methods.** `read_protobuf_fields(self, len)` / `read_protobuf_fields_with(self, len, handler)` take a message byte budget tracked by [`LimitedRead`](src/field/read/limited.rs) over the owned reader (not nested `std::io::Take`). `ReadExtVarint` / `ReadExtTag` methods do not take `len`; reader EOF (or the caller's bound such as `LimitedRead`) is the bound. `AsRefExtProtobuf` has no extra `len` argument; the slice length is the bound. Do not reintroduce byte-iterator input traits (`IteratorExt*` / `TryIteratorExt*`).
- **Streaming length-delimited payloads use `LenPayloadHandler`.** `ReadExtProtobuf::read_protobuf_fields_with` takes `F: LenPayloadHandler<R>` (closures `FnMut(FieldNumber, &mut LimitedRead<'_, R>) -> Result<L>` implement it). Once per Len field the reader `reborrow_len`s the payload budget and calls `handle_len`; unread body bytes are skipped afterward. `L` is entirely the handler's choice (bytes, folded struct, nested fields, …) — do not require `AsMut<[u8]>`. Default `read_protobuf_fields` uses `VecLenPayload` and still yields `Field<Vec<u8>>`. Nested messages call `LimitedRead::read_protobuf_fields` / `parse_field` on the body without wrapping another adapter. Do not box the handler. `AsRefExtProtobuf` still returns `Field<&[u8]>` and stays zero-copy.
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

- **Groups / `parse_field`.** Skip (`skip_until` + `until_group_ends`), preserve (`take_until` / `copy_until`), and parse (`group_body` / `GroupBody`) helpers are shipped. What remains deferred: `parse_field` / `BufWireCursor` do **not** automatically skip or preserve a group body after an `SGroup` tag — callers must use those helpers (or walk fields by hand). Do not treat skipping or preserving as done by `parse_field`.
- **Size constants are not enforced.** `MAX_MESSAGE_SIZE` and `MAX_STRING_SIZE` are public and unused by field readers. Streaming readers pass the length prefix to the payload closure. A slice read returns `UnexpectedEof` when the buffer is shorter than that prefix. Do not document the constants as a parse limit until a reader checks them.
- **`Varint::try_to_sint32` error payload.** On failure, `ProtobufError::VarintDowncastOutOfRange.value` is the ZigZag-decoded `i64` cast with `as u64`, not the raw varint. `try_to_uint32` and `try_to_int32` report the raw varint. Keep that difference in mind before changing the `value` field; callers may already match on it.
