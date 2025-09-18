//! Protocol Buffers core library
//!
//! This library provides common definitions, constants, enums, and basic logic
//! for implementing Protocol Buffers in Rust.

pub mod field_number;
#[cfg(feature = "parser")]
pub mod parser;
pub mod tag;
pub mod varint;
pub mod wire_format;

pub use self::field_number::FieldNumber;
#[cfg(feature = "parser")]
pub use self::parser::{parse_stream, Field, FieldValue};
pub use self::tag::{read_tag, ReadExtTag, Tag};
pub use self::varint::{IteratorExtVarint, ReadExtVarint, Varint, WriteExtVarint};
pub use self::wire_format::{WireType, MAX_FIELD_NUMBER, MAX_MESSAGE_SIZE, MIN_FIELD_NUMBER};

use ::thiserror::Error;

/// Unified error type for all protobuf operations
#[derive(Error, Debug)]
pub enum ProtobufError {
    #[error("Field number {value} is out of valid range [1, 536_870_911]")]
    FieldNumberOutOfRange { value: String },

    #[error("Invalid wire type: {value} (must be 0-5)")]
    InvalidWireType { value: u8 },

    #[error("Varint value {value} is out of range for target type: {target_type}")]
    VarintDowncastOutOfRange {
        value: u64,
        target_type: &'static str,
    },

    #[error("Failed to downcast field value to expected type: {expected_type}")]
    FieldTypeDowncastError { expected_type: String },

    #[error("Malformed tag: field_number={field_number}, wire_type={wire_type}")]
    MalformedTag { field_number: u32, wire_type: u8 },

    #[error("Unexpected EOF while parsing field")]
    UnexpectedEof,

    #[error("I/O error: {0}")]
    IoError(#[from] ::std::io::Error),
}

/// Custom Result type for protobuf operations
pub type Result<T> = ::std::result::Result<T, ProtobufError>;
