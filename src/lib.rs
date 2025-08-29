//! Protocol Buffers core utility library.
//!
//! This library provides common definitions, constants, enums, and trivial logic
//! for implementing Protocol Buffers in any programming language.

pub mod descriptor;
pub mod field_number;
pub mod tag;
pub mod variant;
pub mod wire_format;

// Re-export commonly used items for convenience
pub use self::field_number::FieldNumber;
pub use self::tag::{build_tag, parse_tag, read_tag};
pub use self::variant::{read_variant, Variant};
pub use self::wire_format::{WireType, MAX_FIELD_NUMBER, MAX_MESSAGE_SIZE, MIN_FIELD_NUMBER};

/// Integrated error type for all protobuf operations.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ProtobufError {
    /// Field number is out of the valid range [1, 2^29 - 1].
    #[error("Field number {value} is out of range (must be between 1 and 536870911)")]
    FieldNumberOutOfRange { value: u32 },

    /// Wire type value is invalid (must be 0-5).
    #[error("Invalid wire type: {value} (must be between 0 and 5)")]
    InvalidWireType { value: u8 },

    /// Variant value is out of the representable range when downcasting.
    #[error("Variant value {value} is out of range for the target type: {target_type}")]
    VariantDowncastOutOfRange {
        value: u64,
        target_type: &'static str,
    },

    /// Tag contains invalid field number or wire type.
    #[error("Malformed tag: field number {field_number} (out of range), wire type {wire_type}")]
    MalformedTag { field_number: u32, wire_type: u8 },
}

/// Type alias for Result with our fixed error type.
pub type Result<T> = std::result::Result<T, ProtobufError>;
