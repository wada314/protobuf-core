# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - Unreleased

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

## [0.1.0] - Unreleased

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

3. **`FieldValue::from_bytes()` and `from_string()` moved**
   - **Previous**: Available on `impl FieldValue`
   - **Current**: Available only on `impl FieldValue<Vec<u8>>`
   - **Impact**: These methods are now clearly marked as for owned data only
   - **Migration**: No code changes needed if you were using them with `Vec<u8>` data

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
