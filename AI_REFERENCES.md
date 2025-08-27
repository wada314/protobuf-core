# AI References for protobuf-core Project

## Project Overview
This project aims to create a **language-neutral protobuf utility library** that provides common definitions, constants, enums, and trivial logic implementations. The goal is to minimize entry barriers for developers who want to implement Protocol Buffers in their preferred programming language.

## Implementation Language
- **Primary implementation**: Rust
- **Generated code**: Can target any programming language
- **Focus**: Rust developers who want to implement protobuf functionality

## Project Goals
- Provide common protobuf definitions that can be used across different language implementations
- Reduce the complexity and learning curve for new protobuf implementers
- Enable more developers to enjoy implementing protobuf in their favorite languages
- Create a reference implementation that can be ported to various programming languages

## Key Concepts
- **Language-neutral**: Not tied to any specific programming language
- **Common definitions**: Constants, enums, and basic logic that are universal across protobuf implementations
- **Entry barrier reduction**: Make it easier for developers to start protobuf implementation projects
- **Educational value**: Help developers understand the core concepts of Protocol Buffers

## Planned Implementation Components
1. **Wire format constants** - Basic protobuf wire format definitions
2. **Integer (de)serialization logic** - Core encoding/decoding algorithms
3. **Minimum error types** - Essential error handling for protobuf operations
4. **Descriptor.proto and plugin.proto** - Minimal implementations for code generation support
5. **Edition support** - Support for different protobuf editions

## Technical Requirements
- **Delivery format**: Rust library providing constants and trivial logic
- **Protobuf version support**: All versions (proto2, proto3) and latest editions
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

2. **Variant Encoding/Decoding** (`src/variant.rs`)
   - VariantValue type with fixed-size [u8; 8] array
   - **Only supports varint encoding** (variable-length integer encoding)
   - Protobuf type-specific conversion functions:
     - `to_uint64()` - for UInt64 type
     - `to_sint64()` - for SInt64 type (ZigZag decoding)
     - `to_uint32()` - for UInt32 type
     - `to_sint32()` - for SInt32 type (ZigZag decoding)
     - `to_bool()` - for Bool type
   - **Does NOT support** Fixed32/Fixed64, SFixed32/SFixed64, Float/Double types
     - These use different wire types (I32/I64) and fixed-width encoding
   - ZigZag encoding support for signed integers
   - `read_variant()` function for byte iterator reading

3. **Tag Operations** (`src/tag.rs`)
   - `build_tag()` - construct tag from field number and wire type
   - `parse_tag()` - parse tag into field number and wire type
   - `read_tag()` - read tag from byte iterator

4. **Field Number Type** (`src/field_number.rs`)
   - `FieldNumber` wrapper type for validated protobuf field numbers
   - Range validation: 1 to 2^29 - 1 (536,870,911)
   - Helper methods for common use cases:
     - `is_small()` - checks if field number ≤ 16 (efficient encoding)
     - `is_common()` - checks if field number ≤ 16 (most efficient)
     - `is_reserved()` - checks if in reserved range 19000-19999
     - `is_in_range(min, max)` - custom range checking
     - `tag_encoded_size()` - calculates tag size considering 3-bit shift
     - `tag_size_with_wire_type(wire_type)` - specific tag size calculation
     - `encoded_size()` - field number only size (reference)
   - Proper error handling with `FieldNumberError`
   - Integration with existing tag and wire_format modules
   - Comprehensive test coverage including edge cases

### 🔄 Next Steps
5. **Minimum error types** - Essential error handling for protobuf operations
6. **Descriptor.proto and plugin.proto** - Minimal implementations for code generation support

## Design Decisions

### Type Naming Convention
- **Rust types**: Standard Rust integer types (u32, i64, etc.)
- **Protobuf types**: Explicit protobuf type names (UInt32, SInt64, etc.)
- **Conversion functions**: Always use protobuf type names to avoid confusion

### File Organization
- `wire_format.rs` - Core constants and wire type definitions
- `variant.rs` - Variant encoding/decoding logic (renamed from varint.rs)
- `tag.rs` - Tag construction and parsing operations

## Official Protocol Buffer Documentation:
https://protobuf.dev/

