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
     - `is_tag_single_byte()` - checks if field number ≤ 15 (single-byte tag encoding)
     - `is_reserved()` - checks if in reserved range 19000-19999
     - `tag_encoded_size()` - calculates tag size considering 3-bit shift
     - `encoded_size()` - field number only size (reference)
   - Proper error handling with `FieldNumberError`
   - Integration with existing tag and wire_format modules
   - Comprehensive test coverage including edge cases

5. **Unified Error Handling** (`src/lib.rs`)
   - **Integrated error type**: `ProtobufError` enum with specific error variants:
     - `FieldNumberOutOfRange` - field number out of valid range [1, 2^29 - 1]
     - `InvalidWireType` - invalid wire type value (must be 0-5)
     - `VariantDowncastOutOfRange` - variant value out of range when downcasting
     - `MalformedTag` - tag contains invalid field number or wire type
   - **Custom Result type**: `Result<T>` alias for `std::result::Result<T, ProtobufError>`
   - **Benefits**:
     - Eliminates individual error types per module
     - Simplifies error handling code (no more `crate::ProtobufError`)
     - Provides clear error messages with context (which value, which type)
     - Enables consistent error handling across all protobuf operations
     - Makes code more readable with `Result<T>` instead of `Result<T, ProtobufError>`

6. **Protobuf Descriptor Structures** (`src/descriptor/`)
   - **Module structure**: Organized following protobuf package hierarchy
     - `google.protobuf` - Core descriptor definitions
     - `google.protobuf.compiler` - Plugin-related structures
   - **Implementation approach**: Constant-based wrapped i32 types (see design decisions below)
   - **Coverage**: Basic structure outlines for descriptor.proto and plugin.proto
   - **Status**: Skeleton structures completed, field implementation pending

7. **Enum to Constant-Based Wrapped i32 Types Conversion** (`src/descriptor/`)
   - **Complete conversion**: All protobuf enums converted from Rust enums to constant-based wrapped i32 types
   - **Files updated**:
     - `descriptor.rs`: 18 enum types converted (Edition, FieldType, FieldLabel, OptimizeMode, CType, JSType, OptionRetention, OptionTargetType, SymbolVisibility, FieldPresence, EnumType, RepeatedFieldEncoding, Utf8Validation, MessageEncoding, JsonFormat, EnforceNamingStyle, DefaultSymbolVisibility, IdempotencyLevel, Semantic)
     - `plugin.rs`: Feature enum converted
     - `feature_set.rs`: DefaultSymbolVisibility enum converted
     - `extension_range_options.rs`: VerificationState enum converted
   - **Implementation pattern**:
     ```rust
     #[repr(transparent)]
     #[derive(Clone, Copy, PartialEq, Eq, Hash)]
     pub struct TypeName(i32);
     
     impl TypeName {
         pub const VARIANT: Self = Self(value);
         // ... other constants
         
         pub fn new(value: i32) -> Self { Self(value) }
         pub fn value(&self) -> i32 { self.0 }
         pub fn is_known(&self) -> bool { matches!(self.0, value1 | value2 | ...) }
     }
     ```

8. **Manual Debug Implementation for All Wrapped Types**
   - **Custom Debug output**: Provides readable constant names instead of raw i32 values
   - **Pattern matching approach**: Uses `match *self` for correct pattern matching behavior
   - **Output format**: `TypeName::VARIANT` for known values, `TypeName(value)` for unknown values
   - **Example output**:
     - Known: `Edition::EDITION_2023` instead of `Edition(1000)`
     - Unknown: `Edition(9999)` for unrecognized values
   - **Benefits**:
     - Eliminates dependency on `derive_more` crate
     - Provides consistent, readable debug output across all types
     - Maintains zero-cost abstraction with `#[repr(transparent)]`

### 🔄 Next Steps
9. **Add fields to descriptor structures** 
10. **Minimum error types** - Essential error handling for protobuf operations
11. **Descriptor.proto and plugin.proto** - Complete implementations for code generation support

## Design Decisions

### Type Naming Convention
- **Rust types**: Standard Rust integer types (u32, i64, etc.)
- **Protobuf types**: Explicit protobuf type names (UInt32, SInt64, etc.)
- **Conversion functions**: Always use protobuf type names to avoid confusion

### File Organization
- `wire_format.rs` - Core constants and wire type definitions
- `variant.rs` - Variant encoding/decoding logic (renamed from varint.rs)
- `tag.rs` - Tag construction and parsing operations

### Protobuf Enum Implementation Strategy
**Decision**: Use constant-based wrapped i32 types instead of Rust enums

**Analysis of approaches considered**:

1. **Rust enums (original approach)**
   - ❌ Cannot handle unknown protobuf values
   - ❌ Breaks protobuf's dynamic nature
   - ❌ Requires library updates for new protobuf versions
   - ✅ Type safety and pattern matching

2. **`#[non_exhaustive]` enums**
   - ❌ Still requires library updates for new values
   - ❌ Users must wait for library updates
   - ✅ Type safety and pattern matching
   - ✅ Handles unknown values

3. **Constant-based wrapped i32 types (chosen approach)**
   - ✅ Immediate compatibility with new protobuf values
   - ✅ No dependency on library updates
   - ✅ Protobuf specification compliance
   - ✅ Supports match-case syntax (though not exhaustive)
   - ✅ Backward compatibility
   - ✅ Follows Google's official Rust implementation

**Key insight**: Protobuf's dynamic nature means new enum values can be added without library updates. The constant-based approach allows users to use new values immediately, while wrapped types provide some type safety and match-case capability.

**Implementation pattern**:
```rust
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Edition(i32);

impl Edition {
    pub const UNKNOWN: Self = Self(0);
    pub const PROTO2: Self = Self(998);
    pub const PROTO3: Self = Self(999);
    
    pub fn new(value: i32) -> Self { Self(value) }
    pub fn value(&self) -> i32 { self.0 }
    pub fn is_known(&self) -> bool { matches!(self.0, 0 | 998 | 999) }
}

impl std::fmt::Debug for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Edition::UNKNOWN => write!(f, "Edition::UNKNOWN"),
            Edition::PROTO2 => write!(f, "Edition::PROTO2"),
            Edition::PROTO3 => write!(f, "Edition::PROTO3"),
            _ => write!(f, "Edition({})", self.0),
        }
    }
}
```

### Debug Implementation Strategy
**Decision**: Manual Debug implementation instead of derive_more dependency

**Benefits**:
- **Dependency reduction**: Avoids external crate dependency
- **Custom output**: Provides readable constant names instead of raw values
- **Consistency**: All types follow the same debug output pattern
- **Maintainability**: Clear, explicit implementation without magic

**Pattern matching approach**: Uses `match *self` instead of `match self` for correct behavior with wrapped types

## Official Protocol Buffer Documentation:
https://protobuf.dev/

