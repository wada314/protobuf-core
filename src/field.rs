//! Field-level I/O utilities for Protocol Buffers
//!
//! This module provides primitive utilities for reading and writing raw protobuf fields.
//! These are building blocks for constructing higher-level parsers and serializers,
//! not a complete message parser or serializer.
//!
//! ## Core Types
//!
//! `Field` and `FieldValue` are the raw field types shared by reading and writing.
//!
//! ## Reading (Deserialization)
//!
//! The utilities read fields sequentially from `std::io::Read` (with an explicit
//! byte budget `len`) and from `AsRef<[u8]>` (the slice length is the bound),
//! returning raw field values (varint bytes, fixed-width bytes, or
//! length-delimited bytes) without interpretation of the semantic meaning.
//! Byte iterators are not supported; use a slice or a `Read` with `len`.
//!
//! ## Writing (Serialization)
//!
//! The utilities write fields to output targets that implement `std::io::Write`,
//! encoding field numbers, wire types, and values into the protobuf wire format.

use crate::field_number::FieldNumber;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::WireType;

pub(crate) mod read;
pub(crate) mod write;

pub use self::read::{
    AsRefExtProtobuf, FieldIterFromRead, FieldIterFromSlice, GroupBody, Limited, OnLen,
    ReadExtProtobuf, VecLenPayload, until_group_ends,
};
pub use self::write::WriteExtProtobuf;

/// A raw field value read from the wire
///
/// This represents the raw bytes of a field value without semantic interpretation.
/// The caller is responsible for converting these raw values to the appropriate types
/// based on the field's schema definition.
///
/// `L` is the type for length-delimited values (Len variant).
///  - Streaming readers (`Read` with a `len` budget) default to `Vec<u8>`
///    ([`VecLenPayload`]). `read_protobuf_fields_with` takes an [`OnLen`]
///    handler that receives a [`Limited`] body and returns any `L`.
///  - Slice readers borrow the input as `&'a [u8]`.
///
/// [`SGroup`](Self::SGroup) / [`EGroup`](Self::EGroup) are tag-only: the start tag
/// does not include the group body. After an `SGroup`, discard the body with
/// [`skip_until`](crate::FieldIterFromSlice::skip_until) +
/// [`until_group_ends`](crate::until_group_ends), preserve it with
/// [`take_until`](crate::FieldIterFromSlice::take_until) (slice) or
/// [`copy_until`](crate::FieldIterFromRead::copy_until) (Read), parse the body
/// with [`group_body`](crate::FieldIterFromSlice::group_body) /
/// [`GroupBody`], or read nested fields until a matching `EGroup` before
/// the next top-level field.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue<L> {
    /// Variable-width integers (`Int32`, `Int64`, `UInt32`, `UInt64`, `SInt32`, `SInt64`, `Bool`, `Enum`)
    Varint(Varint),
    /// 32-bit fixed-width values (`Fixed32`, `SFixed32`, `Float`)
    I32([u8; 4]),
    /// 64-bit fixed-width values (`Fixed64`, `SFixed64`, `Double`)
    I64([u8; 8]),
    /// Length-delimited values (`String`, `Bytes`, embedded messages, packed repeated fields)
    Len(L),
    /// Start-group tag (deprecated). No payload bytes follow the tag itself.
    SGroup,
    /// End-group tag (deprecated). No payload bytes follow the tag itself.
    EGroup,
}

impl<L> FieldValue<L> {
    // Varint constructors

    /// Create a field value from a Varint
    pub fn from_varint(varint: Varint) -> Self {
        Self::Varint(varint)
    }

    /// Create a field value from a `UInt64` protobuf type
    pub fn from_uint64(value: u64) -> Self {
        Self::Varint(Varint::from_uint64(value))
    }

    /// Create a field value from a `UInt32` protobuf type
    pub fn from_uint32(value: u32) -> Self {
        Self::Varint(Varint::from_uint32(value))
    }

    /// Create a field value from a `SInt64` protobuf type (ZigZag encoded)
    pub fn from_sint64(value: i64) -> Self {
        Self::Varint(Varint::from_sint64(value))
    }

    /// Create a field value from a `SInt32` protobuf type (ZigZag encoded)
    pub fn from_sint32(value: i32) -> Self {
        Self::Varint(Varint::from_sint32(value))
    }

    /// Create a field value from an `Int64` protobuf type (non-ZigZag)
    pub fn from_int64(value: i64) -> Self {
        Self::Varint(Varint::from_int64(value))
    }

    /// Create a field value from an `Int32` protobuf type (non-ZigZag)
    pub fn from_int32(value: i32) -> Self {
        Self::Varint(Varint::from_int32(value))
    }

    /// Create a field value from a `Bool` protobuf type
    pub fn from_bool(value: bool) -> Self {
        Self::Varint(Varint::from_bool(value))
    }

    // Fixed-width constructors

    /// Create a field value from a `Fixed32` protobuf type
    pub fn from_fixed32(value: u32) -> Self {
        Self::I32(value.to_le_bytes())
    }

    /// Create a field value from a `SFixed32` protobuf type
    pub fn from_sfixed32(value: i32) -> Self {
        Self::I32(value.to_le_bytes())
    }

    /// Create a field value from a `Float` protobuf type
    pub fn from_float(value: f32) -> Self {
        Self::I32(value.to_le_bytes())
    }

    /// Create a field value from a `Fixed64` protobuf type
    pub fn from_fixed64(value: u64) -> Self {
        Self::I64(value.to_le_bytes())
    }

    /// Create a field value from a `SFixed64` protobuf type
    pub fn from_sfixed64(value: i64) -> Self {
        Self::I64(value.to_le_bytes())
    }

    /// Create a field value from a `Double` protobuf type
    pub fn from_double(value: f64) -> Self {
        Self::I64(value.to_le_bytes())
    }

    /// Calculate the encoded size of this field value in bytes (excluding the tag)
    ///
    /// For Len values, this includes the length varint plus the data bytes.
    pub fn encoded_size(&self) -> usize
    where
        L: AsRef<[u8]>,
    {
        match self {
            Self::Varint(varint) => varint.varint_size(),
            Self::I32(_) => 4,
            Self::I64(_) => 8,
            Self::Len(data) => {
                let data_slice = data.as_ref();
                let length = data_slice.len() as u64;
                let length_varint = Varint::from_uint64(length);
                length_varint.varint_size() + data_slice.len()
            }
            // Group tags carry no payload; the tag size is counted in `Field::encoded_size`.
            Self::SGroup | Self::EGroup => 0,
        }
    }
}

/// A raw field read from the wire
///
/// Contains the field number and the raw field value.
/// The caller must interpret the value based on the message schema.
///
/// `L` is the type for length-delimited values (FieldValue::Len).
/// Streaming readers (`Read` with a `len` budget) default to `Vec<u8>`
/// ([`crate::VecLenPayload`]). `read_protobuf_fields_with` takes an
/// [`crate::OnLen`] handler over a [`crate::Limited`] body.
/// Slice readers borrow the input as `&'a [u8]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Field<L> {
    pub field_number: FieldNumber,
    pub value: FieldValue<L>,
}

impl<L> Field<L> {
    /// Create a new field with the given field number and value
    pub fn new(field_number: FieldNumber, value: FieldValue<L>) -> Self {
        Self {
            field_number,
            value,
        }
    }

    /// Calculate the total encoded size of this field in bytes (tag + value)
    pub fn encoded_size(&self) -> usize
    where
        L: AsRef<[u8]>,
    {
        // Tag size (field number shifted left by 3 bits to make room for wire type)
        let wire_type = match &self.value {
            FieldValue::Varint(_) => WireType::Varint,
            FieldValue::I32(_) => WireType::Int32,
            FieldValue::I64(_) => WireType::Int64,
            FieldValue::Len(_) => WireType::Len,
            FieldValue::SGroup => WireType::SGroup,
            FieldValue::EGroup => WireType::EGroup,
        };
        let tag = Tag {
            field_number: self.field_number,
            wire_type,
        };
        let tag_varint = tag.to_encoded();
        let tag_size = tag_varint.varint_size();

        // Value size
        let value_size = self.value.encoded_size();

        tag_size + value_size
    }
}
