# AGENTS.md

Instructions for coding agents working in this repository. This file is the [AGENTS.md](https://agents.md/) entry point. [`README.md`](README.md) is for humans.

## Overview

`protobuf-core` is a **wire-format primitives** crate: tags, varints, field numbers, and raw field I/O. It is not a message library and does not implement `google.protobuf.*`.

Edition 2024. Features `read` and `write` are on by default and can be used independently.

## Build, test, lint

```sh
cargo test
cargo test --no-default-features --features read
cargo test --no-default-features --features write
cargo fmt
cargo clippy --all-targets
```

## Constraints (do not reverse these)

- **Sync only.** No async public API, no `StreamExt*` traits. Callers buffer bytes and use sync parse (including tag/varint `*_partial` / `*_resume`).
- **No field-level partial/resume** (`read_field_partial`). Compose from tag + varint primitives if needed.
- **No full message types.** Keep the surface to constants, `Varint`, `Tag`, `Field` / `FieldValue`, and extension traits.
- **Do not confuse Rust types with protobuf types.** Name conversions after the protobuf type (`from_sint32`, `to_uint64`), not the Rust integer.
- Minimize dependencies. `thiserror` is the expected exception.
- `FieldNumber::try_new(u32) -> Result<Self, u32>` returns the bad input (not `ProtobufError`) so it can stay `const fn`. Map to `ProtobufError::FieldNumberOutOfRange` at I/O boundaries.

## Code style

- Imports: `crate::` / `self::` for this crate; other paths start with `::`.
- Module files use `foo.rs`, not `foo/mod.rs`.
- Comments in English.

## Docs

- User API: [`README.md`](README.md), [`CHANGELOG.md`](CHANGELOG.md), rustdoc on `src/lib.rs`.
- Older design discussion (not the agent entry point): [`AI_REFERENCES.md`](AI_REFERENCES.md). Put new durable instructions here in `AGENTS.md`.
