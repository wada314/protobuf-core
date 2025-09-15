//! Tag encoding and decoding for Protocol Buffers.
//!
//! This module provides functions for building and parsing protobuf tags,
//! which combine field numbers with wire types.

use crate::field_number::FieldNumber;
use crate::wire_format::{WireType, FIELD_NUMBER_SHIFT, WIRE_TYPE_MASK};
use crate::Result;
use ::std::convert::TryFrom;

/// A protobuf tag containing field number and wire type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub field_number: FieldNumber,
    pub wire_type: WireType,
}

impl Tag {
    /// Build the encoded tag value
    pub fn to_encoded(self) -> u32 {
        (Into::<u32>::into(self.field_number) << FIELD_NUMBER_SHIFT) | (self.wire_type as u32)
    }

    /// Parse a tag from an encoded value
    pub fn from_encoded(encoded: u32) -> Result<Self> {
        let field_number_value = encoded >> FIELD_NUMBER_SHIFT;
        let wire_type_value = encoded & WIRE_TYPE_MASK;

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
    use crate::variant::IteratorExtVariant;

    let variant_result = iter.try_collect_variant()?;
    let Some(variant) = variant_result else {
        return Ok(None);
    };
    let tag_value = variant.to_uint32()?;
    let tag = Tag::from_encoded(tag_value)?;
    Ok(Some(tag))
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
        let tag = 0; // field_number = 0, wire_type = 0
        let result = Tag::from_encoded(tag);
        assert!(result.is_err());

        if let Err(ProtobufError::FieldNumberOutOfRange { value }) = result {
            assert_eq!(value, 0);
        } else {
            panic!("Expected FieldNumberOutOfRange error");
        }
    }

    #[test]
    fn test_parse_tag_invalid_wire_type() {
        // Test with invalid wire type 6
        let tag = (1 << 3) | 6; // field_number = 1, wire_type = 6
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
        // This should trigger VariantDowncastOutOfRange error
        // u32::MAX = 4,294,967,295 (0xFFFFFFFF)
        // Use 0x100000000 (4,294,967,296) which exceeds u32::MAX
        let bytes = vec![0x80, 0x80, 0x80, 0x80, 0x10]; // Value: 0x100000000 (exceeds u32::MAX)
        let mut iter = bytes.into_iter();
        let result = read_tag(&mut iter);

        assert!(result.is_err());
        if let Err(ProtobufError::VariantDowncastOutOfRange { value, target_type }) = result {
            assert_eq!(value, 0x100000000);
            assert_eq!(target_type, "u32");
        } else {
            panic!("Expected VariantDowncastOutOfRange error");
        }
    }
}
