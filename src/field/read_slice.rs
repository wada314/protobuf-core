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

//! Field reading utilities for Protocol Buffers from slices
//!
//! This module provides low-level utilities for reading raw protobuf fields from byte slices.
//! Unlike the `read` module which uses `std::io::Read` and returns owned data,
//! this module works directly with slices and returns references to sub-slices.

use crate::field::{Field, FieldValue};
use crate::tag::read_tag;
use crate::varint::IteratorExtVarint;
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};

/// Iterator for reading raw protobuf fields sequentially from a slice
pub struct ProtobufFieldSliceIterator<'a> {
    slice: &'a [u8],
    position: usize,
}

impl<'a> Iterator for ProtobufFieldSliceIterator<'a> {
    type Item = Result<Field<&'a [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.slice.len() {
            return None;
        }

        let remaining = &self.slice[self.position..];
        match parse_field_from_slice(remaining) {
            Ok(Some((field, consumed))) => {
                self.position += consumed;
                Some(Ok(field))
            }
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Extension trait for reading raw Protocol Buffer fields from slice types
///
/// This trait provides low-level utilities for reading field-by-field from a byte slice.
/// It does not provide semantic interpretation - that is the caller's responsibility.
pub trait SliceExtProtobuf {
    /// Read a single raw protobuf field from the slice
    ///
    /// Returns `Ok(Some((field, consumed_bytes)))` if a field was successfully read,
    /// `Ok(None)` if the slice is empty, or an error if reading failed.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::field::read_slice::SliceExtProtobuf;
    ///
    /// let mut slice = &[0x08, 0x96, 0x01][..]; // field 1: 150
    ///
    /// if let Some((field, consumed)) = slice.read_protobuf_field()? {
    ///     assert_eq!(field.field_number.as_u32(), 1);
    ///     assert_eq!(consumed, 3);
    /// }
    /// # Ok::<(), protobuf_core::ProtobufError>(())
    /// ```
    fn read_protobuf_field(&mut self) -> Result<Option<(Field<&[u8]>, usize)>>;

    /// Read raw protobuf fields from the slice, returning an iterator
    ///
    /// This returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::field::read_slice::SliceExtProtobuf;
    ///
    /// let slice = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
    ///
    /// for field_result in slice.read_protobuf_fields() {
    ///     let field = field_result?;
    ///     // Process field...
    /// }
    /// # Ok::<(), protobuf_core::ProtobufError>(())
    /// ```
    fn read_protobuf_fields(&self) -> ProtobufFieldSliceIterator<'_>;
}

impl SliceExtProtobuf for &[u8] {
    fn read_protobuf_field(&mut self) -> Result<Option<(Field<&[u8]>, usize)>> {
        parse_field_from_slice(*self).map(|result| {
            if let Some((field, consumed)) = result {
                *self = &self[consumed..];
                Some((field, consumed))
            } else {
                None
            }
        })
    }

    fn read_protobuf_fields(&self) -> ProtobufFieldSliceIterator<'_> {
        ProtobufFieldSliceIterator {
            slice: self,
            position: 0,
        }
    }
}

/// Parse a single field from a byte slice, returning the field and the number of bytes consumed.
///
/// This function reads a field from the beginning of the slice and returns:
/// - `Ok(Some((field, consumed_bytes)))` if a field was successfully parsed
/// - `Ok(None)` if the slice is empty (no field to parse)
/// - `Err(err)` if parsing failed
///
/// The returned `Field<&'a [u8]>` contains references to the input slice,
/// so the field's lifetime is tied to the input slice.
///
/// # Example
/// ```
/// use protobuf_core::field::read_slice::parse_field_from_slice;
///
/// let data = &[0x08, 0x96, 0x01][..]; // field 1: 150
/// match parse_field_from_slice(data)? {
///     Some((field, consumed)) => {
///         assert_eq!(field.field_number.as_u32(), 1);
///         assert_eq!(consumed, 3);
///     }
///     None => {}
/// }
/// # Ok::<(), protobuf_core::ProtobufError>(())
/// ```
fn parse_field_from_slice(data: &[u8]) -> Result<Option<(Field<&[u8]>, usize)>> {
    if data.is_empty() {
        return Ok(None);
    }

    // Read tag
    let mut iter = data.iter().copied();
    let tag = match read_tag(&mut iter) {
        Ok(Some(tag)) => tag,
        Ok(None) => {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    let tag_bytes = tag.to_encoded().varint_size();
    let remaining_after_tag = &data[tag_bytes..];

    let value = match tag.wire_type {
        WireType::Varint => {
            let varint_iter = remaining_after_tag.iter().copied();
            let varint = match varint_iter.try_collect_varint() {
                Ok(Some(v)) => v,
                Ok(None) => {
                    return Err(ProtobufError::UnexpectedEof);
                }
                Err(e) => return Err(e),
            };
            FieldValue::Varint(varint)
        }
        WireType::Int32 => {
            if remaining_after_tag.len() < 4 {
                return Err(ProtobufError::UnexpectedEof);
            }
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(&remaining_after_tag[..4]);
            FieldValue::I32(bytes)
        }
        WireType::Int64 => {
            if remaining_after_tag.len() < 8 {
                return Err(ProtobufError::UnexpectedEof);
            }
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&remaining_after_tag[..8]);
            FieldValue::I64(bytes)
        }
        WireType::Len => {
            // Read length prefix (varint)
            let length_iter = remaining_after_tag.iter().copied();
            let length_varint = match length_iter.try_collect_varint() {
                Ok(Some(v)) => v,
                Ok(None) => {
                    return Err(ProtobufError::UnexpectedEof);
                }
                Err(e) => return Err(e),
            };
            let length = length_varint.try_to_uint32()? as usize;
            let length_bytes = length_varint.varint_size();
            let value_start = tag_bytes + length_bytes;
            let value_end = value_start + length;
            if value_end > data.len() {
                return Err(ProtobufError::UnexpectedEof);
            }
            FieldValue::Len(&data[value_start..value_end])
        }
        _ => {
            return Err(ProtobufError::InvalidWireType {
                value: tag.wire_type as u8,
            });
        }
    };

    let field = Field {
        field_number: tag.field_number,
        value,
    };

    let consumed = field.encoded_size();
    Ok(Some((field, consumed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_varint_field_from_slice() {
        let data = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let (field, consumed) = parse_field_from_slice(data).unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 1);
        assert_eq!(consumed, 3);
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

        let (field, consumed) = parse_field_from_slice(data).unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 2);
        assert_eq!(consumed, 5);
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

        let (field, consumed) = parse_field_from_slice(data).unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 2);
        assert_eq!(consumed, 5);
        match field.value {
            FieldValue::I32(bytes) => {
                assert_eq!(bytes, [0x78, 0x56, 0x34, 0x12]);
            }
            _ => panic!("Expected I32 field"),
        }
    }

    #[test]
    fn test_parse_empty_slice() {
        let data = &[][..];
        assert!(parse_field_from_slice(data).unwrap().is_none());
    }

    #[test]
    fn test_slice_ext_read_single_field() {
        let mut slice = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let (field, consumed) = slice.read_protobuf_field().unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 1);
        assert_eq!(consumed, 3);
        assert!(slice.is_empty());

        // Should return None for empty slice
        assert!(slice.read_protobuf_field().unwrap().is_none());
    }

    #[test]
    fn test_slice_ext_read_all_fields() {
        let slice = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
        // field 1: 150, field 2: "Hel"

        let fields: Vec<_> = slice
            .read_protobuf_fields()
            .collect::<std::result::Result<Vec<_>, _>>()
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
