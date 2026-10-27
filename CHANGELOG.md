# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Additive public API for unknown-field group preserve. Callers that only skip groups need no changes. Version bump left for a separate release PR.

### Added

- `ReadExtVarint::read_varint_with_raw` — decodes a varint while copying its exact wire bytes into a caller-provided `[u8; MAX_VARINT_BYTES]` (including non-canonical encodings).
- `read_varint_with_raw_from_slice` — zero-copy slice counterpart (borrows the wire bytes from the input).
- `FieldIterFromSlice::take_until(pred)` — after an `SGroup` tag was consumed, returns a zero-copy `&[u8]` span of the group body through the matching `EGroup` (use with `until_group_ends`). Exact wire bytes.
- `FieldIterFromRead::copy_until(pred, out)` — same stop semantics as `skip_until` for streaming input; tees exact wire bytes to `out` (including non-canonical varints).
- `GroupBody` / `group_body(field_number)` on field iterators — adapter that yields body fields until the matching outermost `EGroup` (consumed, not yielded). Nesting uses a depth counter only; **nested end-group field numbers are not validated** (caller should nest another `group_body` when seeing an inner `SGroup`). Distinct from `until_group_ends`, which keeps a full field-number stack for skip/preserve. Premature end of input before that `EGroup` is `UnexpectedEof`. After the first `None` or a yielded `Err`, further `next` is unspecified (may read past the group); use `Iterator::fuse` for sticky `None`.
- Contract for skip/preserve mirrors `until_group_ends`: the start-group tag is already consumed (caller writes it to a preserve blob if needed); the helper advances past the matching end-group. Nested groups use the same stack rules. On predicate / decode error, the input cursor may have advanced; `Read` `copy_until` may leave a partial write in `out`.

### Internal

- `WireCursor` is Iterator-like: required fragments (`remaining` / `read_varint_with_raw` / fixed / `skip_bytes` / `copy_bytes`) plus default compose methods (`read_varint` / `read_tag` / `skip_field` / `skip_until` / `copy_field` / `copy_until`). Read-side `read_varint_with_raw` delegates to `ReadExtVarint::read_varint_with_raw`; the slice cursor still borrows the input.

## [0.6.0]

Breaking relative to 0.5.0. Exhaustive matches on `FieldValue` and `ProtobufError` must update. Call sites that used byte-iterator field/varint/tag traits must move to a slice or `Read`. Field `Read` APIs take an explicit message budget `len`; varint/tag `Read` methods do not.

### Added

- `FieldValue::SGroup` and `FieldValue::EGroup` — field readers return these as successful tag-only values (0.5.0 reported group tags as `InvalidWireType`). Writers emit the corresponding tag with no payload. `encoded_size` for the value is `0` (tag size is in `Field::encoded_size`).
- `skip_until(pred)` on `FieldIterFromRead` / `FieldIterFromSlice`. Skips fields (without allocating Len payloads) until `pred(Tag) -> Result<bool>` returns `Ok(true)`. Not on `ReadExtProtobuf` / `AsRefExtProtobuf`: call it on the field iterator returned by `read_protobuf_fields*`.
- `until_group_ends(field_number)` — returns `impl FnMut(Tag) -> Result<bool>` for `skip_until` after an `SGroup` tag was consumed; handles nested groups (stack inside the closure) and reports `MismatchedEndGroup`.
- `ProtobufError::MismatchedEndGroup { expected, got }` when an end-group field number does not match the open start-group during skip.
- `VarintIterFromRead` — return type of `ReadExtVarint::read_varints` (replaces `VarintIterFromTryBytes` / `VarintIterFromBytes`).

### Changed

- `ReadExtProtobuf::read_protobuf_fields(self, len)` and `read_protobuf_fields_with(self, len, make)` take an explicit byte budget `len` and bound the reader with `std::io::Take`. That is the only `Read` decode API that takes `len`.
- `ReadExtVarint` / `ReadExtTag` methods (`read_varint`, `read_varint_partial`, `read_varint_resume`, `read_varints`, and the tag equivalents) do **not** take `len`. The reader's EOF (or an existing `Take` from the field API / caller) is the bound. `read_varints` returns `VarintIterFromRead<&mut Self>` with no `Take` wrapper.
- `FieldIterFromRead` holds `Take<R>`; its `skip_until` has no extra `len` (already bounded).
- `AsRefExtProtobuf` is unchanged: no extra `len` argument; the slice length is the bound.
- `InvalidWireType` remains only for tag low-bits outside 0–5 (for example from `WireType::try_from`). Group wire types are known and return `FieldValue::SGroup` / `FieldValue::EGroup` on the field-read path.

### Internal

- Field I/O shares `WireCursor` (required: `remaining` / varint / fixed / `skip_bytes`; default methods: `read_tag`, `skip_field`, `skip_until`) and `BufWireCursor<L>` (`read_bytes` plus default `parse_field`). Empty input is `UnexpectedEof`; soft end is `remaining() == 0`. `skip_field` returns the `Tag`; group bodies use `skip_until` with `until_group_ends`. Read-side impls are specialized on `Take`.

### Removed

- Iterator-input traits and types: `IteratorExtVarint`, `TryIteratorExtVarint`, `IteratorExtTag`, `TryIteratorExtTag`, `IteratorExtProtobuf`, `TryIteratorExtProtobuf`, `FieldIterFromBytes`, `FieldIterFromTryBytes`, `VarintIterFromBytes`, `VarintIterFromTryBytes`.
- Use `AsRefExtProtobuf` on a slice, or `ReadExtProtobuf` with `len` / `ReadExtVarint`/`ReadExtTag` until EOF, instead of feeding byte iterators.

## [0.5.0]

Breaking iterator return-type renames, plus caller-built length-delimited payloads. Callers that named the 0.4.0 iterator types must update. Callers that only used the default `read_protobuf_fields` / `protobuf_fields` paths keep the same call sites.

### Added

- `ReadExtProtobuf::read_protobuf_fields_with`, `IteratorExtProtobuf::protobuf_fields_with`, and `TryIteratorExtProtobuf::protobuf_fields_with`. Each takes `FnMut(usize) -> Result<L, ProtobufError>` where `L: AsMut<[u8]>`. The closure is called once per length-delimited field with the payload length. It returns at least that many initialized bytes. The reader writes the first `len` bytes and leaves any suffix. A shorter buffer panics. `Err` fails that field. The closure is stored by value on the iterator. Capture an allocator in the closure when the buffer needs one. If the payload bytes run out, that buffer is dropped before the reader returns `UnexpectedEof`.
- The streaming iterator types take that closure as a second type parameter, defaulting to `fn(usize) -> Result<Vec<u8>, ProtobufError>`. `read_protobuf_fields` and `protobuf_fields` still yield `Field<Vec<u8>>`.
- `AsRefExtProtobuf` still returns `Field<&[u8]>`.
- `VarintIterFromBytes` — standalone return type of `IteratorExtVarint::read_varints` (`Item = u8`).
- Crate-root re-export of `FieldIterFromTryBytes` (formerly `ProtobufFieldIteratorFromTryBytes`, previously not re-exported).

### Changed

- `VarintIterator` → `VarintIterFromTryBytes` (return type of `TryIteratorExtVarint::read_varints` and of `ReadExtVarint::read_varints` as `VarintIterFromTryBytes<std::io::Bytes<&mut R>>`).
- `ProtobufFieldIterator` → `FieldIterFromRead`.
- `ProtobufFieldSliceIterator` → `FieldIterFromSlice`.
- `ProtobufFieldIteratorFromBytes` → `FieldIterFromBytes`.
- `ProtobufFieldIteratorFromTryBytes` → `FieldIterFromTryBytes`.

### Removed

- Public `ToResultIterator`. It remains a private adapter inside `VarintIterFromBytes`.

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
