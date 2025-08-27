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
- **External dependencies**: Minimize external dependencies
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

### 🔄 Next Steps
2. **Integer (de)serialization logic** - Core encoding/decoding algorithms
3. **Minimum error types** - Essential error handling for protobuf operations

## Official Protocol Buffer Documentation:
https://protobuf.dev/

