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
4. **Basic descriptor structures** - Core types and enums for protobuf descriptors
5. **Edition support** - Support for different protobuf editions

## Scope Limitations
- **No protobuf message implementations** - This library focuses on core utilities, not full message implementations
- **No google.protobuf.* message structures** - These are implementation details that should be handled by specific protobuf libraries
- **Focus on constants and basic logic** - Primarily provides definitions, enums, and fundamental operations

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

6. **Protobuf Descriptor Structures** (`src/descriptor.rs`)
   - **Implementation approach**: Constant-based wrapped i32 types (see design decisions below)
   - **Coverage**: Basic structure outlines for descriptor.proto and plugin.proto
   - **Status**: Skeleton structures completed, field implementation pending

7. **Enum to Constant-Based Wrapped i32 Types Conversion** (`src/descriptor.rs`)
   - **Complete conversion**: All protobuf enums converted from Rust enums to constant-based wrapped i32 types
   - **Files updated**:
     - `descriptor.rs`: 18 enum types converted (Edition, FieldType, FieldLabel, OptimizeMode, CType, JSType, OptionRetention, OptionTargetType, SymbolVisibility, FieldPresence, EnumType, RepeatedFieldEncoding, Utf8Validation, MessageEncoding, JsonFormat, EnforceNamingStyle, DefaultSymbolVisibility, IdempotencyLevel, Semantic)
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

9. **Module Structure Refactoring**
   - **Transition from mod.rs**: Stopped using legacy `mod.rs` files, adopted modern `<module-name>.rs` approach
   - **File organization**: 
     - `src/descriptor.rs` - descriptor traits and utilities
   - **Benefits**: Clearer namespace hierarchy, better maintainability, follows modern Rust practices

10. **Feature Flag Implementation**
    - **Conditional compilation**: Added feature flags for descriptor serialization/deserialization
    - **Feature structure**:
      - `descriptor-encode` - Basic encoding utilities
      - `descriptor-decode` - Basic decoding utilities  
      - `descriptor-full` - Combines both features
    - **Default features**: Enabled `descriptor-encode` and `descriptor-decode` by default
    - **Benefits**: Users get basic functionality without additional configuration, while maintaining optional advanced features

### 🔄 Next Steps
11. **Add fields to descriptor structures** - Complete the basic descriptor type definitions
12. **Enhanced error types** - Expand error handling for additional protobuf operations
13. **Documentation improvements** - Add comprehensive examples and usage guides
14. **Performance optimizations** - Optimize critical paths if needed

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

### Protobuf Message Implementation Strategy
**Decision**: Encapsulated approach with getter/setter methods instead of public fields

**Analysis of approaches considered**:

1. **Open structs (public fields)**
   - ❌ API stability issues - field changes break public API
   - ❌ No validation or business logic control
   - ❌ Difficult to implement protobuf defaults correctly
   - ✅ Simpler implementation
   - ✅ Direct field access

2. **Encapsulated classes (getter/setter methods) - CHOSEN APPROACH**
   - ✅ API stability - internal changes don't affect public interface
   - ✅ Validation and business logic control
   - ✅ Proper protobuf default value handling
   - ✅ Future extensibility
   - ✅ Follows protobuf "message" concept naturally

**Implementation pattern**:
```rust
pub struct FileDescriptorProto {
    name: Option<String>,
    package: Option<String>,
    dependency: Vec<String>,
    // ... other fields
}

impl FileDescriptorProto {
    // Constructor
    pub fn new() -> Self { /* ... */ }
    
    // Getter with default values
    pub fn name(&self) -> String {
        self.name.clone().unwrap_or_default()
    }
    
    // Setter
    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }
    
    // Clear method
    pub fn clear_name(&mut self) {
        self.name = None;
    }
    
    // Has method for optional fields
    pub fn has_name(&self) -> bool {
        self.name.is_some()
    }
}
```

### Getter Return Type Strategy
**Decision**: Return `T` directly instead of `Option<T>` to properly handle protobuf default values

**Key insight**: Many protobuf fields have non-zero default values (e.g., `cc_enable_arenas = true`, `optimize_for = SPEED`). Using `Option<T>` would make it impossible to distinguish between "unset" and "explicitly set to default value".

**Implementation approach**:
```rust
// Instead of returning Option<T>
pub fn java_multiple_files(&self) -> Option<bool> { /* ... */ }

// Return T with proper default handling
pub fn java_multiple_files(&self) -> bool {
    self.java_multiple_files.unwrap_or(false)
}

pub fn cc_enable_arenas(&self) -> bool {
    self.cc_enable_arenas.unwrap_or(true)
}
```

### Setter vs Builder Pattern Decision
**Decision**: Simple setter methods instead of Builder pattern

**Rationale**: 
- **Primary use case**: protoc plugin development where users mainly read descriptor messages
- **Modification needs**: Limited to `CodeGeneratorResponse` construction
- **Complexity trade-off**: Builder pattern adds unnecessary complexity for the target use case
- **Performance**: No significant performance benefit for the expected usage patterns

**Implementation approach**: Provide basic setter methods for essential operations, focus on read-only access for descriptor messages.

### Backend Architecture Strategy
**Decision**: Type-based predefined serialization/deserialization instead of custom closures per field

**Approach**: Create a common backend that handles serialization/deserialization based on field type information rather than custom closures for each field.

**Key components**:
```rust
pub struct FieldMetadata {
    pub number: u32,
    pub wire_type: WireType,
    pub field_label: FieldLabel,    // Optional/Repeated/Required
    pub value_type: ValueType,      // Int32/String/Message/etc.
    pub default_value: DefaultValue,
    pub getter: fn(&dyn std::any::Any) -> &dyn std::any::Any,
    pub setter: fn(&mut dyn std::any::Any) -> &mut dyn std::any::Any,
}

pub enum FieldLabel {
    Optional, Repeated, Required,
}

pub enum ValueType {
    Int32, Int64, Bool, String, Bytes, 
    Enum(String), Message(String),
}

pub enum DefaultValue {
    None, Bool(bool), Int32(i32), String(String), EmptyVec,
}
```

**Benefits**:
- **Efficiency**: No custom closures per field
- **Maintainability**: Type-specific logic centralized
- **Consistency**: Same type uses same serialization logic
- **Extensibility**: Easy to add new types
- **Debugging**: Clear type information for troubleshooting

**Alternative considered**: Custom closures per field with `std::any::Any` for dynamic access
- **Rejected**: Performance overhead and complexity outweigh benefits for the target use case

### Field Metadata Macro Implementation Strategy
**Decision**: Flat macro structure instead of nested helper macros due to Rust's macro expansion order

**Key insight**: Rust's nested macro expansion order is "unnatural" - inner macros expand first, making helper macro approach problematic for type recognition.

**Implementation approach**: Single `define_metadata!` macro with inline logic instead of separate helper macros:
```rust
#[macro_export]
macro_rules! define_metadata {
    (
        $(
            $field_type:ident $field_name:ident: $rust_type:ty = $field_number:literal;
        )*
    ) => {
        pub fn metadata() -> crate::descriptor::MessageMetadata {
            crate::descriptor::MessageMetadata {
                fields: vec![
                    $(
                        crate::descriptor::FieldMetadata {
                            number: $field_number,
                            wire_type: match stringify!($rust_type) {
                                "String" => crate::wire_format::WireType::Len,
                                "bool" => crate::wire_format::WireType::Varint,
                                // ... other type mappings
                            },
                            field_label: match stringify!($field_type) {
                                "required" => crate::descriptor::FieldLabel::Required,
                                "optional" => crate::descriptor::FieldLabel::Optional,
                                "repeated" => crate::descriptor::FieldLabel::Repeated,
                            },
                            // ... other field metadata
                        }
                    ),*
                ],
            }
        }
    };
}
```

**Benefits**:
- **Type recognition**: Works correctly with Rust's macro system
- **Maintainability**: Single macro definition, easier to understand and modify
- **Performance**: No nested macro expansion overhead
- **Debugging**: Clearer error messages and easier troubleshooting

**Usage example**:
```rust
impl NamePart {
    define_metadata! {
        required name_part: String = 1;
        required is_extension: bool = 2;
    }
}
```

### Field Label Terminology Correction
**Decision**: Use `FieldLabel` instead of `FieldType` to align with official Protocol Buffers terminology

**Official reference**: [Protocol Buffers Language Guide](https://protobuf.dev/programming-guides/editions/) and [descriptor.proto](https://protobuf.dev/programming-guides/editions/) use "label" for field cardinality.

**Terminology mapping**:
- **Before**: `FieldType` enum (confusing with value types)
- **After**: `FieldLabel` enum (clear cardinality meaning)
- **Field name**: `field_type` → `field_label`

**Official protobuf definition**:
```protobuf
message FieldDescriptorProto {
  enum Label {
    LABEL_OPTIONAL = 1;
    LABEL_REPEATED = 3;
    LABEL_REQUIRED = 2;
  }
  
  optional Label label = 4;
}
```

**Benefits**:
- **Clarity**: Eliminates confusion between field labels and value types
- **Standards compliance**: Matches official protobuf terminology
- **Documentation alignment**: Consistent with official guides and examples
- **Developer experience**: More intuitive for protobuf developers

## Official Protocol Buffer Documentation:
https://protobuf.dev/

