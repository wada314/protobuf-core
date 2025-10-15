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

//! Low-level field I/O utilities for Protocol Buffers
//!
//! This module provides primitive utilities for reading and writing raw protobuf fields.
//! These are building blocks for constructing higher-level parsers and serializers,
//! not a complete message parser or serializer.
//!
//! This module is available when either the `parser` or `serializer` feature is enabled.
//! The `Field` and `FieldValue` types are always available when the module is enabled.
//!
//! ## Reading (Deserialization)
//! Available when the `parser` feature is enabled.
//!
//! The utilities read fields sequentially from input sources that implement `std::io::Read`,
//! returning raw field values (varint bytes, fixed-width bytes, or length-delimited bytes)
//! without interpretation of the semantic meaning.
//!
//! ## Writing (Serialization)
//! Available when the `serializer` feature is enabled.
//!
//! The utilities write fields to output targets that implement `std::io::Write`,
//! encoding field numbers, wire types, and values into the protobuf wire format.

#[cfg(feature = "parser")]
use crate::ProtobufError;
use crate::Result;
use crate::field_number::FieldNumber;
#[cfg(feature = "parser")]
use crate::tag::ReadExtTag;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::WireType;
#[cfg(feature = "parser")]
use ::std::io::Read;
#[cfg(feature = "serializer")]
use ::std::io::Write;

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

/// Iterator for reading raw protobuf fields sequentially from a reader
#[cfg(feature = "parser")]
pub struct ProtobufFieldIterator<R> {
    reader: R,
}

#[cfg(feature = "parser")]
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

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// It does not provide semantic interpretation - that is the caller's responsibility.
#[cfg(feature = "parser")]
pub trait ReadExtProtobuf {
    /// Read a single raw protobuf field from the reader
    ///
    /// Returns `Ok(Some(field))` if a field was successfully read,
    /// `Ok(None)` if the reader has reached end-of-input, or an error if reading failed.
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

    /// Read raw protobuf fields from the reader, returning an iterator
    ///
    /// This consumes the reader and returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
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

#[cfg(feature = "parser")]
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

/// Read a single raw field from the reader (private helper function)
#[cfg(feature = "parser")]
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

/// Extension trait for writing raw Protocol Buffer fields to `Write` types
///
/// This trait provides low-level utilities for writing fields to a byte stream.
/// It is the counterpart to `ReadExtProtobuf` for serialization.
///
/// This trait is only available when the `serializer` feature is enabled.
#[cfg(feature = "serializer")]
pub trait WriteExtProtobuf {
    /// Write a single raw protobuf field to the writer (tag + value)
    ///
    /// Returns the number of bytes written.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::parser::{WriteExtProtobuf, Field, FieldValue};
    /// use protobuf_core::field_number::FieldNumber;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut buffer = Vec::new();
    ///     
    ///     let field = Field::new(
    ///         FieldNumber::try_from(1)?,
    ///         FieldValue::from_uint64(150)
    ///     );
    ///     
    ///     buffer.write_protobuf_field(&field)?;
    ///     assert_eq!(buffer, vec![0x08, 0x96, 0x01]); // field 1: 150
    ///     Ok(())
    /// }
    /// ```
    fn write_protobuf_field(&mut self, field: &Field) -> Result<usize>;

    /// Write multiple raw protobuf fields to the writer
    ///
    /// Returns the total number of bytes written.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::parser::{WriteExtProtobuf, Field, FieldValue};
    /// use protobuf_core::field_number::FieldNumber;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut buffer = Vec::new();
    ///     
    ///     let fields = vec![
    ///         Field::new(FieldNumber::try_from(1)?, FieldValue::from_uint64(150)),
    ///         Field::new(FieldNumber::try_from(2)?, FieldValue::from_string("Hello".to_string())),
    ///     ];
    ///     
    ///     buffer.write_protobuf_fields(&fields)?;
    ///     Ok(())
    /// }
    /// ```
    fn write_protobuf_fields<'a, I>(&mut self, fields: I) -> Result<usize>
    where
        I: IntoIterator<Item = &'a Field>;
}

#[cfg(feature = "serializer")]
impl<W> WriteExtProtobuf for W
where
    W: Write,
{
    fn write_protobuf_field(&mut self, field: &Field) -> Result<usize> {
        use crate::varint::WriteExtVarint;

        let mut bytes_written = 0;

        // Write tag
        let wire_type = match &field.value {
            FieldValue::Varint(_) => WireType::Varint,
            FieldValue::I32(_) => WireType::Int32,
            FieldValue::I64(_) => WireType::Int64,
            FieldValue::Len(_) => WireType::Len,
        };
        let tag = Tag {
            field_number: field.field_number,
            wire_type,
        };
        let tag_varint = tag.to_encoded();
        bytes_written += self.write_varint(&tag_varint)?;

        // Write value
        match &field.value {
            FieldValue::Varint(varint) => {
                bytes_written += self.write_varint(varint)?;
            }
            FieldValue::I32(bytes) => {
                self.write_all(bytes)?;
                bytes_written += 4;
            }
            FieldValue::I64(bytes) => {
                self.write_all(bytes)?;
                bytes_written += 8;
            }
            FieldValue::Len(data) => {
                // Write length
                let length_varint = Varint::from_uint64(data.len() as u64);
                bytes_written += self.write_varint(&length_varint)?;
                // Write data
                self.write_all(data)?;
                bytes_written += data.len();
            }
        }

        Ok(bytes_written)
    }

    fn write_protobuf_fields<'a, I>(&mut self, fields: I) -> Result<usize>
    where
        I: IntoIterator<Item = &'a Field>,
    {
        let mut total_bytes = 0;
        for field in fields {
            total_bytes += self.write_protobuf_field(field)?;
        }
        Ok(total_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parser (deserialization) tests
    #[cfg(feature = "parser")]
    mod parser_tests {
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

    // Serialization tests
    #[cfg(feature = "serializer")]
    mod serializer_tests {
        use super::*;

        #[test]
        fn test_write_single_varint_field() {
            let mut buffer = Vec::new();

            let field = Field::new(
                FieldNumber::try_from(1).unwrap(),
                FieldValue::from_uint64(150),
            );

            let bytes_written = buffer.write_protobuf_field(&field).unwrap();
            assert_eq!(bytes_written, 3); // tag (1 byte) + value (2 bytes)
            assert_eq!(buffer, vec![0x08, 0x96, 0x01]); // field 1: 150
        }

        #[test]
        fn test_write_len_field() {
            let mut buffer = Vec::new();

            let field = Field::new(
                FieldNumber::try_from(2).unwrap(),
                FieldValue::from_string("Hel".to_string()),
            );

            buffer.write_protobuf_field(&field).unwrap();
            assert_eq!(buffer, vec![0x12, 0x03, 0x48, 0x65, 0x6c]); // field 2: "Hel"
        }

        #[test]
        fn test_write_i32_field() {
            let mut buffer = Vec::new();

            let field = Field::new(
                FieldNumber::try_from(2).unwrap(),
                FieldValue::from_fixed32(0x12345678),
            );

            buffer.write_protobuf_field(&field).unwrap();
            assert_eq!(buffer, vec![0x15, 0x78, 0x56, 0x34, 0x12]); // field 2: 0x12345678
        }

        #[test]
        fn test_write_i64_field() {
            let mut buffer = Vec::new();

            let field = Field::new(
                FieldNumber::try_from(3).unwrap(),
                FieldValue::from_fixed64(0x1234567890ABCDEF),
            );

            buffer.write_protobuf_field(&field).unwrap();
            assert_eq!(
                buffer,
                vec![0x19, 0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]
            ); // field 3: 0x1234567890ABCDEF
        }

        #[test]
        fn test_write_multiple_fields() {
            let mut buffer = Vec::new();

            let fields = vec![
                Field::new(
                    FieldNumber::try_from(1).unwrap(),
                    FieldValue::from_uint64(150),
                ),
                Field::new(
                    FieldNumber::try_from(2).unwrap(),
                    FieldValue::from_string("Hel".to_string()),
                ),
            ];

            buffer.write_protobuf_fields(&fields).unwrap();
            assert_eq!(
                buffer,
                vec![
                    0x08, 0x96, 0x01, // field 1: 150
                    0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
                ]
            );
        }

        #[test]
        fn test_field_encoded_size() {
            let field = Field::new(
                FieldNumber::try_from(1).unwrap(),
                FieldValue::from_uint64(150),
            );
            assert_eq!(field.encoded_size(), 3); // tag (1 byte) + value (2 bytes)

            let field = Field::new(
                FieldNumber::try_from(2).unwrap(),
                FieldValue::from_string("Hello".to_string()),
            );
            assert_eq!(field.encoded_size(), 7); // tag (1 byte) + length (1 byte) + data (5 bytes)
        }

        #[test]
        fn test_fieldvalue_constructors() {
            // Varint types
            assert!(matches!(FieldValue::from_uint64(42), FieldValue::Varint(_)));
            assert!(matches!(FieldValue::from_uint32(42), FieldValue::Varint(_)));
            assert!(matches!(
                FieldValue::from_sint64(-42),
                FieldValue::Varint(_)
            ));
            assert!(matches!(
                FieldValue::from_sint32(-42),
                FieldValue::Varint(_)
            ));
            assert!(matches!(FieldValue::from_int64(-42), FieldValue::Varint(_)));
            assert!(matches!(FieldValue::from_int32(-42), FieldValue::Varint(_)));
            assert!(matches!(FieldValue::from_bool(true), FieldValue::Varint(_)));

            // Fixed-width types
            assert!(matches!(FieldValue::from_fixed32(42), FieldValue::I32(_)));
            assert!(matches!(FieldValue::from_sfixed32(-42), FieldValue::I32(_)));
            assert!(matches!(FieldValue::from_float(3.14), FieldValue::I32(_)));
            assert!(matches!(FieldValue::from_fixed64(42), FieldValue::I64(_)));
            assert!(matches!(FieldValue::from_sfixed64(-42), FieldValue::I64(_)));
            assert!(matches!(FieldValue::from_double(3.14), FieldValue::I64(_)));

            // Length-delimited types
            assert!(matches!(
                FieldValue::from_bytes(vec![1, 2, 3]),
                FieldValue::Len(_)
            ));
            assert!(matches!(
                FieldValue::from_string("test".to_string()),
                FieldValue::Len(_)
            ));
        }
    }

    // Roundtrip tests (require both parser and serializer features)
    #[cfg(all(feature = "parser", feature = "serializer"))]
    mod roundtrip_tests {
        use super::*;

        #[test]
        fn test_roundtrip_varint() {
            let mut buffer = Vec::new();

            // Write
            let original_field = Field::new(
                FieldNumber::try_from(1).unwrap(),
                FieldValue::from_uint64(150),
            );
            buffer.write_protobuf_field(&original_field).unwrap();

            // Read
            let mut reader = buffer.as_slice();
            let read_field = reader.read_protobuf_field().unwrap().unwrap();

            assert_eq!(read_field, original_field);
        }

        #[test]
        fn test_roundtrip_string() {
            let mut buffer = Vec::new();

            // Write
            let original_field = Field::new(
                FieldNumber::try_from(2).unwrap(),
                FieldValue::from_string("Hello, Protocol Buffers!".to_string()),
            );
            buffer.write_protobuf_field(&original_field).unwrap();

            // Read
            let mut reader = buffer.as_slice();
            let read_field = reader.read_protobuf_field().unwrap().unwrap();

            assert_eq!(read_field, original_field);
        }

        #[test]
        fn test_roundtrip_multiple_fields() {
            let mut buffer = Vec::new();

            // Write
            let original_fields = vec![
                Field::new(
                    FieldNumber::try_from(1).unwrap(),
                    FieldValue::from_uint64(150),
                ),
                Field::new(
                    FieldNumber::try_from(2).unwrap(),
                    FieldValue::from_string("Hello".to_string()),
                ),
                Field::new(
                    FieldNumber::try_from(3).unwrap(),
                    FieldValue::from_fixed32(0x12345678),
                ),
            ];
            buffer.write_protobuf_fields(&original_fields).unwrap();

            // Read
            let reader = buffer.as_slice();
            let read_fields: Vec<_> = reader
                .read_protobuf_fields()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();

            assert_eq!(read_fields, original_fields);
        }
    }
}
