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

//! Protocol Buffers parser
//!
//! This module provides a simple parser for Protocol Buffers messages.
//! It reads and parses protobuf fields from input sources that implement `std::io::Read`.
//! The parser is provided as an extension trait `ReadExtProtobuf` for `std::io::Read` types.
//!
//! This module is only available when the `parser` feature is enabled.

use crate::field_number::FieldNumber;
use crate::tag::ReadExtTag;
use crate::varint::Varint;
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};
use ::std::io::Read;

/// A parsed field value
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

/// A parsed field
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub field_number: FieldNumber,
    pub value: FieldValue,
}

/// Iterator over protobuf fields from a reader
pub struct ProtobufFieldIterator<R> {
    reader: R,
}

impl<R> Iterator for ProtobufFieldIterator<R>
where
    R: Read,
{
    type Item = Result<Field>;

    fn next(&mut self) -> Option<Self::Item> {
        match parse_next_field(&mut self.reader) {
            Ok(Some(field)) => Some(Ok(field)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Extension trait for parsing Protocol Buffers from `Read` types
pub trait ReadExtProtobuf {
    /// Read and parse a single protobuf field from the reader
    ///
    /// Returns `Ok(Some(field))` if a field was successfully read,
    /// `Ok(None)` if the reader has reached end-of-input, or an error if parsing failed.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::parser::{ReadExtProtobuf, Field, FieldValue};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut reader = &[0x08, 0x96, 0x01][..]; // field 1: 150
    ///
    ///     if let Some(field) = reader.read_protobuf_field()? {
    ///         match field.value {
    ///             FieldValue::Varint(varint) => {
    ///                 println!("Field {}: {}", field.field_number.as_u32(), varint.to_uint64());
    ///             },
    ///             _ => {}
    ///         }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    fn read_protobuf_field(&mut self) -> Result<Option<Field>>;

    /// Read and parse all protobuf fields from the reader, returning an iterator
    ///
    /// # Example
    /// ```
    /// use protobuf_core::parser::{ReadExtProtobuf, Field, FieldValue};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let reader = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
    ///
    ///     for field in reader.read_protobuf_fields() {
    ///         let field = field?;
    ///         match field.value {
    ///             FieldValue::Varint(varint) => {
    ///                 println!("Field {}: {}", field.field_number.as_u32(), varint.to_uint64());
    ///             },
    ///             FieldValue::Len(data) => {
    ///                 println!("Field {}: {:?}", field.field_number.as_u32(), data);
    ///             },
    ///             _ => {}
    ///         }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    fn read_protobuf_fields(self) -> ProtobufFieldIterator<Self>
    where
        Self: Sized;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_field(&mut self) -> Result<Option<Field>> {
        parse_next_field(self)
    }

    fn read_protobuf_fields(self) -> ProtobufFieldIterator<Self>
    where
        Self: Sized,
    {
        ProtobufFieldIterator { reader: self }
    }
}

/// Parse a single field from the reader (private)
fn parse_next_field<R>(reader: &mut R) -> Result<Option<Field>>
where
    R: Read,
{
    // Read tag
    let Some(tag) = reader.read_tag()? else {
        return Ok(None);
    };

    let value = match tag.wire_type {
        WireType::Varint => {
            use crate::varint::ReadExtVarint;
            let Some(varint) = reader.read_varint()? else {
                return Err(ProtobufError::UnexpectedEof);
            };
            FieldValue::Varint(varint)
        }
        WireType::I32 => {
            let mut bytes = [0u8; 4];
            reader.read_exact(&mut bytes)?;
            FieldValue::I32(bytes)
        }
        WireType::I64 => {
            let mut bytes = [0u8; 8];
            reader.read_exact(&mut bytes)?;
            FieldValue::I64(bytes)
        }
        WireType::Len => {
            use crate::varint::ReadExtVarint;
            let Some(varint) = reader.read_varint()? else {
                return Err(ProtobufError::UnexpectedEof);
            };
            let length = varint.try_to_uint32()? as usize;
            let mut data = vec![0u8; length];
            reader.read_exact(&mut data)?;
            FieldValue::Len(data)
        }
        _ => {
            return Err(ProtobufError::InvalidWireType {
                value: tag.wire_type as u8,
            });
        }
    };

    Ok(Some(Field {
        field_number: tag.field_number,
        value,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_single_varint_field() {
        let mut reader = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let field = reader.read_protobuf_field().unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 1);
        match field.value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }

        // Should return None for end of input
        assert!(reader.read_protobuf_field().unwrap().is_none());
    }

    #[test]
    fn test_read_all_varint_fields() {
        let reader = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 1);
        match &field.value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }
    }

    #[test]
    fn test_parse_len_field() {
        let reader = &[0x12, 0x03, 0x48, 0x65, 0x6c][..]; // field 2: "Hel"

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 2);
        match &field.value {
            FieldValue::Len(data) => {
                assert_eq!(data, b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_parse_i32_field() {
        let reader = &[0x15, 0x00, 0x00, 0x00, 0x00][..]; // field 2: 0 (i32)

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 2);
        match &field.value {
            FieldValue::I32(bytes) => {
                assert_eq!(*bytes, [0x00, 0x00, 0x00, 0x00]);
            }
            _ => panic!("Expected I32 field"),
        }
    }

    #[test]
    fn test_parse_i64_field() {
        let reader = &[0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00][..]; // field 3: 0 (i64)

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 3);
        match &field.value {
            FieldValue::I64(bytes) => {
                assert_eq!(*bytes, [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
            }
            _ => panic!("Expected I64 field"),
        }
    }

    #[test]
    fn test_parse_multiple_fields() {
        let reader = &[
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ][..];

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 2);

        // Check first field
        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 1);
        match &field.value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }

        // Check second field
        let field = &fields[1];
        assert_eq!(field.field_number.as_u32(), 2);
        match &field.value {
            FieldValue::Len(data) => {
                assert_eq!(data, b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_parse_empty_stream() {
        let reader = &[][..];

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 0);
    }
}
