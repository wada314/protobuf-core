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
protobuf-core = "0.1.0"
```

### Feature Flags

- `read` (enabled by default): Enables field reading utilities
- `write` (enabled by default): Enables field writing utilities

You can use features independently:

```toml
[dependencies]
protobuf-core = { version = "0.1.0", default-features = false, features = ["read"] }
```

## Quick Start

### Reading Fields from Different Sources

This library provides four different approaches for reading protobuf fields, each suited for different data sources:

1. **IteratorExtProtobuf**: 
   - Input: `Iterator<Item = u8>` (byte iterators)
   - Output: `Field<Vec<u8>>` - Len values are owned `Vec<u8>`

2. **TryIteratorExtProtobuf**: 
   - Input: `Iterator<Item = Result<u8, E>>` (iterators with error handling)
   - Output: `Field<Vec<u8>>` - Len values are owned `Vec<u8>`

3. **AsRefExtProtobuf**: 
   - Input: `AsRef<[u8]>` types (slices, arrays, etc.)
   - Output: `Field<&[u8]>` - Len values are borrowed `&[u8]` slices (zero-copy)

4. **ReadExtProtobuf**: 
   - Input: `std::io::Read` types (file readers, network streams, etc.)
   - Output: `Field<Vec<u8>>` - Len values are owned `Vec<u8>`

```rust
use protobuf_core::{IteratorExtProtobuf, TryIteratorExtProtobuf, AsRefExtProtobuf, ReadExtProtobuf};
use std::io::{Cursor, Read};

// From a byte iterator using IteratorExtProtobuf - returns Field<Vec<u8>> (Len values are Vec<u8>)
let bytes = vec![0x08, 0x96, 0x01]; // field 1: 150
let fields: Vec<Field<Vec<u8>>> = bytes
    .into_iter()
    .protobuf_fields()  // IteratorExtProtobuf::protobuf_fields()
    .collect::<Result<Vec<Field<Vec<u8>>>, _>>()
    .unwrap();

// From an iterator with error handling using TryIteratorExtProtobuf - returns Field<Vec<u8>> (Len values are Vec<u8>)
let data = vec![0x08, 0x96, 0x01];
let reader = Cursor::new(data);
let fields: Vec<Field<Vec<u8>>> = reader
    .bytes()  // Iterator<Item = Result<u8, io::Error>>
    .protobuf_fields()  // TryIteratorExtProtobuf::protobuf_fields()
    .collect::<Result<Vec<Field<Vec<u8>>>, _>>()
    .unwrap();

// From a slice using AsRefExtProtobuf - returns Field<&[u8]> (Len values are &[u8], zero-copy)
let slice = &[0x08, 0x96, 0x01][..];
let fields: Vec<Field<&[u8]>> = slice
    .read_protobuf_fields()  // AsRefExtProtobuf::read_protobuf_fields()
    .collect::<Result<Vec<Field<&[u8]>>, _>>()
    .unwrap();

// From a Read source using ReadExtProtobuf - returns Field<Vec<u8>> (Len values are Vec<u8>)
// Note: &[u8] implements Read, so we can use slices directly
let data = &[0x08, 0x96, 0x01][..];
let fields: Vec<Field<Vec<u8>>> = data
    .read_protobuf_fields()  // ReadExtProtobuf::read_protobuf_fields()
    .collect::<Result<Vec<Field<Vec<u8>>>, _>>()
    .unwrap();
```

### Writing Fields

```rust
use protobuf_core::{WriteExtProtobuf, Field, FieldValue, FieldNumber};

let mut buffer = Vec::new();
let field = Field::new(
    FieldNumber::try_from(1)?,
    FieldValue::from_uint64(150)
);
buffer.write_protobuf_field(&field)?;
```

### Reading Tags

```rust
use protobuf_core::IteratorExtTag;

let bytes = vec![0x08]; // tag 1:0 (field 1, wire type 0)
let mut iter = bytes.into_iter();
let tag = iter.read_tag()?.unwrap();
assert_eq!(tag.field_number.as_u32(), 1);
```

### Varint Operations

```rust
use protobuf_core::IteratorExtVarint;

let bytes = vec![0x96, 0x01]; // 150
let mut iter = bytes.into_iter();
let varint = iter.read_varint()?.unwrap();
assert_eq!(varint.to_uint64(), 150);
```

## API Overview

### Core Types

- `Field<L>`: A raw field with field number and value
  - `L` can be `Vec<u8>` for owned data or `&'a [u8]` for borrowed data
- `FieldValue<L>`: Raw field value (Varint, I32, I64, or Len)
- `Tag`: Field number + wire type
- `Varint`: Deserialized varint value
- `FieldNumber`: Validated field number (1 to 2^29 - 1)

### Reading Traits

- `IteratorExtProtobuf`: Read fields from `Iterator<Item = u8>`
- `TryIteratorExtProtobuf`: Read fields from `Iterator<Item = Result<u8, E>>`
- `AsRefExtProtobuf`: Read fields from `AsRef<[u8]>` (returns borrowed references)
- `ReadExtProtobuf`: Read fields from `std::io::Read`

### Writing Traits

- `WriteExtProtobuf`: Write fields to `std::io::Write`

### Tag/Varint Traits

- `IteratorExtTag` / `TryIteratorExtTag` / `ReadExtTag`: Read tags
- `IteratorExtVarint` / `TryIteratorExtVarint` / `ReadExtVarint`: Read varints
- `WriteExtVarint`: Write varints

## Design Philosophy

This library provides **building blocks** for implementing Protocol Buffers, not a complete message parser or serializer. It focuses on:

- **Low-level primitives**: Raw field I/O without semantic interpretation
- **Flexibility**: Support for both owned and borrowed data
- **Minimal dependencies**: Only depends on `thiserror` for error handling
- **Clear API**: Trait-based extension methods following Rust conventions

## License

Licensed under the Apache License, Version 2.0.
