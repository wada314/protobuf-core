# protobuf-core

A primitive utility library for Protocol Buffers in Rust.

This library provides common definitions, constants, enums, and basic logic for implementing Protocol Buffers. It is designed to minimize entry barriers for developers who want to implement Protocol Buffers functionality.

## Features

- **Wire Format Constants**: Fundamental protobuf wire format definitions and limits
- **Varint Encoding/Decoding**: Core varint operations with support for all protobuf integer types
- **Tag Operations**: Tag construction and parsing (field number + wire type)
- **Field I/O**: Low-level utilities for reading and writing raw protobuf fields
- **Field Number Validation**: Type-safe field number handling with range validation

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
protobuf-core = "0.2.0"
```

### Feature Flags

- `read` (enabled by default): Enables field reading utilities
- `write` (enabled by default): Enables field writing utilities

You can use features independently:

```toml
[dependencies]
protobuf-core = { version = "0.2.1", default-features = false, features = ["read"] }
```

## Quick Start

### Reading Fields

```rust
use protobuf_core::{Field, IteratorExtProtobuf};

let bytes = vec![0x08, 0x96, 0x01]; // field 1: 150
let fields: Vec<Field<Vec<u8>>> = bytes
    .into_iter()
    .protobuf_fields()
    .collect::<Result<Vec<Field<Vec<u8>>>, _>>()
    .unwrap();
```

### Writing Fields

```rust
use protobuf_core::{WriteExtProtobuf, Field, FieldValue, FieldNumber};

let mut buffer = Vec::new();
let field: Field<Vec<u8>> = Field::new(
    FieldNumber::try_from(1)?,
    FieldValue::from_uint64(150)
);
buffer.write_protobuf_field(&field)?;
# Ok::<(), protobuf_core::ProtobufError>(())
```

## Design Philosophy

This library provides **building blocks** for implementing Protocol Buffers, not a complete message parser or serializer. It focuses on:

- **Low-level primitives**: Raw field I/O without semantic interpretation
- **Flexibility**: Support for both owned and borrowed data
- **Minimal dependencies**: Only depends on `thiserror` for error handling
- **Clear API**: Trait-based extension methods following Rust conventions

For comprehensive API documentation, see [docs.rs](https://docs.rs/protobuf-core).

## License

Licensed under the Apache License, Version 2.0.
