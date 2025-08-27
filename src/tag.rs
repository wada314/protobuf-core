//! Tag encoding and decoding for Protocol Buffers.
//!
//! This module provides functions for building and parsing protobuf tags,
//! which combine field numbers with wire types.

use crate::field_number::FieldNumber;
use crate::wire_format::{
    WireType, FIELD_NUMBER_SHIFT, MAX_FIELD_NUMBER, MIN_FIELD_NUMBER, WIRE_TYPE_MASK,
};
use crate::{ProtobufError, Result};
use std::convert::TryFrom;

/// Build a tag from field number and wire type.
///
/// The tag is encoded as: (field_number << 3) | wire_type
pub fn build_tag(field_number: FieldNumber, wire_type: WireType) -> u32 {
    (field_number.get() << FIELD_NUMBER_SHIFT) | (wire_type as u32)
}

/// Parse a tag into field number and wire type.
///
/// Returns an error if the field number or wire type is invalid.
pub fn parse_tag(tag: u32) -> Result<(FieldNumber, WireType)> {
    let field_number_value = tag >> FIELD_NUMBER_SHIFT;
    let wire_type_value = tag & WIRE_TYPE_MASK;

    // Check field number range
    if field_number_value < MIN_FIELD_NUMBER.get() || field_number_value > MAX_FIELD_NUMBER.get() {
        return Err(ProtobufError::MalformedTag {
            field_number: field_number_value,
            wire_type: wire_type_value as u8,
        });
    }

    // Create field number (this should succeed since we already validated the range)
    let field_number =
        FieldNumber::new(field_number_value).map_err(|_| ProtobufError::FieldNumberOutOfRange {
            value: field_number_value,
        })?;

    // Parse wire type
    let wire_type =
        WireType::try_from(wire_type_value as u8).map_err(|_| ProtobufError::InvalidWireType {
            value: wire_type_value as u8,
        })?;

    Ok((field_number, wire_type))
}

/// Read a tag from a byte iterator.
///
/// Returns the field number and wire type.
/// Returns `Ok(None)` if no input is available.
/// Returns `Err(ProtobufError)` if the tag is malformed.
pub fn read_tag<I>(iter: &mut I) -> Result<Option<(FieldNumber, WireType)>>
where
    I: Iterator<Item = u8>,
{
    use crate::variant::read_variant;

    let variant_result = read_variant(iter)?;
    let Some(variant) = variant_result else {
        return Ok(None);
    };
    let tag_value = variant.to_uint32()?;
    let (field_number, wire_type) = parse_tag(tag_value)?;
    Ok(Some((field_number, wire_type)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire_format::WireType;

    #[test]
    fn test_tag_build_and_parse() {
        let field_number = FieldNumber::new(1).unwrap();
        let wire_type = WireType::Varint;
        let tag = build_tag(field_number, wire_type);

        let (parsed_field, parsed_wire_type) = parse_tag(tag).unwrap();
        assert_eq!(parsed_field, field_number);
        assert_eq!(parsed_wire_type, wire_type);
    }

    #[test]
    fn test_parse_tag_invalid_field_number() {
        // Test with field number 0 (invalid)
        let tag = 0; // field_number = 0, wire_type = 0
        let result = parse_tag(tag);
        assert!(result.is_err());

        if let Err(ProtobufError::MalformedTag {
            field_number,
            wire_type,
        }) = result
        {
            assert_eq!(field_number, 0);
            assert_eq!(wire_type, 0);
        } else {
            panic!("Expected MalformedTag error");
        }
    }

    #[test]
    fn test_parse_tag_invalid_wire_type() {
        // Test with invalid wire type 6
        let tag = (1 << 3) | 6; // field_number = 1, wire_type = 6
        let result = parse_tag(tag);
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
        let (field_number, wire_type) = read_tag(&mut iter).unwrap().unwrap();
        assert_eq!(field_number, FieldNumber::new(1).unwrap());
        assert_eq!(wire_type, WireType::Varint);
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
