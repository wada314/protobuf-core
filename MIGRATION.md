# Migration Guide

This guide helps you migrate from protobuf-core 0.0.1 to 0.1.0.

## Breaking Changes

### 1. `read_tag()` Function → Trait Methods

The standalone `read_tag()` function has been removed in favor of trait methods.

**Before (0.0.1)**:
```rust
use protobuf_core::read_tag;

let mut iter = bytes.into_iter();
let tag = read_tag(&mut iter)?;
```

**After (0.1.0)**:
```rust
use protobuf_core::IteratorExtTag;

let mut iter = bytes.into_iter();
let tag = iter.read_tag()?;
```

**For different iterator types**:
```rust
// For Iterator<Item = u8>
use protobuf_core::IteratorExtTag;
let tag = iter.read_tag()?;

// For Iterator<Item = Result<u8, E>> (e.g., from Read::bytes())
use protobuf_core::TryIteratorExtTag;
let tag = iter.read_tag()?;

// For std::io::Read
use protobuf_core::ReadExtTag;
let tag = reader.read_tag()?;
```

### 2. `Field` and `FieldValue` Generic Parameters

`Field` and `FieldValue` are now generic over the length-delimited value type.

**Before (0.0.1)**:
```rust
let field: Field = Field::new(field_number, value);
let value: FieldValue = FieldValue::from_uint64(150);
```

**After (0.1.0)**:
```rust
// Type is usually inferred, but can be explicit:
let field: Field<Vec<u8>> = Field::new(field_number, value);
let value: FieldValue<Vec<u8>> = FieldValue::from_uint64(150);

// For borrowed data from slices:
let field: Field<&[u8]> = slice.read_protobuf_fields().next()?.unwrap();
```

**When type inference fails**:
```rust
// Sometimes you need to specify the type parameter:
let fields: Vec<Field<Vec<u8>>> = reader
    .read_protobuf_fields()
    .collect::<Result<Vec<_>, _>>()?;
```

### 3. `FieldValue::from_bytes()` and `from_string()` Location

These methods are now only available on `FieldValue<Vec<u8>>`, not on the generic `FieldValue<L>`.

**Before (0.0.1)**:
```rust
let value = FieldValue::from_bytes(data);
let value = FieldValue::from_string(s);
```

**After (0.1.0)**:
```rust
// Still works the same way - the type is inferred:
let value = FieldValue::from_bytes(data);  // FieldValue<Vec<u8>>
let value = FieldValue::from_string(s);    // FieldValue<Vec<u8>>

// If you need to be explicit:
let value: FieldValue<Vec<u8>> = FieldValue::from_bytes(data);
```

**Note**: These methods are not available for `FieldValue<&[u8]>` since they require owned data.

## New Features

### Zero-Copy Parsing from Slices

You can now parse fields from slices without allocating:

```rust
use protobuf_core::AsRefExtProtobuf;

let slice = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];

// Returns Field<&[u8]> - no allocation!
for field_result in slice.read_protobuf_fields() {
    let field = field_result?;
    match field.value {
        FieldValue::Varint(v) => {
            // Process varint
        }
        FieldValue::Len(data) => {
            // data is &[u8] - borrowed from the original slice
        }
        _ => {}
    }
}
```

### Multiple Reading Sources

You can now read from different sources:

```rust
// From Iterator<Item = u8>
use protobuf_core::IteratorExtProtobuf;
let fields = bytes.into_iter().protobuf_fields().collect::<Result<Vec<_>, _>>()?;

// From Iterator<Item = Result<u8, E>>
use protobuf_core::TryIteratorExtProtobuf;
use std::io::Cursor;
let reader = Cursor::new(data);
let fields = reader.bytes().protobuf_fields().collect::<Result<Vec<_>, _>>()?;

// From AsRef<[u8]> (zero-copy)
use protobuf_core::AsRefExtProtobuf;
let fields = slice.read_protobuf_fields().collect::<Result<Vec<_>, _>>()?;

// From std::io::Read (existing)
use protobuf_core::ReadExtProtobuf;
let fields = reader.read_protobuf_fields().collect::<Result<Vec<_>, _>>()?;
```

## Common Migration Patterns

### Pattern 1: Reading Fields from Byte Iterator

**Before**:
```rust
use protobuf_core::ReadExtProtobuf;
use std::io::Cursor;

let data = vec![0x08, 0x96, 0x01];
let reader = Cursor::new(data);
let fields = reader.read_protobuf_fields().collect::<Result<Vec<_>, _>>()?;
```

**After** (more efficient):
```rust
use protobuf_core::IteratorExtProtobuf;

let data = vec![0x08, 0x96, 0x01];
let fields = data.into_iter().protobuf_fields().collect::<Result<Vec<_>, _>>()?;
```

### Pattern 2: Zero-Copy Parsing

**Before** (requires allocation):
```rust
use protobuf_core::ReadExtProtobuf;
use std::io::Cursor;

let slice = &[0x08, 0x96, 0x01][..];
let reader = Cursor::new(slice);
let fields = reader.read_protobuf_fields().collect::<Result<Vec<_>, _>>()?;
```

**After** (zero-copy):
```rust
use protobuf_core::AsRefExtProtobuf;

let slice = &[0x08, 0x96, 0x01][..];
let fields = slice.read_protobuf_fields().collect::<Result<Vec<_>, _>>()?;
// fields contains Field<&[u8]> - no allocation!
```

## Questions?

If you encounter issues during migration, please:
1. Check the [CHANGELOG.md](CHANGELOG.md) for detailed change descriptions
2. Review the [API documentation](https://docs.rs/protobuf-core/)
3. Open an issue on the repository
