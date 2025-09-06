//! Protocol Buffers core library
//!
//! This library provides common definitions, constants, enums, and basic logic
//! for implementing Protocol Buffers in Rust.

pub mod field_number;
pub mod tag;
pub mod variant;
pub mod wire_format;

pub use field_number::FieldNumber;
pub use tag::{build_tag, parse_tag, read_tag};
pub use variant::{read_variant, Variant};
pub use wire_format::{WireType, MAX_FIELD_NUMBER, MAX_MESSAGE_SIZE, MIN_FIELD_NUMBER};

use thiserror::Error;

/// Unified error type for all protobuf operations
#[derive(Error, Debug)]
pub enum ProtobufError {
    #[error("Field number {value} is out of valid range [1, {}]", 2u32.pow(29) - 1)]
    FieldNumberOutOfRange { value: u32 },

    #[error("Invalid wire type: {value} (must be 0-5)")]
    InvalidWireType { value: u8 },

    #[error("Variant value {value} is out of range for target type: {target_type}")]
    VariantDowncastOutOfRange {
        value: u64,
        target_type: &'static str,
    },

    #[error("Failed to downcast field value to expected type: {expected_type}")]
    FieldTypeDowncastError { expected_type: String },

    #[error("Malformed tag: field_number={field_number}, wire_type={wire_type}")]
    MalformedTag { field_number: u32, wire_type: u8 },
}

/// Custom Result type for protobuf operations
pub type Result<T> = std::result::Result<T, ProtobufError>;
