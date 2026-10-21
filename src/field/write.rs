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
    /// use ::protobuf_core::{WriteExtProtobuf, Field, FieldValue, FieldNumber};
    ///
    /// fn main() -> Result<(), Box<dyn ::std::error::Error>> {
    ///     let mut buffer = Vec::new();
    ///     
    ///     let field: Field<Vec<u8>> = Field::new(
    ///         FieldNumber::try_new(1).unwrap(),
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
    /// use ::protobuf_core::{WriteExtProtobuf, Field, FieldValue, FieldNumber};
    ///
    /// fn main() -> Result<(), Box<dyn ::std::error::Error>> {
    ///     let mut buffer = Vec::new();
    ///     
    ///     let fields = vec![
    ///         Field::new(FieldNumber::try_new(1).unwrap(), FieldValue::from_uint64(150)),
    ///         Field::new(FieldNumber::try_new(2).unwrap(), FieldValue::Len("Hello".to_string().into_bytes())),
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
            FieldNumber::try_new(1).unwrap(),
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
            FieldValue::Len("Hel".to_string().into_bytes()),
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
                FieldNumber::try_new(1).unwrap(),
                FieldValue::from_uint64(150),
            ),
            Field::new(
                FieldNumber::try_from(2).unwrap(),
                FieldValue::Len("Hel".to_string().into_bytes()),
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
            FieldNumber::try_new(1).unwrap(),
            FieldValue::from_uint64(150),
        );
        assert_eq!(field.encoded_size(), 3); // tag (1 byte) + value (2 bytes)

        let field: Field<Vec<u8>> = Field::new(
            FieldNumber::try_from(2).unwrap(),
            FieldValue::Len("Hello".to_string().into_bytes()),
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
            FieldValue::<Vec<u8>>::from_float(1.5),
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
            FieldValue::<Vec<u8>>::from_double(1.5),
            FieldValue::I64(_)
        ));

        // Length-delimited types
        assert!(matches!(FieldValue::Len(vec![1, 2, 3]), FieldValue::Len(_)));
        assert!(matches!(
            FieldValue::Len("test".to_string().into_bytes()),
            FieldValue::Len(_)
        ));
    }

    #[test]
    fn test_scalar_encoding_bytes_and_sizes() {
        match FieldValue::<Vec<u8>>::from_float(-0.0) {
            FieldValue::I32(bytes) => assert_eq!(bytes, (-0.0f32).to_le_bytes()),
            other => panic!("expected f32 bytes, got {other:?}"),
        }
        match FieldValue::<Vec<u8>>::from_double(1.5) {
            FieldValue::I64(bytes) => assert_eq!(bytes, 1.5f64.to_le_bytes()),
            other => panic!("expected f64 bytes, got {other:?}"),
        }
        match FieldValue::<Vec<u8>>::from_sfixed32(i32::MIN) {
            FieldValue::I32(bytes) => assert_eq!(bytes, i32::MIN.to_le_bytes()),
            other => panic!("expected sfixed32 bytes, got {other:?}"),
        }
        match FieldValue::<Vec<u8>>::from_sfixed64(i64::MIN) {
            FieldValue::I64(bytes) => assert_eq!(bytes, i64::MIN.to_le_bytes()),
            other => panic!("expected sfixed64 bytes, got {other:?}"),
        }
        match FieldValue::<Vec<u8>>::from_fixed32(u32::MAX) {
            FieldValue::I32(bytes) => assert_eq!(bytes, u32::MAX.to_le_bytes()),
            other => panic!("expected fixed32 bytes, got {other:?}"),
        }
        match FieldValue::<Vec<u8>>::from_fixed64(u64::MAX) {
            FieldValue::I64(bytes) => assert_eq!(bytes, u64::MAX.to_le_bytes()),
            other => panic!("expected fixed64 bytes, got {other:?}"),
        }

        let empty: Field<Vec<u8>> = Field::new(
            FieldNumber::try_new(1).unwrap(),
            FieldValue::Len(Vec::new()),
        );
        assert_eq!(empty.value.encoded_size(), 1);
        assert_eq!(empty.encoded_size(), 2);
        let mut buffer = Vec::new();
        assert_eq!(buffer.write_protobuf_field(&empty).unwrap(), 2);
        assert_eq!(buffer, vec![0x0A, 0x00]);

        let negative: Field<Vec<u8>> =
            Field::new(FieldNumber::try_new(1).unwrap(), FieldValue::from_int32(-1));
        assert_eq!(negative.value.encoded_size(), 10);
        assert_eq!(negative.encoded_size(), 11);
        let mut buffer = Vec::new();
        assert_eq!(buffer.write_protobuf_field(&negative).unwrap(), 11);
        assert_eq!(buffer.len(), 11);
        assert_eq!(buffer[0], 0x08);
    }

    mod roundtrip_tests {
        use super::*;
        use crate::field::read::ReadExtProtobuf;

        #[test]
        fn test_roundtrip_varint() {
            let mut buffer = Vec::new();

            // Write
            let original_field: Field<Vec<u8>> = Field::new(
                FieldNumber::try_new(1).unwrap(),
                FieldValue::from_uint64(150),
            );
            buffer.write_protobuf_field(&original_field).unwrap();

            // Read
            let reader = buffer.as_slice();
            let fields: Vec<_> = reader
                .read_protobuf_fields()
                .collect::<::std::result::Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0], original_field);
        }

        #[test]
        fn test_roundtrip_string() {
            let mut buffer = Vec::new();

            // Write
            let original_field: Field<Vec<u8>> = Field::new(
                FieldNumber::try_from(2).unwrap(),
                FieldValue::Len("Hello, Protocol Buffers!".to_string().into_bytes()),
            );
            buffer.write_protobuf_field(&original_field).unwrap();

            // Read
            let reader = buffer.as_slice();
            let fields: Vec<_> = reader
                .read_protobuf_fields()
                .collect::<::std::result::Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(fields.len(), 1);
            let read_field = &fields[0];

            assert_eq!(*read_field, original_field);
        }

        #[test]
        fn test_roundtrip_multiple_fields() {
            let mut buffer = Vec::new();

            // Write
            let original_fields: Vec<Field<Vec<u8>>> = vec![
                Field::new(
                    FieldNumber::try_new(1).unwrap(),
                    FieldValue::from_uint64(150),
                ),
                Field::new(
                    FieldNumber::try_from(2).unwrap(),
                    FieldValue::Len("Hello".to_string().into_bytes()),
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
                .collect::<::std::result::Result<Vec<_>, _>>()
                .unwrap();

            assert_eq!(read_fields, original_fields);
        }

        #[test]
        fn test_roundtrip_scalar_edges() {
            let original_fields: Vec<Field<Vec<u8>>> = vec![
                Field::new(FieldNumber::try_new(1).unwrap(), FieldValue::from_int32(-1)),
                Field::new(
                    FieldNumber::try_new(2).unwrap(),
                    FieldValue::from_sint32(i32::MIN),
                ),
                Field::new(
                    FieldNumber::try_new(3).unwrap(),
                    FieldValue::from_sint64(i64::MIN),
                ),
                Field::new(
                    FieldNumber::try_new(4).unwrap(),
                    FieldValue::from_float(-0.0),
                ),
                Field::new(
                    FieldNumber::try_new(5).unwrap(),
                    FieldValue::from_double(1.5),
                ),
                Field::new(
                    FieldNumber::try_new(6).unwrap(),
                    FieldValue::from_sfixed64(i64::MIN),
                ),
                Field::new(
                    FieldNumber::try_new(7).unwrap(),
                    FieldValue::from_bool(false),
                ),
                Field::new(
                    FieldNumber::try_new(16).unwrap(),
                    FieldValue::Len(Vec::new()),
                ),
            ];

            let mut buffer = Vec::new();
            let written = buffer.write_protobuf_fields(&original_fields).unwrap();
            let expected = original_fields
                .iter()
                .map(|field| field.encoded_size())
                .sum::<usize>();
            assert_eq!(written, expected);

            let read_fields: Vec<_> = buffer
                .as_slice()
                .read_protobuf_fields()
                .collect::<::std::result::Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(read_fields, original_fields);
        }
    }
}
