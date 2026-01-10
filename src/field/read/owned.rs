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

//! Field reading utilities for Protocol Buffers (owned data)
//!
//! This module provides low-level utilities for reading raw protobuf fields from byte streams.
//! It returns `Field<Vec<u8>>` - owned data suitable for streaming sources like `std::io::Read`.

use crate::field::Field;
use crate::field::read::{FieldValueReader, parse_field};
use crate::varint::{IteratorExtVarint, Varint};
use crate::{ProtobufError, Result};
use ::std::io::Read;

/// Helper struct implementing FieldValueReader for Iterator<Item = u8>
struct IteratorReader<'a, I>(&'a mut I)
where
    I: Iterator<Item = u8>;

impl<'a, I> FieldValueReader<Vec<u8>> for IteratorReader<'a, I>
where
    I: Iterator<Item = u8>,
{
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        self.0.read_varint()
    }

    fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        for byte in bytes.iter_mut() {
            *byte = self.0.next().ok_or(ProtobufError::UnexpectedEof)?;
        }
        Ok(bytes)
    }

    fn read_len(&mut self, length: usize) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(length);
        for _ in 0..length {
            data.push(self.0.next().ok_or(ProtobufError::UnexpectedEof)?);
        }
        Ok(data)
    }
}

/// Helper struct implementing FieldValueReader for Iterator<Item = Result<u8, E>>
struct TryIteratorReader<'a, I, E>(&'a mut I)
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>;

impl<'a, I, E> FieldValueReader<Vec<u8>> for TryIteratorReader<'a, I, E>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        use crate::varint::TryIteratorExtVarint;
        self.0.read_varint()
    }

    fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        for byte in bytes.iter_mut() {
            *byte = self
                .0
                .next()
                .ok_or(ProtobufError::UnexpectedEof)?
                .map_err(Into::into)?;
        }
        Ok(bytes)
    }

    fn read_len(&mut self, length: usize) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(length);
        for _ in 0..length {
            data.push(
                self.0
                    .next()
                    .ok_or(ProtobufError::UnexpectedEof)?
                    .map_err(Into::into)?,
            );
        }
        Ok(data)
    }
}

/// Helper struct implementing FieldValueReader for Read
struct ReadReader<'a, R>(&'a mut R)
where
    R: Read;

impl<'a, R> FieldValueReader<Vec<u8>> for ReadReader<'a, R>
where
    R: Read,
{
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        use crate::varint::ReadExtVarint;
        self.0.read_varint()
    }

    fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        self.0.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn read_len(&mut self, length: usize) -> Result<Vec<u8>> {
        let mut data = vec![0u8; length];
        self.0.read_exact(&mut data)?;
        Ok(data)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a reader
pub struct ProtobufFieldIterator<R> {
    reader: R,
}

impl<R> Iterator for ProtobufFieldIterator<R>
where
    R: Read,
{
    type Item = Result<Field<Vec<u8>>>; // Owned data from Read

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = ReadReader(&mut self.reader);
        match parse_field(&mut reader) {
            Ok(Some(field)) => Some(Ok(field)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Iterator for reading raw protobuf fields sequentially from a byte iterator
pub struct ProtobufFieldIteratorFromBytes<I> {
    iter: I,
}

impl<I> Iterator for ProtobufFieldIteratorFromBytes<I>
where
    I: Iterator<Item = u8>,
{
    type Item = Result<Field<Vec<u8>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = IteratorReader(&mut self.iter);
        match parse_field(&mut reader) {
            Ok(Some(field)) => Some(Ok(field)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Iterator for reading raw protobuf fields sequentially from a byte iterator that yields `Result<u8, E>`
pub struct ProtobufFieldIteratorFromTryBytes<I> {
    iter: I,
}

impl<I, E> Iterator for ProtobufFieldIteratorFromTryBytes<I>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    type Item = Result<Field<Vec<u8>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = TryIteratorReader(&mut self.iter);
        match parse_field(&mut reader) {
            Ok(Some(field)) => Some(Ok(field)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Extension trait for reading raw Protocol Buffer fields from byte iterators.
///
/// This trait provides convenient methods to read fields directly from
/// any iterator that yields bytes. Returns owned data (`Field<Vec<u8>>`).
///
/// # Example
/// ```
/// use ::protobuf_core::IteratorExtProtobuf;
///
/// let bytes = vec![0x08, 0x96, 0x01]; // field 1: 150
/// let iter = bytes.into_iter();
/// let fields: Vec<_> = iter.protobuf_fields().collect::<::std::result::Result<Vec<_>, _>>().unwrap();
/// assert_eq!(fields[0].field_number.as_u32(), 1);
/// ```
pub trait IteratorExtProtobuf {
    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// Returns an iterator that yields `Result<Field<Vec<u8>>>`.
    /// Each field is parsed from the byte stream sequentially.
    fn protobuf_fields(self) -> ProtobufFieldIteratorFromBytes<Self>
    where
        Self: Sized;
}

impl<I> IteratorExtProtobuf for I
where
    I: Iterator<Item = u8>,
{
    fn protobuf_fields(self) -> ProtobufFieldIteratorFromBytes<Self>
    where
        Self: Sized,
    {
        ProtobufFieldIteratorFromBytes { iter: self }
    }
}

/// Extension trait for reading raw Protocol Buffer fields from byte iterators that yield `Result<u8, E>`.
///
/// This trait provides convenient methods to read fields directly from
/// any iterator that yields `Result<u8, E>`, allowing proper error propagation
/// from I/O operations. Returns owned data (`Field<Vec<u8>>`).
///
/// # Example
/// ```
/// use ::std::io::{Cursor, Read};
/// use ::protobuf_core::TryIteratorExtProtobuf;
///
/// let data = vec![0x08, 0x96, 0x01]; // field 1: 150
/// let mut reader = Cursor::new(data);
/// let iter = reader.bytes(); // Iterator<Item = Result<u8, io::Error>>
/// let fields: Vec<_> = iter.protobuf_fields().collect::<Result<Vec<_>, _>>().unwrap();
/// assert_eq!(fields[0].field_number.as_u32(), 1);
/// ```
pub trait TryIteratorExtProtobuf {
    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// Returns an iterator that yields `Result<Field<Vec<u8>>>`.
    /// Each field is parsed from the byte stream sequentially.
    /// I/O errors from the underlying iterator are properly propagated.
    fn protobuf_fields(self) -> ProtobufFieldIteratorFromTryBytes<Self>
    where
        Self: Sized;
}

impl<I, E> TryIteratorExtProtobuf for I
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn protobuf_fields(self) -> ProtobufFieldIteratorFromTryBytes<Self>
    where
        Self: Sized,
    {
        ProtobufFieldIteratorFromTryBytes { iter: self }
    }
}

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// It returns owned data (`Field<Vec<u8>>`) suitable for streaming sources.
/// It does not provide semantic interpretation - that is the caller's responsibility.
///
/// # Example
/// ```
/// use ::protobuf_core::{ReadExtProtobuf, Field, FieldValue};
///
/// fn main() -> Result<(), Box<dyn ::std::error::Error>> {
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
pub trait ReadExtProtobuf {
    /// Read raw protobuf fields from the reader, returning an iterator
    ///
    /// This consumes the reader and returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Returns owned data (`Field<Vec<u8>>`).
    fn read_protobuf_fields(self) -> ProtobufFieldIterator<Self>
    where
        Self: Sized;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_fields(self) -> ProtobufFieldIterator<Self>
    where
        Self: Sized,
    {
        ProtobufFieldIterator { reader: self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldValue;

    #[test]
    fn test_read_single_varint_field() {
        let reader = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 1);
        match &fields[0].value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }
    }

    #[test]
    fn test_read_all_varint_fields() {
        let reader = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
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
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 2);
        match &field.value {
            FieldValue::Len(data) => {
                assert_eq!(&data[..], b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_parse_i32_field() {
        let reader = &[0x15, 0x78, 0x56, 0x34, 0x12][..]; // field 2: 0x12345678 (Fixed32)

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
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
            .collect::<::std::result::Result<Vec<_>, _>>()
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
            .collect::<::std::result::Result<Vec<_>, _>>()
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
                assert_eq!(&data[..], b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_parse_empty_stream() {
        let reader = &[][..];

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 0);
    }

    #[test]
    fn test_iterator_ext_read_single_field() {
        use super::IteratorExtProtobuf;

        let bytes = vec![0x08, 0x96, 0x01]; // field 1: 150
        let iter = bytes.into_iter();
        let fields: Vec<_> = iter
            .protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 1);
        match &fields[0].value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }
    }

    #[test]
    fn test_iterator_ext_read_multiple_fields() {
        use super::IteratorExtProtobuf;

        let bytes = vec![
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ];
        let iter = bytes.into_iter();
        let fields: Vec<_> = iter
            .protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 2);

        // Check first field
        assert_eq!(fields[0].field_number.as_u32(), 1);
        match &fields[0].value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }

        // Check second field
        assert_eq!(fields[1].field_number.as_u32(), 2);
        match &fields[1].value {
            FieldValue::Len(data) => {
                assert_eq!(&data[..], b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_try_iterator_ext_read_single_field() {
        use super::TryIteratorExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![0x08, 0x96, 0x01]; // field 1: 150
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let fields: Vec<_> = iter
            .protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 1);
        match &fields[0].value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }
    }

    #[test]
    fn test_try_iterator_ext_read_multiple_fields() {
        use super::TryIteratorExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ];
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let fields: Vec<_> = iter
            .protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 2);

        // Check first field
        assert_eq!(fields[0].field_number.as_u32(), 1);
        match &fields[0].value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }

        // Check second field
        assert_eq!(fields[1].field_number.as_u32(), 2);
        match &fields[1].value {
            FieldValue::Len(data) => {
                assert_eq!(&data[..], b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }
}
