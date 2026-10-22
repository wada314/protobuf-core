# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `ReadExtProtobuf::read_protobuf_fields_with`, `IteratorExtProtobuf::protobuf_fields_with`, and `TryIteratorExtProtobuf::protobuf_fields_with`. Each takes `FnMut(usize) -> Result<L, ProtobufError>` where `L: AsMut<[u8]>`. The closure is called once per length-delimited field with the payload length. It returns at least that many initialized bytes. The reader writes the first `len` bytes and leaves any suffix. A shorter buffer panics in every build. `Err` fails that field. The closure is stored by value on the iterator. Capture an allocator in the closure when the buffer needs one. If the payload bytes run out, that buffer is dropped before the reader returns `UnexpectedEof`.
- The streaming iterator types take that closure as a second type parameter, defaulting to `fn(usize) -> Result<Vec<u8>, ProtobufError>`. `read_protobuf_fields` and `protobuf_fields` still yield `Field<Vec<u8>>`.
- `AsRefExtProtobuf` still returns `Field<&[u8]>`.

## [0.4.0]

Breaking removal of the `read` and `write` Cargo features. Manifests that set `default-features = false` or selected those features must drop them. Callers that used the default features keep the same types.

### Removed

- Cargo features `read` and `write`. Field, tag, and varint I/O is always compiled. Selecting those features, or disabling defaults to turn one side off, no longer changes the build.

## [0.3.0]

Breaking field-number and error-enum changes. Callers that matched `ProtobufError` exhaustively, or that used `FieldNumber::try_new` / `TryFrom` errors as `ProtobufError`, must update.

### Changed

- `FieldNumber::try_new` is `const fn` and returns `Result<Self, u32>` (the rejected input) instead of `Result<Self, ProtobufError>`.
- `TryFrom<u32> for FieldNumber` uses `Error = u32`. `TryFrom<i32> for FieldNumber` uses `Error = i32`, and returns that original `i32` for both negative values and out-of-range magnitudes.
- `ProtobufError::FieldNumberOutOfRange.value` is `i64` instead of `String`. Tag parsing still reports this variant, converting the rejected `u32` with `i64::from`.
- `FieldNumber::as_u32`, `as_i32`, and `as_usize` are `const fn` and take `self` by value.

### Fixed

- `Varint::from_sint64(i64::MIN)` no longer panics. It ZigZag-encodes as `u64::MAX`.
- A 10-byte varint whose last byte sets more than bit 63 is `ProtobufError::VarintTooLong`. Previously the extra bits were discarded and decoding returned success.

### Removed

- Per-file copyright and Apache boilerplate comments. The crate license remains Apache-2.0 (`LICENSE` and `license` in `Cargo.toml`).
- `ProtobufError::FieldTypeDowncastError`
- `ProtobufError::MalformedTag`

## [0.2.2]

This release is intended to be **backward compatible** with 0.2.0: existing method signatures (`read_varint(self)`, `read_tag(&mut self)`, etc.) and types are unchanged; only new types and methods were added.

### Added

#### Partial / resumable tag and varint reading
- **Varint**: `DecodeState` and `DecodeOutcome` for incremental decoding when input is split across chunks
- **Varint**: `read_varint_partial()` and `read_varint_resume(state)` on `IteratorExtVarint`, `TryIteratorExtVarint`, and `ReadExtVarint`
- **Tag**: `Outcome` (re-exported as `TagOutcome`) with variants `Complete(Tag)`, `Empty`, and `Incomplete(DecodeState)`
- **Tag**: `read_tag_partial()` and `read_tag_resume(state)` on `IteratorExtTag`, `TryIteratorExtTag`, and `ReadExtTag`

- Test case to verify `?Sized` bound on `AsRefExtProtobuf` trait implementation

### Fixed
- Added `?Sized` bound to `AsRefExtProtobuf` trait implementation to support unsized types like `[u8]`

## [0.1.0]

### Added

#### Generic Parameter Support
- `Field<L>` and `FieldValue<L>` now accept a generic parameter `L` for length-delimited values
  - `Vec<u8>` for owned data (from `Iterator<Item = u8>` or `std::io::Read`)
  - `&'a [u8]` for borrowed data (from `AsRef<[u8]>` types)
  - Enables memory-efficient zero-copy parsing from slices

#### New Reading Traits
- `IteratorExtProtobuf`: Read fields from `Iterator<Item = u8>` (returns `Field<Vec<u8>>`)
- `TryIteratorExtProtobuf`: Read fields from `Iterator<Item = Result<u8, E>>` (returns `Field<Vec<u8>>`)
- `AsRefExtProtobuf`: Read fields from `AsRef<[u8]>` types (returns `Field<&'a [u8]>`)
  - Zero-copy parsing suitable for slice-based sources
  - Available for all types implementing `AsRef<[u8]>`

#### New Tag/Varint Traits
- `IteratorExtTag`: Read tags from `Iterator<Item = u8>`
- `TryIteratorExtTag`: Read tags from `Iterator<Item = Result<u8, E>>`
- `TryIteratorExtVarint`: Read varints from `Iterator<Item = Result<u8, E>>`

#### Wire Format Constants
Added constants in `wire_format` module:
- `WIRE_TYPE_MASK`, `FIELD_NUMBER_SHIFT` - Tag encoding utilities
- `MAX_VARINT_BYTES` - Maximum varint size (10 bytes)
- `MAX_1_BYTE_VARINT` through `MAX_9_BYTE_VARINT` - Maximum values for each varint byte length
- `VARINT_CONTINUATION_BIT`, `VARINT_PAYLOAD_MASK` - Varint encoding masks
- `FIXED32_BYTES`, `FIXED64_BYTES` - Fixed-width type sizes
- `MAX_STRING_SIZE` - Maximum string/bytes field size (2 GiB)

#### Error Handling
- `ProtobufError::VarintTooLong` variant added
- `From<Infallible>` implementation for `ProtobufError`

#### Module Visibility
- All internal modules changed from `pub` to `pub(crate)` for stricter visibility control

### Changed

#### Breaking Changes

1. **`read_tag()` function removed**
   - **Previous**: `pub fn read_tag<I>(iter: &mut I) -> Result<Option<Tag>>`
   - **Replacement**: Trait methods are now available:
     - `IteratorExtTag::read_tag()` for `Iterator<Item = u8>`
     - `TryIteratorExtTag::read_tag()` for `Iterator<Item = Result<u8, E>>`
     - `ReadExtTag::read_tag()` for `std::io::Read`
   - **Migration**: Replace function calls with trait method calls:
     ```rust
     // Before
     use protobuf_core::read_tag;
     let tag = read_tag(&mut iter)?;
     
     // After
     use protobuf_core::IteratorExtTag;
     let tag = iter.read_tag()?;
     ```

2. **`Field` and `FieldValue` are now generic**
   - **Previous**: `Field`, `FieldValue`
   - **Current**: `Field<L>`, `FieldValue<L>`
   - **Migration**: Type inference usually handles this, but you may need to specify:
     ```rust
     // Before
     let field: Field = ...;
     
     // After (usually inferred)
     let field: Field<Vec<u8>> = ...;
     // Or for slices
     let field: Field<&[u8]> = ...;
     ```

3. **`FieldValue::from_bytes()` and `from_string()` removed**
   - Length-delimited values are constructed as `FieldValue::Len(...)`.
   - There is no `from_bytes` or `from_string` constructor.

### Documentation

- Comprehensive documentation comments added to all new traits
- Usage examples included in trait documentation
- Module-level documentation improved

## [0.0.1] - Initial Release

Initial release with basic protobuf utilities:

- Wire format constants and definitions
- Varint encoding/decoding
- Tag construction and parsing
- Field number validation
- Basic field reading/writing utilities
- `ReadExtProtobuf` and `WriteExtProtobuf` traits
- `ReadExtTag` and `ReadExtVarint` traits
