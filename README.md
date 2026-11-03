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

Give the library a byte slice. It walks the message one field at a time.
Each step yields a `Result` of a `Field`: a field number plus a raw value.
On a slice, length-delimited payloads borrow as `&[u8]`.

```rust
use protobuf_core::{AsRefExtProtobuf, Field, FieldValue};

let bytes: &[u8] = &[0x08, 0x96, 0x01]; // field 1 = 150

let mut fields = bytes.read_protobuf_fields();
// `next()` → Option<Result<Field<&[u8]>, _>>
// first unwrap: Some (end of message would be None)
// second unwrap: Ok (wire/parse failure would be Err)
let field: Field<&[u8]> = fields.next().unwrap().unwrap();

assert_eq!(field.field_number.as_u32(), 1);
assert!(matches!(
    field.value,
    FieldValue::Varint(v) if v.to_uint64() == 150
));
// Other `FieldValue` variants: I32 / I64 / Len(&[u8]) / SGroup / EGroup
```

For a streaming `std::io::Read` (you pass the message length yourself), see
`ReadExtProtobuf` on [docs.rs](https://docs.rs/protobuf-core). Groups,
nested messages, and custom Len handlers are documented there too.

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
