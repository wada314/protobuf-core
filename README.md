# protobuf-core

---

## ⚠️ NOT A GOOGLE OFFICIAL PRODUCT

**This library is not an official Google product.**  
Google provides its own Protocol Buffers implementation for Rust (see [protocolbuffers/protobuf](https://github.com/protocolbuffers/protobuf) and official Rust support). This project is an **independent** library.

---

A primitive utility library for Protocol Buffers in Rust.

This library provides common definitions, constants, enums, and basic logic for implementing Protocol Buffers. It is designed to minimize entry barriers for developers who want to implement Protocol Buffers functionality.

## Features

- **Wire Format Constants**: Fundamental protobuf wire format definitions and limits
- **Varint Encoding/Decoding**: Core varint operations with support for all protobuf integer types
- **Tag Operations**: Tag construction and parsing (field number + wire type)
- **Field I/O**: Low-level utilities for reading and writing raw protobuf fields
- **Field Number Validation**: Type-safe field number handling with range validation

## Quick Start

### Reading Fields

```rust
use protobuf_core::{AsRefExtProtobuf, Field};

let bytes: &[u8] = &[0x08, 0x96, 0x01]; // field 1: 150
let fields: Vec<Field<&[u8]>> = bytes
    .read_protobuf_fields()
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
```

The example above is `AsRefExtProtobuf`: `read_protobuf_fields` borrows `Field<&[u8]>` (the slice length is the bound). `ReadExtProtobuf::read_protobuf_fields(len)` / `read_protobuf_fields_with(len, handler)` take an explicit host `usize` message byte budget. Lower-level `ReadExtVarint` / `ReadExtTag` methods do not take `len`; they read until that reader's EOF (or the caller's bound such as `std::io::Take`). Length-delimited payloads default to `Vec<u8>` via `VecLenPayload`. The `*_with` handler is `FnMut(FieldNumber, &mut R, u32) -> Result<(L, u32)>` (Len payload length / consumed as `u32`) and chooses how to consume each Len body. The library skips `length - consumed` afterward. Nested messages call `read_protobuf_fields*` again on `&mut R`. Byte iterators are not supported; use a slice or a `Read` (with `len` for field APIs).

Deprecated group wire types (`SGroup` / `EGroup`) are returned as tag-only `FieldValue` variants. The group body stays on the cursor; discard via `skip_until` + `until_group_ends`, preserve via slice `take_until` / Read `copy_until` with the same predicate, or parse with `group_body` — the start tag is already consumed, so write it yourself first when building an unknown-field blob. Prefer `until_group_ends` for skip/preserve (full field-number stack); `group_body` is depth-only and does not check nested end-group numbers — nest another `group_body` when you see an inner `SGroup`.

```rust
use protobuf_core::{AsRefExtProtobuf, FieldValue, WriteExtProtobuf, until_group_ends};

let bytes = [0x0B, 0x10, 0x01, 0x0C, 0x18, 0x07];
let mut fields = bytes.as_slice().read_protobuf_fields();
let start = fields.next().unwrap().unwrap();
assert!(matches!(start.value, FieldValue::SGroup));

let mut preserved = Vec::new();
preserved.write_protobuf_field(&start).unwrap();
let body = fields.take_until(until_group_ends(start.field_number)).unwrap();
preserved.extend_from_slice(body);
assert_eq!(&preserved[..], &bytes[..bytes.len() - 2]);
```

### Writing Fields

```rust
use protobuf_core::{WriteExtProtobuf, Field, FieldValue, FieldNumber};

fn main() -> Result<(), protobuf_core::ProtobufError> {
    let mut buffer = Vec::new();
    let field: Field<Vec<u8>> = Field::new(
        FieldNumber::try_new(1).unwrap(),
        FieldValue::from_uint64(150),
    );
    buffer.write_protobuf_field(&field)?;
    Ok(())
}
```

## Design Philosophy

This library provides **building blocks** for implementing Protocol Buffers, not a complete message parser or serializer. It focuses on:

- **Low-level primitives**: Raw field I/O without semantic interpretation
- **Flexibility**: Support for both owned and borrowed data
- **Minimal dependencies**: Only depends on `thiserror` for error handling
- **Clear API**: Trait-based extension methods following Rust conventions

For comprehensive API documentation, see [docs.rs](https://docs.rs/protobuf-core).

## License

Copyright 2025-2026 Shohei Wada.

Licensed under the Apache License, Version 2.0.
