# AI References for protobuf-core Project

## Project Overview
This project aims to create a **protobuf utility library** that provides common definitions, constants, enums, and trivial logic implementations. The goal is to minimize entry barriers for developers who want to implement Protocol Buffers.

## Implementation Language
- **Primary implementation**: Rust
- **Focus**: Rust developers who want to implement protobuf functionality

## Project Goals
- Provide common protobuf definitions that can be used across different implementations
- Reduce the complexity and learning curve for new protobuf implementers
- Enable more developers to enjoy implementing protobuf

## Key Concepts
- **Common definitions**: Constants, enums, and basic logic that are universal across protobuf implementations
- **Entry barrier reduction**: Make it easier for developers to start protobuf implementation projects
- **Educational value**: Help developers understand the core concepts of Protocol Buffers

## Planned Implementation Components
1. **Wire format constants** - Basic protobuf wire format definitions
2. **Integer (de)serialization logic** - Core encoding/decoding algorithms
3. **Minimum error types** - Essential error handling for protobuf operations

## Scope Limitations
- **No protobuf message implementations** - This library focuses on core utilities, not full message implementations
- **No google.protobuf.* message structures** - These are implementation details that should be handled by specific protobuf libraries
- **Focus on constants and basic logic** - Primarily provides definitions, enums, and fundamental operations

## Technical Requirements
- **Delivery format**: Rust library providing constants and trivial logic
- **Relationship to existing implementations**: New base for new implementers (no compatibility concerns)
- **External dependencies**: Minimize external dependencies, with exception for `thiserror` crate (de-facto standard for error handling)
- **Performance approach**: Avoid performance optimizations, focus on constants and basic logic
- **Performance considerations**: Only implement performance-critical features when absolutely necessary

## Implementation Progress

### ✅ Completed
1. **Wire Format Constants** (`src/wire_format.rs`)
   - Wire types enum (Varint, I64, Len, SGroup, EGroup, I32)
   - Field number limits (MIN_FIELD_NUMBER, MAX_FIELD_NUMBER)
   - Message size limits (MAX_MESSAGE_SIZE, MAX_STRING_SIZE)
   - Tag encoding constants (WIRE_TYPE_MASK, FIELD_NUMBER_SHIFT)
   - Varint encoding constants (VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK)
   - Size constants for fixed-width types (FIXED32_SIZE, FIXED64_SIZE, etc.)
   - Comprehensive test coverage

2. **Varint Encoding/Decoding** (`src/varint.rs`)
   - Varint type with fixed-size [u8; 8] array
   - **Only supports varint encoding** (variable-length integer encoding)
   - Protobuf type-specific conversion functions:
     - `to_uint64()` - for UInt64 type
     - `to_sint64()` - for SInt64 type (ZigZag decoding)
     - `try_to_uint32()` - for UInt32 type (with range validation)
     - `try_to_sint32()` - for SInt32 type (ZigZag decoding with range validation)
     - `to_int64()` - for Int64 type (non-ZigZag)
     - `try_to_int32()` - for Int32 type (non-ZigZag with range validation)
     - `to_bool()` - for Bool type
   - **Does NOT support** Fixed32/Fixed64, SFixed32/SFixed64, Float/Double types
     - These use different wire types (I32/I64) and fixed-width encoding
   - ZigZag encoding support for signed integers
   - Encoding methods for creating varints from protobuf types:
     - `from_uint64()`, `from_uint32()` - for unsigned integers
     - `from_sint64()`, `from_sint32()` - for signed integers (ZigZag encoding)
     - `from_int64()`, `from_int32()` - for signed integers (non-ZigZag)
     - `from_bool()` - for boolean values

3. **Tag Operations** (`src/tag.rs`)
   - `Tag` struct - represents a protobuf tag with field number and wire type
   - `Tag::to_encoded()` - construct encoded tag value as Varint
   - `Tag::from_encoded()` - parse tag from encoded Varint value
   - `read_tag()` - standalone function to read tag from byte iterator
   - `ReadExtTag` trait - extension trait for reading tags from `std::io::Read` types

4. **Field Number Type** (`src/field_number.rs`)
   - `FieldNumber` wrapper type for validated protobuf field numbers
   - Range validation: 1 to 2^29 - 1 (536,870,911)
   - Helper methods for common use cases:
     - `is_tag_single_byte()` - checks if field number ≤ 15 (single-byte tag encoding)
     - `is_reserved()` - checks if in reserved range 19000-19999
     - `is_in_range()` - checks if field number is within a specific range
     - `encoded_size()` - calculates tag size considering 3-bit shift for wire type
     - `as_u32()`, `as_i32()`, `as_usize()` - conversion methods
   - Proper error handling via `ProtobufError::FieldNumberOutOfRange`
   - Implements `TryFrom<u32>`, `TryFrom<i32>`, `From<FieldNumber>` traits
   - Integration with existing tag and wire_format modules
   - Comprehensive test coverage including edge cases

5. **Unified Error Handling** (`src/lib.rs`)
   - **Integrated error type**: `ProtobufError` enum with specific error variants:
     - `FieldNumberOutOfRange` - field number out of valid range [1, 2^29 - 1]
     - `InvalidWireType` - invalid wire type value (must be 0-5)
     - `VarintDowncastOutOfRange` - varint value out of range when downcasting
     - `FieldTypeDowncastError` - failed to downcast field value to expected type
     - `MalformedTag` - tag contains invalid field number or wire type
     - `UnexpectedEof` - unexpected end of file while parsing
     - `IoError` - I/O error (wrapped from `std::io::Error`)
   - **Custom Result type**: `Result<T>` alias for `std::result::Result<T, ProtobufError>`
   - **Benefits**:
     - Eliminates individual error types per module
     - Simplifies error handling code (no more `crate::ProtobufError`)
     - Provides clear error messages with context (which value, which type)
     - Enables consistent error handling across all protobuf operations
     - Makes code more readable with `Result<T>` instead of `Result<T, ProtobufError>`

6. **Module Structure Refactoring**
   - **Transition from mod.rs**: Stopped using legacy `mod.rs` files, adopted modern `<module-name>.rs` approach
   - **File organization**: 
     - `src/field_number.rs` - field number validation and utilities
     - `src/tag.rs` - tag construction and parsing operations
     - `src/varint.rs` - varint encoding/decoding logic
     - `src/wire_format.rs` - wire format constants and definitions
     - `src/parser.rs` - low-level field reading utilities (feature-gated)
   - **Benefits**: Clearer namespace hierarchy, better maintainability, follows modern Rust practices

7. **Enhanced Varint I/O API** (`src/varint.rs`)
   - **Extension Traits Design**: 
     - `ReadExtVarint` - adds `read_varint()` method to any `std::io::Read` type
     - `WriteExtVarint` - adds `write_varint()` method to any `std::io::Write` type
     - `IteratorExtVarint` - adds `try_collect_varint()` method to any `Iterator<Item = u8>` type
   - **Varint Methods**:
     - `Varint::encode()` - encodes the varint as varint bytes with count
     - `Varint::varint_size()` - calculates the encoded size in bytes
   - **Benefits**:
     - Natural API: `reader.read_varint()`, `writer.write_varint(&varint)`, `iter.try_collect_varint()`, and `varint.encode()`
     - Performance: efficient 1-byte reads from buffered readers
     - Consistency: follows Rust standard library patterns
     - Clean separation: I/O operations are extension methods, encoding is a varint method
     - Clean imports: Uses `std::io::{Read, Write}` imports instead of full paths
     - Iterator integration: `try_collect_varint()` provides `FromIterator`-like functionality for varints
     - API unification: All operations use methods, standalone function `read_tag()` remains for compatibility
   - **Error Handling**: Added `ProtobufError::IoError` variant for I/O errors
   - **Comprehensive Testing**: All APIs tested with both unit tests and doc tests

8. **Field Reading Utilities** (`src/parser.rs`) - **Feature-gated with `parser` feature**
   - **Low-level primitives for reading protobuf fields** - not a complete message parser, but building blocks for parsing
   - **FieldValue enum** - represents raw parsed field values:
     - `Varint(Varint)` - variable-width integers (Int32, Int64, UInt32, UInt64, SInt32, SInt64, Bool, Enum)
     - `I32([u8; 4])` - 32-bit fixed-width values (Fixed32, SFixed32, Float)
     - `I64([u8; 8])` - 64-bit fixed-width values (Fixed64, SFixed64, Double)
     - `Len(Vec<u8>)` - length-delimited values (String, Bytes, embedded messages, packed repeated fields)
   - **Field struct** - represents a parsed field with field number and raw value
   - **ReadExtProtobuf trait** - extension trait for `std::io::Read` types:
     - `read_protobuf_field()` - reads a single protobuf field (tag + value)
     - `read_protobuf_fields()` - returns an iterator over all fields
   - **ProtobufFieldIterator** - iterator for reading protobuf fields sequentially from a reader
   - **Benefits**:
     - Simple, low-level utilities for reading raw protobuf fields
     - Works with any `std::io::Read` source
     - Handles all wire types including deprecated group types
     - Natural iterator-based API for processing fields sequentially
     - Proper error handling with detailed error messages
     - Building blocks for higher-level parsers
   - **Comprehensive Testing**: Covers all wire types and multiple field scenarios

### 🔄 Next Steps
9. **Documentation improvements** - Add comprehensive examples and usage guides
10. **Performance optimizations** - Optimize critical paths if needed
11. **Additional field reading utilities** - Add helper methods for common field value conversions

## Design Decisions

### Type Naming Convention - Avoiding Confusion Between Rust Types and Protobuf Types

**Critical Principle**: Do NOT confuse Rust types with Protobuf types. Even though we must use Rust types (u32, i32, u64, i64, bool) in code to represent Protobuf types (UInt32, Int32, SInt32, UInt64, Int64, SInt64, Bool), they are **NOT in 1-to-1 correspondence**.

**Key Differences**:
- Rust `i32` can represent multiple different Protobuf types:
  - `Int32` (varint, non-ZigZag encoding)
  - `SInt32` (varint, ZigZag encoding)
  - `SFixed32` (fixed 4-byte little-endian, wire type I32)
  - `Enum` (varint encoding)
- Rust `u32` can represent:
  - `UInt32` (varint encoding)
  - `Fixed32` (fixed 4-byte little-endian, wire type I32)
- Rust `i64` can represent:
  - `Int64` (varint, non-ZigZag encoding)
  - `SInt64` (varint, ZigZag encoding)
  - `SFixed64` (fixed 8-byte little-endian, wire type I64)
- Rust `u64` can represent:
  - `UInt64` (varint encoding)
  - `Fixed64` (fixed 8-byte little-endian, wire type I64)
- **Same Rust type, completely different wire encodings**: Protobuf types using the same Rust type can have entirely different wire formats (varint vs fixed-width, ZigZag vs non-ZigZag)

**Conversion Function Naming**:
- Always use explicit Protobuf type names in function names (e.g., `from_sint32()`, `to_uint64()`)
- The Rust types are already clear from function signatures (return types or argument types)
- Example: `to_sint32() -> Result<i32>` - the Rust type `i32` is obvious from the return type, but `sint32` clarifies it uses ZigZag decoding for the Protobuf `SInt32` type
- Example: `from_int32(value: i32)` vs `from_sint32(value: i32)` - both take Rust `i32`, but represent different Protobuf types with different encodings


### File Organization
- `wire_format.rs` - Core constants and wire type definitions
- `varint.rs` - Varint encoding/decoding logic
- `tag.rs` - Tag construction and parsing operations
- `field_number.rs` - Field number validation and utilities
- `parser.rs` - Low-level field reading utilities (feature-gated)


## Official Protocol Buffer Documentation:
https://protobuf.dev/

