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

//! Field reading utilities for Protocol Buffers (borrowed references)
//!
//! This module provides low-level utilities for reading raw protobuf fields from byte slices.
//! Unlike the `owned` module which uses `std::io::Read` and returns owned data,
//! this module works directly with slices and returns references to sub-slices (`Field<&'a [u8]>`).

use crate::field::Field;
use crate::field::read::{FieldValueReader, parse_field};
use crate::varint::{ReadExtVarint, Varint};
use crate::{ProtobufError, Result};
use ::std::convert::AsRef;
use ::std::io::Cursor;

/// Helper struct implementing FieldValueReader for slice-based readers
pub(crate) struct SliceReader<'a, 'b> {
    pub(crate) slice: &'b mut &'a [u8],
}

impl<'a, 'b> FieldValueReader<&'a [u8]> for SliceReader<'a, 'b> {
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        let mut cursor = Cursor::new(*self.slice);
        let varint_result = cursor.read_varint()?;
        let Some(varint) = varint_result else {
            return Ok(None);
        };
        let consumed = cursor.position() as usize;
        *self.slice = &(*self.slice)[consumed..];
        Ok(Some(varint))
    }

    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let (bytes, remaining) = (*self.slice)
            .split_first_chunk::<N>()
            .ok_or(ProtobufError::UnexpectedEof)?;
        *self.slice = remaining;
        Ok(*bytes)
    }

    fn read_length_delimited(&mut self, length: usize) -> Result<&'a [u8]> {
        let (value_slice, remaining) = (*self.slice)
            .split_at_checked(length)
            .ok_or(ProtobufError::UnexpectedEof)?;
        *self.slice = remaining;
        Ok(value_slice)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a slice
pub struct ProtobufFieldSliceIterator<'a> {
    slice: &'a [u8],
}

impl<'a> Iterator for ProtobufFieldSliceIterator<'a> {
    type Item = Result<Field<&'a [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = SliceReader {
            slice: &mut self.slice,
        };
        parse_field(&mut reader).transpose()
    }
}

/// Extension trait for reading raw Protocol Buffer fields from slice types
///
/// This trait provides low-level utilities for reading field-by-field from a byte slice.
/// It returns borrowed references (`Field<&'a [u8]>`) suitable for slice sources.
/// It does not provide semantic interpretation - that is the caller's responsibility.
///
/// # Example
/// ```
/// use ::protobuf_core::AsRefExtProtobuf;
///
/// let slice = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
///
/// for field_result in slice.read_protobuf_fields() {
///     let field = field_result?;
///     // Process field...
/// }
/// # Ok::<(), protobuf_core::ProtobufError>(())
/// ```
pub trait AsRefExtProtobuf: AsRef<[u8]> {
    /// Read raw protobuf fields from the slice, returning an iterator
    ///
    /// This returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Returns borrowed references (`Field<&'a [u8]>`).
    ///
    /// This method is available for all types implementing `AsRef<[u8]>`.
    fn read_protobuf_fields(&self) -> ProtobufFieldSliceIterator<'_> {
        ProtobufFieldSliceIterator {
            slice: self.as_ref(),
        }
    }
}

impl<T> AsRefExtProtobuf for T where T: AsRef<[u8]> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldValue;
    use crate::field::read::parse_field;

    #[test]
    fn test_parse_varint_field_from_slice() {
        let data = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let mut slice = data;
        let mut reader = SliceReader { slice: &mut slice };
        let field = parse_field(&mut reader).unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 1);
        assert_eq!(data.len() - slice.len(), 3);
        assert!(slice.is_empty());
        match field.value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }
    }

    #[test]
    fn test_parse_len_field_from_slice() {
        let data = &[0x12, 0x03, 0x48, 0x65, 0x6c][..]; // field 2: "Hel"

        let mut slice = data;
        let mut reader = SliceReader { slice: &mut slice };
        let field = parse_field(&mut reader).unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 2);
        assert_eq!(data.len() - slice.len(), 5);
        assert!(slice.is_empty());
        match field.value {
            FieldValue::Len(data_slice) => {
                assert_eq!(data_slice, b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_parse_i32_field_from_slice() {
        let data = &[0x15, 0x78, 0x56, 0x34, 0x12][..]; // field 2: 0x12345678

        let mut slice = data;
        let mut reader = SliceReader { slice: &mut slice };
        let field = parse_field(&mut reader).unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 2);
        assert_eq!(data.len() - slice.len(), 5);
        assert!(slice.is_empty());
        match field.value {
            FieldValue::I32(bytes) => {
                assert_eq!(bytes, [0x78, 0x56, 0x34, 0x12]);
            }
            _ => panic!("Expected I32 field"),
        }
    }

    #[test]
    fn test_parse_empty_slice() {
        let mut data = &[][..];
        let mut reader = SliceReader { slice: &mut data };
        assert!(parse_field(&mut reader).unwrap().is_none());
    }

    #[test]
    fn test_slice_ext_read_single_field() {
        let slice = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let fields: Vec<_> = slice
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
    fn test_slice_ext_read_all_fields() {
        let slice = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
        // field 1: 150, field 2: "Hel"

        let fields: Vec<_> = slice
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 2);

        let field1 = &fields[0];
        assert_eq!(field1.field_number.as_u32(), 1);
        match &field1.value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }

        let field2 = &fields[1];
        assert_eq!(field2.field_number.as_u32(), 2);
        match &field2.value {
            FieldValue::Len(data_slice) => {
                assert_eq!(data_slice, b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }
}
