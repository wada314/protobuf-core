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
use crate::slice::SliceAdvance;
use crate::tag::read_tag;
use crate::varint::SliceExtVarint;
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};
use ::std::convert::AsRef;

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

        let mut remaining = &self.slice[self.position..];
        match parse_field_from_slice(&mut remaining) {
            Ok(Some(field)) => {
                // Update position based on how much slice was advanced
                let consumed = self.slice.len() - remaining.len() - self.position;
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
pub trait SliceExtProtobuf: AsRef<[u8]> {
    /// Read a single raw protobuf field from the slice
    ///
    /// Returns `Ok(Some(field))` if a field was successfully read,
    /// `Ok(None)` if the slice is empty, or an error if reading failed.
    ///
    /// The slice is advanced to point after the consumed field.
    ///
    /// This method requires `Self: SliceAdvance`. Typical types implementing
    /// `SliceAdvance` are `&[u8]` or `Cursor<T>` where `T: AsRef<[u8]>`.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::SliceExtProtobuf;
    ///
    /// let mut slice = &[0x08, 0x96, 0x01][..]; // field 1: 150
    ///
    /// if let Some(field) = slice.read_protobuf_field()? {
    ///     assert_eq!(field.field_number.as_u32(), 1);
    ///     // slice now points after the consumed field
    /// }
    /// # Ok::<(), protobuf_core::ProtobufError>(())
    /// ```
    fn read_protobuf_field(&mut self) -> Result<Option<Field<&[u8]>>>
    where
        Self: SliceAdvance;

    /// Read raw protobuf fields from the slice, returning an iterator
    ///
    /// This returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    ///
    /// This method is available for all types implementing `AsRef<[u8]>`.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::SliceExtProtobuf;
    ///
    /// let slice = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
    ///
    /// for field_result in slice.read_protobuf_fields() {
    ///     let field = field_result?;
    ///     // Process field...
    /// }
    /// # Ok::<(), protobuf_core::ProtobufError>(())
    /// ```
    fn read_protobuf_fields(&self) -> ProtobufFieldSliceIterator<'_> {
        ProtobufFieldSliceIterator {
            slice: self.as_ref(),
            position: 0,
        }
    }
}

impl SliceExtProtobuf for &[u8] {
    fn read_protobuf_field(&mut self) -> Result<Option<Field<&[u8]>>>
    where
        Self: SliceAdvance,
    {
        parse_field_from_slice(self)
    }
}

fn parse_field_from_slice<'a>(slice: &mut &'a [u8]) -> Result<Option<Field<&'a [u8]>>> {
    if slice.is_empty() {
        return Ok(None);
    }

    // Read tag
    let mut iter = slice.iter().copied();
    let tag = match read_tag(&mut iter) {
        Ok(Some(tag)) => tag,
        Ok(None) => {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    let tag_bytes = tag.to_encoded().varint_size();
    if slice.len() < tag_bytes {
        return Err(ProtobufError::UnexpectedEof);
    }
    *slice = &slice[tag_bytes..];

    let value = match tag.wire_type {
        WireType::Varint => {
            let varint = match slice.read_varint()? {
                Some(v) => v,
                None => {
                    return Err(ProtobufError::UnexpectedEof);
                }
            };
            FieldValue::Varint(varint)
        }
        WireType::Int32 => {
            if slice.len() < 4 {
                return Err(ProtobufError::UnexpectedEof);
            }
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(&slice[..4]);
            *slice = &slice[4..];
            FieldValue::I32(bytes)
        }
        WireType::Int64 => {
            if slice.len() < 8 {
                return Err(ProtobufError::UnexpectedEof);
            }
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&slice[..8]);
            *slice = &slice[8..];
            FieldValue::I64(bytes)
        }
        WireType::Len => {
            // Read length prefix (varint)
            let length_varint = match slice.read_varint()? {
                Some(v) => v,
                None => {
                    return Err(ProtobufError::UnexpectedEof);
                }
            };
            // After read_varint, slice points to the start of the value
            let length = length_varint.try_to_uint32()? as usize;
            let (value_slice, remaining) = slice
                .split_at_checked(length)
                .ok_or(ProtobufError::UnexpectedEof)?;
            *slice = remaining;
            FieldValue::Len(value_slice)
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

    Ok(Some(field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_varint_field_from_slice() {
        let data = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let mut slice = data;
        let field = parse_field_from_slice(&mut slice).unwrap().unwrap();
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
        let field = parse_field_from_slice(&mut slice).unwrap().unwrap();
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
        let field = parse_field_from_slice(&mut slice).unwrap().unwrap();
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
        assert!(parse_field_from_slice(&mut data).unwrap().is_none());
    }

    #[test]
    fn test_slice_ext_read_single_field() {
        let mut slice = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let field = slice.read_protobuf_field().unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 1);
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
