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
- **No full message types.** Public surface is wire primitives and raw field I/O: constants, `Varint`, `Tag`, `Field` / `FieldValue`, extension traits, plus streaming Len helpers (`LenBody`, `LenBodyRead`, `LenHandler`, `VecLenPayload`, `FieldIter`, packed iters). Do not add generated-message / `google.protobuf.*` APIs here.
- **Do not confuse Rust types with protobuf types.** Name conversions after the protobuf type (`from_sint32`, `to_uint64`), not the Rust integer.
- Minimize dependencies. `thiserror` is the expected exception.
- **`len` is only on `ReadExtProtobuf` field methods.** `read_protobuf_fields(&mut self, len: u32)` / `read_protobuf_fields_with` are the only public streaming entrances: they borrow the reader into a crate-built `LenBody<'a, R>` held by `FieldIter<'_, R, LenBody<'_, R>, …>`. There is no public `LenBody::new`. `WireCursor::remaining` / `skip_bytes` / `read_bytes` and `FieldIter::remaining` / `LenBody::remaining` are `u32` (aligned with Len length prefixes and `MAX_MESSAGE_SIZE`). `ReadExtVarint` / `ReadExtTag` methods do not take `len`; reader EOF (or the caller's bound such as `std::io::Take`) is the bound. `AsRefExtProtobuf` has no extra `len` argument; the slice length is the bound (`remaining` saturates at `u32::MAX` if a host slice is larger). Do not reintroduce byte-iterator input traits (`IteratorExt*` / `TryIteratorExt*`).
- **`LenBody` is not `Read`.** Bytes/string via consuming `into_read(self)` → `LenBodyRead` (`Read`); packed scalars via `varints` / `fixed32s` / `fixed64s`; nested messages via `fields` / `fields_with`. `FieldIter` handlers are `F: LenHandler<R>` (`VecLenPayload` default; closures get a blanket). Public custom Len building is `fields_with` / `read_protobuf_fields_with` only (`FnMut`); wrap a stateful handler as `|n, body| h.handle(n, body)`. Child Len bodies are created only via `pub(crate) scope(n, leftover)`; on `Drop`, unread count is written to `leftover`, then Len parse discards that many bytes (no I/O in `Drop`). Top-level `new` has no leftover slot.
- **Recursive message decode stays on fixed `R`.** Enter with `read_protobuf_fields_with`; nested Len bodies are `LenBody<'_, R>` by value — prefer `fn parse<R: Read + ?Sized>(body: LenBody<'_, R>)` that re-enters with `body.fields_with(|…, nested| parse(nested))`. Covered by `direct_recursive_node_via_len_body_scope` / `indirect_recursive_a_b_via_len_body_scope` in `len_body.rs`.
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
- **Size constants are mostly not enforced.** `MAX_STRING_SIZE` is unused by field readers. Streaming budgets use public `LenBody` (`u32` limit); protobuf message `len` is also contracted to `<= MAX_MESSAGE_SIZE` (docs + `debug_assert` at `read_protobuf_fields*`); release builds do not reject oversized `len`. Streaming readers pass the length prefix to the Len handler. A slice read returns `UnexpectedEof` when the buffer is shorter than that prefix. Do not document the constants as a hard parse limit until a release-path reader checks them.
- **`Varint::try_to_sint32` error payload.** On failure, `ProtobufError::VarintDowncastOutOfRange.value` is the ZigZag-decoded `i64` cast with `as u64`, not the raw varint. `try_to_uint32` and `try_to_int32` report the raw varint. Keep that difference in mind before changing the `value` field; callers may already match on it.
