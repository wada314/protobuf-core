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
//! This module provides low-level utilities for reading raw protobuf fields from byte streams.

use std::borrow::Cow;

use crate::field::{Field, FieldValue};
use crate::tag::ReadExtTag;
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};
use ::std::io::Read;

/// Iterator for reading raw protobuf fields sequentially from a reader
pub struct ProtobufFieldIterator<R> {
    reader: R,
}

impl<R> Iterator for ProtobufFieldIterator<R>
where
    R: Read,
{
    type Item = Result<Field<Cow<'static, [u8]>>>; // Owned data from Read

    fn next(&mut self) -> Option<Self::Item> {
        match parse_next_field(&mut self.reader) {
            Ok(Some(field)) => Some(Ok(field)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// It does not provide semantic interpretation - that is the caller's responsibility.
pub trait ReadExtProtobuf {
    /// Read a single raw protobuf field from the reader
    ///
    /// Returns `Ok(Some(field))` if a field was successfully read,
    /// `Ok(None)` if the reader has reached end-of-input, or an error if reading failed.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::field::{ReadExtProtobuf, Field, FieldValue};
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
    fn read_protobuf_field(&mut self) -> Result<Option<Field<Cow<'static, [u8]>>>>;

    /// Read raw protobuf fields from the reader, returning an iterator
    ///
    /// This consumes the reader and returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::field::{ReadExtProtobuf, Field, FieldValue};
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
    fn read_protobuf_field(&mut self) -> Result<Option<Field<Cow<'static, [u8]>>>> {
        parse_next_field(self)
    }

    fn read_protobuf_fields(self) -> ProtobufFieldIterator<Self>
    where
        Self: Sized,
    {
        ProtobufFieldIterator { reader: self }
    }
}

/// Read a single raw field from the reader (private helper function)
fn parse_next_field<R>(reader: &mut R) -> Result<Option<Field<Cow<'static, [u8]>>>>
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
        WireType::Int32 => {
            let mut bytes = [0u8; 4];
            reader.read_exact(&mut bytes)?;
            FieldValue::I32(bytes)
        }
        WireType::Int64 => {
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
            FieldValue::Len(Cow::Owned(data)) // 'static lifetime for owned data
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
                assert_eq!(data.as_ref(), b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_parse_i32_field() {
        let reader = &[0x15, 0x78, 0x56, 0x34, 0x12][..]; // field 2: 0x12345678 (Fixed32)

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 2);
        match &field.value {
            FieldValue::I32(bytes) => {
                assert_eq!(*bytes, [0x78, 0x56, 0x34, 0x12]);
            }
            _ => panic!("Expected I32 field"),
        }
    }

    #[test]
    fn test_parse_i64_field() {
        let reader = &[0x19, 0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12][..]; // field 3: 0x1234567890ABCDEF (Fixed64)

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 3);
        match &field.value {
            FieldValue::I64(bytes) => {
                assert_eq!(*bytes, [0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]);
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
                assert_eq!(data.as_ref(), b"Hel");
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
