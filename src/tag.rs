//! Tag encoding and decoding for Protocol Buffers.
//!
//! This module provides functions for building and parsing protobuf tags,
//! which combine field numbers with wire types.

use crate::field_number::FieldNumber;
use crate::varint::Varint;
use crate::wire_format::{WireType, FIELD_NUMBER_SHIFT, WIRE_TYPE_MASK};
use crate::Result;
use ::std::convert::TryFrom;
use std::io::Read;

/// A protobuf tag containing field number and wire type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub field_number: FieldNumber,
    pub wire_type: WireType,
}

impl Tag {
    /// Build the encoded tag value as a `Varint`.
    pub fn to_encoded(self) -> Varint {
        let value: u32 =
            (u32::from(self.field_number) << FIELD_NUMBER_SHIFT) | (self.wire_type as u32);
        Varint::from_uint32(value)
    }

    /// Parse a tag from a `Varint` value.
    pub fn from_encoded(encoded: Varint) -> Result<Self> {
        let value = encoded.try_to_uint32()?;
        let field_number_value = value >> FIELD_NUMBER_SHIFT;
        let wire_type_value = value & WIRE_TYPE_MASK;

        // Create field number
        let field_number = FieldNumber::try_new(field_number_value)?;

        // Parse wire type
        let wire_type = WireType::try_from(wire_type_value as u8)?;

        Ok(Self {
            field_number,
            wire_type,
        })
    }
}

/// Read a tag from a byte iterator.
///
/// Returns the tag containing field number and wire type.
/// Returns `Ok(None)` if no input is available.
/// Returns `Err(ProtobufError)` if the tag is malformed.
pub fn read_tag<I>(iter: &mut I) -> Result<Option<Tag>>
where
    I: Iterator<Item = u8>,
{
    use crate::varint::IteratorExtVarint;

    let varint_result = iter.try_collect_varint()?;
    let Some(varint) = varint_result else {
        return Ok(None);
    };
    let tag = Tag::from_encoded(varint)?;
    Ok(Some(tag))
}

/// Extension trait for reading tags from Read instances.
///
/// This trait provides a convenient method to read tags directly from
/// any type that implements `std::io::Read`.
///
/// # Example
/// ```
/// use std::io::Cursor;
/// use protobuf_core::tag::ReadExtTag;
///
/// let data = vec![0x08]; // tag 1:0 (field 1, wire type 0)
/// let mut reader = Cursor::new(data);
/// let tag = reader.read_tag().unwrap().unwrap();
/// assert_eq!(tag.field_number.as_u32(), 1);
/// ```
pub trait ReadExtTag {
    /// Read a tag from this reader.
    ///
    /// Returns the Tag `Ok(Some(tag))` if successfully read.
    /// Returns `Ok(None)` if no input is available (EOF).
    /// Returns `Err(ProtobufError)` if the tag is malformed.
    fn read_tag(&mut self) -> Result<Option<Tag>>;
}

impl<R> ReadExtTag for R
where
    R: Read,
{
    fn read_tag(&mut self) -> Result<Option<Tag>> {
        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];

        // Read varint for tag
        loop {
            let n = self.read(&mut byte)?;
            if n == 0 {
                return Ok(None); // EOF
            }
            buffer.push(byte[0]);
            if (byte[0] & 0x80) == 0 {
                break; // Last byte
            }
        }

        let mut iter = buffer.into_iter();
        read_tag(&mut iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire_format::WireType;
    use crate::ProtobufError;

    #[test]
    fn test_tag_build_and_parse() {
        let field_number = FieldNumber::try_new(1).unwrap();
        let wire_type = WireType::Varint;
        let tag = Tag {
            field_number,
            wire_type,
        };

        let encoded = tag.to_encoded();
        let parsed_tag = Tag::from_encoded(encoded).unwrap();
        assert_eq!(parsed_tag.field_number, field_number);
        assert_eq!(parsed_tag.wire_type, wire_type);
    }

    #[test]
    fn test_tag_struct() {
        let field_number = FieldNumber::try_new(1).unwrap();
        let wire_type = WireType::Varint;
        let tag = Tag {
            field_number,
            wire_type,
        };

        assert_eq!(tag.field_number, field_number);
        assert_eq!(tag.wire_type, wire_type);

        // Test encoding and decoding
        let encoded = tag.to_encoded();
        let decoded = Tag::from_encoded(encoded).unwrap();
        assert_eq!(decoded, tag);
    }

    #[test]
    fn test_parse_tag_invalid_field_number() {
        // Test with field number 0 (invalid)
        let tag = Varint::from_uint32(0); // field_number = 0, wire_type = 0
        let result = Tag::from_encoded(tag);
        assert!(result.is_err());

        if let Err(ProtobufError::FieldNumberOutOfRange { value }) = result {
            assert_eq!(value, "0");
        } else {
            panic!("Expected FieldNumberOutOfRange error");
        }
    }

    #[test]
    fn test_parse_tag_invalid_wire_type() {
        // Test with invalid wire type 6
        let tag = Varint::from_uint32((1 << 3) | 6); // field_number = 1, wire_type = 6
        let result = Tag::from_encoded(tag);
        assert!(result.is_err());

        if let Err(ProtobufError::InvalidWireType { value }) = result {
            assert_eq!(value, 6);
        } else {
            panic!("Expected InvalidWireType error");
        }
    }

    #[test]
    fn test_read_tag() {
        let bytes = vec![0x08]; // tag 1:0 (field 1, wire type 0)
        let mut iter = bytes.into_iter();
        let tag = read_tag(&mut iter).unwrap().unwrap();
        assert_eq!(tag.field_number, FieldNumber::try_new(1).unwrap());
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[test]
    fn test_read_tag_u64_overflow() {
        // Test case where the varint value exceeds u32::MAX
        // This should trigger VarintDowncastOutOfRange error
        // u32::MAX = 4,294,967,295 (0xFFFFFFFF)
        // Use 0x100000000 (4,294,967,296) which exceeds u32::MAX
        let bytes = vec![0x80, 0x80, 0x80, 0x80, 0x10]; // Value: 0x100000000 (exceeds u32::MAX)
        let mut iter = bytes.into_iter();
        let result = read_tag(&mut iter);

        assert!(result.is_err());
        if let Err(ProtobufError::VarintDowncastOutOfRange { value, target_type }) = result {
            assert_eq!(value, 0x100000000);
            assert_eq!(target_type, "u32");
        } else {
            panic!("Expected VarintDowncastOutOfRange error");
        }
    }
}
