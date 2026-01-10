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

//! Common field parsing logic shared between owned and borrowed field readers

use crate::field::FieldValue;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};

/// Trait for reading field values from different data sources
///
/// This trait abstracts the reading operations needed to parse field values.
/// Different implementations handle different data source types (Iterator, Read, slice, etc.)
pub trait FieldValueReader<L> {
    /// Read a varint value (may be called multiple times for Len wire type)
    fn read_varint(&mut self) -> Result<Option<Varint>>;

    /// Read exactly 4 bytes
    fn read_i32(&mut self) -> Result<[u8; 4]>;

    /// Read exactly 8 bytes
    fn read_i64(&mut self) -> Result<[u8; 8]>;

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
pub fn parse_field_value<L, R>(tag: Tag, reader: &mut R) -> Result<FieldValue<L>>
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
            let bytes = reader.read_i32()?;
            FieldValue::I32(bytes)
        }
        WireType::Int64 => {
            let bytes = reader.read_i64()?;
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
