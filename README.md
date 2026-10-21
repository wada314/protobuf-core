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

`Read` and byte iterators take a [`LenBuf`](https://docs.rs/protobuf-core/latest/protobuf_core/trait.LenBuf.html) payload type. `Vec<u8>` implements it with no extra feature. Storing that payload in `Vec<u8, A>` with a custom allocator requires the `nightly` Cargo feature and a nightly compiler. A slice read stays borrowed and does not use that trait.

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
