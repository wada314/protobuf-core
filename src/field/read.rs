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

//! Field reading utilities for Protocol Buffers
//!
//! This module provides low-level utilities for reading raw protobuf fields.
//! It is organized by the destination type (owned vs borrowed):
//!
//! - `owned`: Returns `Field<Vec<u8>>` - owned data suitable for streaming sources
//! - `ref`: Returns `Field<&'a [u8]>` - borrowed references suitable for slice sources

use crate::field::{Field, FieldValue};
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};

/// Trait for reading field values from different data sources
///
/// This trait abstracts the reading operations needed to parse field values.
/// Different implementations handle different data source types (Iterator, Read, slice, etc.)
pub(crate) trait FieldValueReader<L> {
    /// Read a tag (field number + wire type)
    ///
    /// Default implementation uses `read_varint` and converts the varint to a tag.
    fn read_tag(&mut self) -> Result<Option<Tag>> {
        let Some(varint) = self.read_varint()? else {
            return Ok(None);
        };
        Tag::from_encoded(varint).map(Some)
    }

    /// Read a varint value (may be called multiple times for Len wire type)
    fn read_varint(&mut self) -> Result<Option<Varint>>;

    /// Read exactly N bytes
    fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N]>;

    /// Read a length-prefixed byte sequence
    ///
    /// The length parameter is the number of bytes to read after the length varint has been parsed.
    fn read_len(&mut self, length: usize) -> Result<L>;
}

/// Parse a field value given a tag and a field value reader
///
/// This is the common parsing logic shared between all field parsers.
/// The `L` type parameter represents the type used for length-delimited values
/// (e.g., `Vec<u8>` for owned data, `&'a [u8]` for borrowed data).
pub(crate) fn parse_field_value<L, R>(tag: Tag, reader: &mut R) -> Result<FieldValue<L>>
where
    R: FieldValueReader<L>,
{
    let value = match tag.wire_type {
        WireType::Varint => {
            let Some(varint) = reader.read_varint()? else {
                return Err(ProtobufError::UnexpectedEof);
            };
            FieldValue::Varint(varint)
        }
        WireType::Int32 => {
            let bytes = reader.read_bytes::<4>()?;
            FieldValue::I32(bytes)
        }
        WireType::Int64 => {
            let bytes = reader.read_bytes::<8>()?;
            FieldValue::I64(bytes)
        }
        WireType::Len => {
            // Read length prefix (varint)
            let Some(length_varint) = reader.read_varint()? else {
                return Err(ProtobufError::UnexpectedEof);
            };
            let length = length_varint.try_to_uint32()? as usize;
            let data = reader.read_len(length)?;
            FieldValue::Len(data)
        }
        _ => {
            return Err(ProtobufError::InvalidWireType {
                value: tag.wire_type as u8,
            });
        }
    };

    Ok(value)
}

/// Parse a complete field (tag + value) from a field value reader
///
/// This is the common parsing logic for reading a complete field.
/// The `L` type parameter represents the type used for length-delimited values
/// (e.g., `Vec<u8>` for owned data, `&'a [u8]` for borrowed data).
pub(crate) fn parse_field<L, R>(reader: &mut R) -> Result<Option<Field<L>>>
where
    R: FieldValueReader<L>,
{
    // Read tag
    let Some(tag) = reader.read_tag()? else {
        return Ok(None);
    };

    // Parse value based on wire type
    let value = parse_field_value(tag, reader)?;

    Ok(Some(Field {
        field_number: tag.field_number,
        value,
    }))
}

#[path = "read/owned.rs"]
pub mod owned;

#[path = "read/ref_.rs"]
pub mod ref_;

// Re-export for convenience
pub use owned::{
    IteratorExtProtobuf, ProtobufFieldIterator, ProtobufFieldIteratorFromBytes, ReadExtProtobuf,
    TryIteratorExtProtobuf,
};
pub use ref_::{AsRefExtProtobuf, ProtobufFieldSliceIterator};
