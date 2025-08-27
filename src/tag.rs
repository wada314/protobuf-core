//! Tag encoding and decoding for Protocol Buffers.
//!
//! This module provides functions for building and parsing protobuf tags,
//! which combine field numbers with wire types.

use crate::wire_format::{
    WireType, FIELD_NUMBER_SHIFT, MAX_FIELD_NUMBER, MIN_FIELD_NUMBER, WIRE_TYPE_MASK,
};
use std::convert::TryFrom;

/// Build a tag from field number and wire type.
///
/// The tag is encoded as: (field_number << 3) | wire_type
pub fn build_tag(field_number: u32, wire_type: WireType) -> u32 {
    (field_number << FIELD_NUMBER_SHIFT) | (wire_type as u32)
}

/// Parse a tag into field number and wire type.
///
/// Returns None if the wire type is invalid.
pub fn parse_tag(tag: u32) -> Option<(u32, WireType)> {
    let field_number = tag >> FIELD_NUMBER_SHIFT;
    let wire_type_value = tag & WIRE_TYPE_MASK;

    if field_number < MIN_FIELD_NUMBER || field_number > MAX_FIELD_NUMBER {
        return None;
    }

    let wire_type = WireType::try_from(wire_type_value as u8).ok()?;
    Some((field_number, wire_type))
}

/// Read a tag from a byte iterator.
///
/// Returns the field number and wire type.
/// Returns `Ok(None)` if no input is available.
/// Returns `Err(VariantError)` if the tag is malformed.
pub fn read_tag<I>(iter: &mut I) -> Result<Option<(u32, WireType)>, crate::variant::VariantError>
where
    I: Iterator<Item = u8>,
{
    use crate::variant::read_variant;

    let variant_result = read_variant(iter)?;
    let Some(variant) = variant_result else {
        return Ok(None);
    };
    let tag_value = variant.to_uint64() as u32;
    let (field_number, wire_type) = parse_tag(tag_value).ok_or(
        crate::variant::VariantError::ValueOutOfRange(tag_value as u64),
    )?;
    Ok(Some((field_number, wire_type)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire_format::WireType;

    #[test]
    fn test_tag_build_and_parse() {
        let field_number = 1;
        let wire_type = WireType::Varint;
        let tag = build_tag(field_number, wire_type);

        let (parsed_field, parsed_wire_type) = parse_tag(tag).unwrap();
        assert_eq!(parsed_field, field_number);
        assert_eq!(parsed_wire_type, wire_type);
    }

    #[test]
    fn test_read_tag() {
        let bytes = vec![0x08]; // tag 1:0 (field 1, wire type 0)
        let mut iter = bytes.into_iter();
        let (field_number, wire_type) = read_tag(&mut iter).unwrap().unwrap();
        assert_eq!(field_number, 1);
        assert_eq!(wire_type, WireType::Varint);
    }
}
