// Copyright 2021 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A primitive utility library for Protocol Buffers in Rust.
//!
//! This library provides common definitions, constants, enums, and basic logic for implementing
//! Protocol Buffers. It is designed to minimize entry barriers for developers who want to
//! implement Protocol Buffers functionality.
//!
//! ## Overview
//!
//! This library provides **building blocks** for implementing Protocol Buffers, not a complete
//! message parser or serializer. It focuses on:
//!
//! - **Low-level primitives**: Raw field I/O without semantic interpretation
//! - **Flexibility**: Support for both owned and borrowed data
//! - **Minimal dependencies**: Only depends on `thiserror` for error handling
//! - **Clear API**: Trait-based extension methods following Rust conventions
//!
//! ## Quick Start
//!
//! ### Reading Fields
//!
//! This library provides multiple ways to read protobuf fields from different sources:
//!
//! ```rust
//! use protobuf_core::{Field, IteratorExtProtobuf, AsRefExtProtobuf};
//!
//! // From a byte iterator - returns Field<Vec<u8>> (Len values are Vec<u8>)
//! let bytes = vec![0x08, 0x96, 0x01]; // field 1: 150
//! let fields: Vec<Field<Vec<u8>>> = bytes
//!     .into_iter()
//!     .protobuf_fields()
//!     .collect::<Result<Vec<Field<Vec<u8>>>, _>>()
//!     .unwrap();
//!
//! // From a slice - returns Field<&[u8]> (Len values are &[u8], zero-copy)
//! let slice: &[u8] = &[0x08, 0x96, 0x01];
//! let fields: Vec<Field<&[u8]>> = AsRefExtProtobuf::read_protobuf_fields(&slice)
//!     .collect::<Result<Vec<Field<&[u8]>>, _>>()
//!     .unwrap();
//! ```
//!
//! ### Writing Fields
//!
//! ```rust
//! use protobuf_core::{WriteExtProtobuf, Field, FieldValue, FieldNumber};
//!
//! let mut buffer = Vec::new();
//! let field: Field<Vec<u8>> = Field::new(
//!     FieldNumber::try_from(1)?,
//!     FieldValue::from_uint64(150)
//! );
//! buffer.write_protobuf_field(&field)?;
//! # Ok::<(), protobuf_core::ProtobufError>(())
//! ```
//!
//! ## Core Types
//!
//! ### Fields
//!
//! - **[`Field<L>`]: Represents a raw protobuf field with a field number and value.
//!   The `L` type parameter represents the type used for length-delimited values (e.g., `Vec<u8>`
//!   for owned data, `&'a [u8]` for borrowed data).
//! - **[`FieldValue<L>`]: Represents the raw value of a field. It can be:
//!   - `Varint(Varint)`: Variable-width integers (Int32, Int64, UInt32, UInt64, SInt32, SInt64,
//!     Bool, Enum)
//!   - `I32([u8; 4])`: 32-bit fixed-width values (Fixed32, SFixed32, Float)
//!   - `I64([u8; 8])`: 64-bit fixed-width values (Fixed64, SFixed64, Double)
//!   - `Len(L)`: Length-delimited values (String, Bytes, embedded messages, packed repeated
//!     fields)
//!
//! ### Basic Types
//!
//! - **[`Tag`]: Represents a protobuf tag (field number + wire type)
//! - **[`Varint`]: Represents a deserialized varint value (8-byte internal
//!   representation)
//! - **[`FieldNumber`]: A validated field number (range: 1 to 2^29 - 1)
//! - **[`WireType`]: Represents the protobuf wire type (Varint, Int32, Int64,
//!   Len, StartGroup, EndGroup)
//!
//! ### Error Handling
//!
//! - **[`ProtobufError`]: Unified error type for all protobuf operations
//! - **[`Result<T>`]: Type alias for `Result<T, ProtobufError>`
//!
//! ## Reading Traits
//!
//! The library provides several traits for reading protobuf fields from different sources:
//!
//! - **[`IteratorExtProtobuf`]: Read fields from `Iterator<Item = u8>`
//!   - Output: [`Field<Vec<u8>>`] - Len values are owned `Vec<u8>`
//! - **[`TryIteratorExtProtobuf`]: Read fields from
//!   `Iterator<Item = Result<u8, E>>`
//!   - Output: [`Field<Vec<u8>>`] - Len values are owned `Vec<u8>`
//! - **[`AsRefExtProtobuf`]: Read fields from `AsRef<[u8]>` types
//!   (slices, arrays, etc.)
//!   - Output: [`Field<&[u8]>`] - Len values are borrowed `&[u8]` slices (zero-copy)
//! - **[`ReadExtProtobuf`]: Read fields from `std::io::Read` types
//!   - Output: [`Field<Vec<u8>>`] - Len values are owned `Vec<u8>`
//!
//! ## Writing Traits
//!
//! - **[`WriteExtProtobuf`]: Write fields to `std::io::Write`
//!
//! ## Tag/Varint Traits
//!
//! For lower-level operations, the library provides traits for reading and writing tags and
//! varints:
//!
//! - **[`IteratorExtTag`]** / **[`TryIteratorExtTag`]**
//!   / **[`ReadExtTag`]**: Read tags
//! - **[`IteratorExtVarint`]** / **[`TryIteratorExtVarint`]**
//!   / **[`ReadExtVarint`]**: Read varints
//! - **[`WriteExtVarint`]**: Write varints
//!
//! ## Feature Flags
//!
//! - `read` (enabled by default): Enables field reading utilities
//! - `write` (enabled by default): Enables field writing utilities
//!
//! You can use features independently:
//!
//! ```toml
//! [dependencies]
//! protobuf-core = { version = "0.1.0", default-features = false, features = ["read"] }
//! ```
//!
//! ## Constants
//!
//! The library also provides various constants related to the protobuf wire format:
//!
//! - Field number limits: [`MIN_FIELD_NUMBER`], [`MAX_FIELD_NUMBER`]
//! - Size limits: [`MAX_MESSAGE_SIZE`], [`MAX_STRING_SIZE`], [`MAX_VARINT_BYTES`]
//! - Wire format constants: [`FIXED32_BYTES`], [`FIXED64_BYTES`], etc.

#[cfg(any(feature = "read", feature = "write"))]
pub(crate) mod field;
pub(crate) mod field_number;
pub(crate) mod tag;
pub(crate) mod varint;
pub(crate) mod wire_format;

#[cfg(feature = "write")]
pub use self::field::WriteExtProtobuf;
#[cfg(feature = "read")]
pub use self::field::{
    AsRefExtProtobuf, IteratorExtProtobuf, ProtobufFieldIterator, ProtobufFieldIteratorFromBytes,
    ProtobufFieldSliceIterator, ReadExtProtobuf, TryIteratorExtProtobuf,
};
#[cfg(any(feature = "read", feature = "write"))]
pub use self::field::{Field, FieldValue};
pub use self::field_number::FieldNumber;
pub use self::tag::{IteratorExtTag, ReadExtTag, Tag, TryIteratorExtTag};
pub use self::varint::{
    IteratorExtVarint, ReadExtVarint, TryIteratorExtVarint, Varint, WriteExtVarint,
};
pub use self::wire_format::{
    FIELD_NUMBER_SHIFT, FIXED32_BYTES, FIXED64_BYTES, MAX_1_BYTE_VARINT, MAX_2_BYTE_VARINT,
    MAX_3_BYTE_VARINT, MAX_4_BYTE_VARINT, MAX_5_BYTE_VARINT, MAX_6_BYTE_VARINT, MAX_7_BYTE_VARINT,
    MAX_8_BYTE_VARINT, MAX_9_BYTE_VARINT, MAX_FIELD_NUMBER, MAX_MESSAGE_SIZE, MAX_STRING_SIZE,
    MAX_VARINT_BYTES, MIN_FIELD_NUMBER, VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK,
    WIRE_TYPE_MASK, WireType,
};

use ::std::convert::Infallible;
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

    #[error("Varint exceeds maximum length of 10 bytes")]
    VarintTooLong,

    #[error("Failed to downcast field value to expected type: {expected_type}")]
    FieldTypeDowncastError { expected_type: String },

    #[error("Malformed tag: field_number={field_number}, wire_type={wire_type}")]
    MalformedTag { field_number: u32, wire_type: u8 },

    #[error("Unexpected EOF while parsing field")]
    UnexpectedEof,

    #[error("I/O error: {0}")]
    IoError(#[from] ::std::io::Error),
}

impl From<Infallible> for ProtobufError {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

/// Custom Result type for protobuf operations
pub type Result<T> = ::std::result::Result<T, ProtobufError>;
