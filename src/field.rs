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

//! Field-level I/O utilities for Protocol Buffers
//!
//! This module provides primitive utilities for reading and writing raw protobuf fields.
//! These are building blocks for constructing higher-level parsers and serializers,
//! not a complete message parser or serializer.
//!
//! ## Core Types
//!
//! The `Field` and `FieldValue` types are always available when this module is enabled.
//!
//! ## Reading (Deserialization)
//!
//! Available when the `read` feature is enabled.
//!
//! The utilities read fields sequentially from input sources that implement `std::io::Read`,
//! returning raw field values (varint bytes, fixed-width bytes, or length-delimited bytes)
//! without interpretation of the semantic meaning.
//!
//! ## Writing (Serialization)
//!
//! Available when the `write` feature is enabled.
//!
//! The utilities write fields to output targets that implement `std::io::Write`,
//! encoding field numbers, wire types, and values into the protobuf wire format.

use crate::field_number::FieldNumber;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::WireType;

#[cfg(feature = "read")]
pub mod read;

#[cfg(feature = "write")]
pub mod write;

// Re-export read functionality
#[cfg(feature = "read")]
pub use self::read::{ProtobufFieldIterator, ReadExtProtobuf};

// Re-export write functionality
#[cfg(feature = "write")]
pub use self::write::WriteExtProtobuf;

/// A raw field value read from the wire
///
/// This represents the raw bytes of a field value without semantic interpretation.
/// The caller is responsible for converting these raw values to the appropriate types
/// based on the field's schema definition.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Variable-width integers (Int32, Int64, UInt32, UInt64, SInt32, SInt64, Bool, Enum)
    Varint(Varint),
    /// 32-bit fixed-width values (Fixed32, SFixed32, Float)
    I32([u8; 4]),
    /// 64-bit fixed-width values (Fixed64, SFixed64, Double)
    I64([u8; 8]),
    /// Length-delimited values (String, Bytes, embedded messages, packed repeated fields)
    Len(Vec<u8>),
}

impl FieldValue {
    // Varint constructors

    /// Create a field value from a Varint
    pub fn from_varint(varint: Varint) -> Self {
        Self::Varint(varint)
    }

    /// Create a field value from a UInt64 protobuf type
    pub fn from_uint64(value: u64) -> Self {
        Self::Varint(Varint::from_uint64(value))
    }

    /// Create a field value from a UInt32 protobuf type
    pub fn from_uint32(value: u32) -> Self {
        Self::Varint(Varint::from_uint32(value))
    }

    /// Create a field value from a SInt64 protobuf type (ZigZag encoded)
    pub fn from_sint64(value: i64) -> Self {
        Self::Varint(Varint::from_sint64(value))
    }

    /// Create a field value from a SInt32 protobuf type (ZigZag encoded)
    pub fn from_sint32(value: i32) -> Self {
        Self::Varint(Varint::from_sint32(value))
    }

    /// Create a field value from an Int64 protobuf type (non-ZigZag)
    pub fn from_int64(value: i64) -> Self {
        Self::Varint(Varint::from_int64(value))
    }

    /// Create a field value from an Int32 protobuf type (non-ZigZag)
    pub fn from_int32(value: i32) -> Self {
        Self::Varint(Varint::from_int32(value))
    }

    /// Create a field value from a Bool protobuf type
    pub fn from_bool(value: bool) -> Self {
        Self::Varint(Varint::from_bool(value))
    }

    // Fixed-width constructors

    /// Create a field value from a Fixed32 protobuf type
    pub fn from_fixed32(value: u32) -> Self {
        Self::I32(value.to_le_bytes())
    }

    /// Create a field value from a SFixed32 protobuf type
    pub fn from_sfixed32(value: i32) -> Self {
        Self::I32(value.to_le_bytes())
    }

    /// Create a field value from a Float protobuf type
    pub fn from_float(value: f32) -> Self {
        Self::I32(value.to_le_bytes())
    }

    /// Create a field value from a Fixed64 protobuf type
    pub fn from_fixed64(value: u64) -> Self {
        Self::I64(value.to_le_bytes())
    }

    /// Create a field value from a SFixed64 protobuf type
    pub fn from_sfixed64(value: i64) -> Self {
        Self::I64(value.to_le_bytes())
    }

    /// Create a field value from a Double protobuf type
    pub fn from_double(value: f64) -> Self {
        Self::I64(value.to_le_bytes())
    }

    // Length-delimited constructors

    /// Create a field value from raw bytes (Bytes protobuf type)
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self::Len(data)
    }

    /// Create a field value from a String protobuf type
    pub fn from_string(s: String) -> Self {
        Self::Len(s.into_bytes())
    }

    /// Calculate the encoded size of this field value in bytes (excluding the tag)
    ///
    /// For Len values, this includes the length varint plus the data bytes.
    pub fn encoded_size(&self) -> usize {
        match self {
            Self::Varint(varint) => varint.varint_size(),
            Self::I32(_) => 4,
            Self::I64(_) => 8,
            Self::Len(data) => {
                let length = data.len() as u64;
                let length_varint = Varint::from_uint64(length);
                length_varint.varint_size() + data.len()
            }
        }
    }
}

/// A raw field read from the wire
///
/// Contains the field number and the raw field value.
/// The caller must interpret the value based on the message schema.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub field_number: FieldNumber,
    pub value: FieldValue,
}

impl Field {
    /// Create a new field with the given field number and value
    pub fn new(field_number: FieldNumber, value: FieldValue) -> Self {
        Self {
            field_number,
            value,
        }
    }

    /// Calculate the total encoded size of this field in bytes (tag + value)
    pub fn encoded_size(&self) -> usize {
        // Tag size (field number shifted left by 3 bits to make room for wire type)
        let wire_type = match &self.value {
            FieldValue::Varint(_) => WireType::Varint,
            FieldValue::I32(_) => WireType::Int32,
            FieldValue::I64(_) => WireType::Int64,
            FieldValue::Len(_) => WireType::Len,
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
