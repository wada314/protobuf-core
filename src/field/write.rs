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

//! Field writing utilities for Protocol Buffers
//!
//! This module provides low-level utilities for writing raw protobuf fields to byte streams.

use crate::Result;
use crate::field::{Field, FieldValue};
use crate::tag::Tag;
use crate::varint::{Varint, WriteExtVarint};
use crate::wire_format::WireType;
use ::std::io::Write;

/// Extension trait for writing raw Protocol Buffer fields to `Write` types
///
/// This trait provides low-level utilities for writing fields to a byte stream.
/// It is the counterpart to `ReadExtProtobuf` for serialization.
pub trait WriteExtProtobuf {
    /// Write a single raw protobuf field to the writer (tag + value)
    ///
    /// Returns the number of bytes written.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::field::{WriteExtProtobuf, Field, FieldValue};
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
    fn write_protobuf_field<L: AsRef<[u8]>>(&mut self, field: &Field<L>) -> Result<usize>;

    /// Write multiple raw protobuf fields to the writer
    ///
    /// Returns the total number of bytes written.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::field::{WriteExtProtobuf, Field, FieldValue};
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
    fn write_protobuf_fields<'a, L: AsRef<[u8]> + 'a, I>(&mut self, fields: I) -> Result<usize>
    where
        I: IntoIterator<Item = &'a Field<L>>;
}

impl<W> WriteExtProtobuf for W
where
    W: Write,
{
    fn write_protobuf_field<L: AsRef<[u8]>>(&mut self, field: &Field<L>) -> Result<usize> {
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
                let data_slice = data.as_ref();
                let length_varint = Varint::from_uint64(data_slice.len() as u64);
                bytes_written += self.write_varint(&length_varint)?;
                // Write data
                self.write_all(data_slice)?;
                bytes_written += data_slice.len();
            }
        }

        Ok(bytes_written)
    }

    fn write_protobuf_fields<'a, L: AsRef<[u8]> + 'a, I>(&mut self, fields: I) -> Result<usize>
    where
        I: IntoIterator<Item = &'a Field<L>>,
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
    use crate::field_number::FieldNumber;

    #[test]
    fn test_write_single_varint_field() {
        let mut buffer = Vec::new();

        let field: Field<Vec<u8>> = Field::new(
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

        let field: Field<Vec<u8>> = Field::new(
            FieldNumber::try_from(2).unwrap(),
            FieldValue::from_string("Hel".to_string()),
        );

        buffer.write_protobuf_field(&field).unwrap();
        assert_eq!(buffer, vec![0x12, 0x03, 0x48, 0x65, 0x6c]); // field 2: "Hel"
    }

    #[test]
    fn test_write_i32_field() {
        let mut buffer = Vec::new();

        let field: Field<Vec<u8>> = Field::new(
            FieldNumber::try_from(2).unwrap(),
            FieldValue::from_fixed32(0x12345678),
        );

        buffer.write_protobuf_field(&field).unwrap();
        assert_eq!(buffer, vec![0x15, 0x78, 0x56, 0x34, 0x12]); // field 2: 0x12345678
    }

    #[test]
    fn test_write_i64_field() {
        let mut buffer = Vec::new();

        let field: Field<Vec<u8>> = Field::new(
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

        let fields: Vec<Field<Vec<u8>>> = vec![
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
        let field: Field<Vec<u8>> = Field::new(
            FieldNumber::try_from(1).unwrap(),
            FieldValue::from_uint64(150),
        );
        assert_eq!(field.encoded_size(), 3); // tag (1 byte) + value (2 bytes)

        let field: Field<Vec<u8>> = Field::new(
            FieldNumber::try_from(2).unwrap(),
            FieldValue::from_string("Hello".to_string()),
        );
        assert_eq!(field.encoded_size(), 7); // tag (1 byte) + length (1 byte) + data (5 bytes)
    }

    #[test]
    fn test_fieldvalue_constructors() {
        // Varint types
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_uint64(42),
            FieldValue::Varint(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_uint32(42),
            FieldValue::Varint(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_sint64(-42),
            FieldValue::Varint(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_sint32(-42),
            FieldValue::Varint(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_int64(-42),
            FieldValue::Varint(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_int32(-42),
            FieldValue::Varint(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_bool(true),
            FieldValue::Varint(_)
        ));

        // Fixed-width types
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_fixed32(42),
            FieldValue::I32(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_sfixed32(-42),
            FieldValue::I32(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_float(3.14),
            FieldValue::I32(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_fixed64(42),
            FieldValue::I64(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_sfixed64(-42),
            FieldValue::I64(_)
        ));
        assert!(matches!(
            FieldValue::<Vec<u8>>::from_double(3.14),
            FieldValue::I64(_)
        ));

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

    // Roundtrip tests (require both read and write features)
    #[cfg(feature = "read")]
    mod roundtrip_tests {
        use super::*;
        use crate::field::read::ReadExtProtobuf;

        #[test]
        fn test_roundtrip_varint() {
            let mut buffer = Vec::new();

            // Write
            let original_field: Field<Vec<u8>> = Field::new(
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
            let original_field: Field<Vec<u8>> = Field::new(
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
            let original_fields: Vec<Field<Vec<u8>>> = vec![
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
