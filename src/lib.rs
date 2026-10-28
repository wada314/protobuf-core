//! A primitive utility library for Protocol Buffers in Rust.
//!
//! # ⚠️ NOT A GOOGLE OFFICIAL PRODUCT
//!
//! **This library is not an official Google product.** Google provides its own Protocol Buffers
//! implementation for Rust (see [protocolbuffers/protobuf](https://github.com/protocolbuffers/protobuf)
//! and official Rust support). This project is an **independent** library.
//!
//! ---
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
//! Read fields from any `AsRef<[u8]>` value, such as a slice or `Vec<u8>`.
//! Length-delimited bytes stay borrowed (`Field<&[u8]>`).
//!
//! ```rust
//! use protobuf_core::{AsRefExtProtobuf, Field};
//!
//! let bytes: &[u8] = &[0x08, 0x96, 0x01]; // field 1: 150
//! let fields: Vec<Field<&[u8]>> = bytes
//!     .read_protobuf_fields()
//!     .collect::<Result<Vec<_>, _>>()
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
//!     FieldNumber::try_new(1).unwrap(),
//!     FieldValue::from_uint64(150)
//! );
//! buffer.write_protobuf_field(&field).unwrap();
//! ```
//!
//! ## Core Types
//!
//! ### Fields
//!
//! - [`Field<L>`]: Represents a raw protobuf field with a field number and value.
//!   The `L` type parameter represents the type used for length-delimited values
//!   (`Vec<u8>` by default from `Read`, a custom type from [`OnLen`], or
//!   `&'a [u8]` for a slice read).
//! - [`FieldValue<L>`]: Represents the raw value of a field. It can be:
//!   - `Varint(Varint)`: Variable-width integers (Int32, Int64, UInt32, UInt64, SInt32, SInt64,
//!     Bool, Enum)
//!   - `I32([u8; 4])`: 32-bit fixed-width values (Fixed32, SFixed32, Float)
//!   - `I64([u8; 8])`: 64-bit fixed-width values (Fixed64, SFixed64, Double)
//!   - `Len(L)`: Length-delimited values (String, Bytes, embedded messages, packed repeated
//!     fields)
//!   - `SGroup` / `EGroup`: Deprecated group start/end tags (no payload; body follows `SGroup`)
//!
//! ### Basic Types
//!
//! - [`Tag`]: Represents a protobuf tag (field number + wire type)
//! - [`Varint`]: Represents a deserialized varint value (8-byte internal
//!   representation)
//! - [`FieldNumber`]: A validated field number (range: 1 to 2^29 - 1)
//! - [`WireType`]: Represents the protobuf wire type (`Varint`, `Int64`, `Len`,
//!   `SGroup`, `EGroup`, `Int32`)
//!
//! ### Error Handling
//!
//! - [`ProtobufError`]: Unified error type for all protobuf operations
//! - [`Result<T>`]: Type alias for `Result<T, ProtobufError>`
//!
//! ## Reading Traits
//!
//! The library provides traits for reading protobuf fields from two sources:
//!
//! - [`AsRefExtProtobuf`]: Read fields from `AsRef<[u8]>` types
//!   (slices, arrays, etc.). The slice length is the bound (no extra `len` arg).
//!   - Output: [`Field<&[u8]>`] — Len values are borrowed `&[u8]` slices (zero-copy)
//! - [`ReadExtProtobuf`]: Read fields from `std::io::Read` types with an explicit
//!   byte budget `len` (tracked by [`Limited`], not [`std::io::Take`])
//!   - Output: [`Field<Vec<u8>>`] by default, or [`Field<L>`] from a custom [`OnLen`]
//! - [`Limited`]: Length budget over a `Read` with [`Limited::reborrow_len`] for
//!   nested Len bodies without nested `Take` adapters
//!
//! Field iterators expose `skip_until`, slice `take_until`, Read `copy_until`,
//! and [`group_body`](FieldIterFromSlice::group_body) /
//! [`GroupBody`] for parsing a group body. After an [`FieldValue::SGroup`],
//! pair skip/preserve with [`until_group_ends`] (start tag already consumed).
//!
//! Byte iterators are not supported; use a slice or a `Read` with `len`.
//!
//! ## Writing Traits
//!
//! - [`WriteExtProtobuf`]: Write fields to `std::io::Write`
//!
//! ## Tag/Varint Traits
//!
//! For lower-level operations, the library provides traits for reading and writing tags and
//! varints. These `Read` methods do not take a byte budget; they stop at the reader's EOF
//! (or the caller's own bound such as [`Limited`] / [`std::io::Take`]):
//!
//! - [`ReadExtTag`]: Read tags from `std::io::Read`
//! - [`ReadExtVarint`]: Read varints from `std::io::Read` (`with_raw` / `read_varint`
//!   return `Result` — empty is `UnexpectedEof`; chunked callers buffer ≤10 bytes)
//! - [`WriteExtVarint`]: Write varints
//!
//! ## Constants
//!
//! The library also provides various constants related to the protobuf wire format:
//!
//! - Field number limits: [`MIN_FIELD_NUMBER`], [`MAX_FIELD_NUMBER`]
//! - Size limits: [`MAX_MESSAGE_SIZE`], [`MAX_STRING_SIZE`], [`MAX_VARINT_BYTES`]
//! - Wire format constants: [`FIXED32_BYTES`], [`FIXED64_BYTES`], etc.

pub(crate) mod field;
pub(crate) mod field_number;
pub(crate) mod tag;
pub(crate) mod varint;
pub(crate) mod wire_format;

pub use self::field::{
    AsRefExtProtobuf, Field, FieldIterFromRead, FieldIterFromSlice, FieldValue, GroupBody, Limited,
    OnLen, ReadExtProtobuf, VecLenPayload, WriteExtProtobuf, until_group_ends,
};
pub use self::field_number::FieldNumber;
pub use self::tag::{ReadExtTag, Tag};
pub use self::varint::{ReadExtVarint, Varint, VarintIterFromRead, WriteExtVarint};
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
    FieldNumberOutOfRange { value: i64 },

    /// The tag's low 3 bits were not a known wire type (must be 0–5).
    ///
    /// Deprecated group types [`WireType::SGroup`] (3) and [`WireType::EGroup`]
    /// (4) are known wire types; field readers return
    /// [`FieldValue::SGroup`] / [`FieldValue::EGroup`] for those.
    #[error("Invalid wire type: {value} (must be 0-5)")]
    InvalidWireType { value: u8 },

    /// An end-group tag's field number did not match the open start-group.
    #[error(
        "End-group field number {} does not match open start-group {}",
        got.as_u32(),
        expected.as_u32()
    )]
    MismatchedEndGroup {
        expected: FieldNumber,
        got: FieldNumber,
    },

    #[error("Varint value {value} is out of range for target type: {target_type}")]
    VarintDowncastOutOfRange {
        value: u64,
        target_type: &'static str,
    },

    #[error("Varint exceeds the 64-bit range or the maximum length of 10 bytes")]
    VarintTooLong,

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
