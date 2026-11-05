//! Tag encoding and decoding for Protocol Buffers.
//!
//! This module provides functions for building and parsing protobuf tags,
//! which combine field numbers with wire types.

use crate::ProtobufError;
use crate::Result;
use crate::field_number::FieldNumber;
#[cfg(doc)]
#[allow(unused_imports)]
use crate::varint::ReadExtVarint;
use crate::varint::Varint;
use crate::wire_format::{FIELD_NUMBER_SHIFT, WIRE_TYPE_MASK, WireType};
use ::std::convert::{From, TryFrom};
use ::std::io::Read;

/// A protobuf tag containing field number and wire type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub field_number: FieldNumber,
    pub wire_type: WireType,
}

impl Tag {
    /// Pack this tag into a [`Varint`] (`(field_number << 3) | wire_type`).
    ///
    /// This is not a byte-stream write; use the write traits to emit wire bytes.
    pub fn to_varint(self) -> Varint {
        let value: u32 =
            (u32::from(self.field_number) << FIELD_NUMBER_SHIFT) | (self.wire_type as u32);
        Varint::from_uint32(value)
    }

    /// Unpack a tag from a [`Varint`] tag value (`(field_number << 3) | wire_type`).
    ///
    /// This is not a byte-stream read; use [`ReadExtTag`] to read wire bytes.
    pub fn try_from_varint(varint: Varint) -> Result<Self> {
        let value = varint.try_to_uint32()?;
        Self::try_from_tag_value(value)
    }

    /// Unpack a tag from its integer value (`(field_number << 3) | wire_type`).
    ///
    /// Same checks as [`try_from_varint()`](Self::try_from_varint). Slice tag
    /// reads use this for 1- and 2-byte tags, which always fit in `u32`, so
    /// they do not build a [`Varint`].
    pub(crate) fn try_from_tag_value(value: u32) -> Result<Self> {
        let field_number_value = value >> FIELD_NUMBER_SHIFT;
        let wire_type_value = value & WIRE_TYPE_MASK;

        let field_number = FieldNumber::try_new(field_number_value).map_err(|value| {
            ProtobufError::FieldNumberOutOfRange {
                value: i64::from(value),
            }
        })?;

        let wire_type = WireType::try_from(wire_type_value as u8)?;

        Ok(Self {
            field_number,
            wire_type,
        })
    }
}

// ============================================================================
// From / Into implementations
// ============================================================================

impl From<Tag> for Varint {
    fn from(tag: Tag) -> Self {
        tag.to_varint()
    }
}

impl TryFrom<Varint> for Tag {
    type Error = ProtobufError;

    fn try_from(varint: Varint) -> Result<Self> {
        Self::try_from_varint(varint)
    }
}

/// Extension trait for reading tags from [`Read`] instances.
///
/// A tag is usually one byte (field numbers 1–15) and otherwise two
/// (16–2047), so this calls
/// [`ReadExtVarint::read_varint_likely_short()`](crate::varint::ReadExtVarint::read_varint_likely_short).
///
/// # Example
/// ```
/// use ::protobuf_core::ReadExtTag;
///
/// let mut data: &[u8] = &[0x08]; // tag 1:0 (field 1, wire type 0)
/// let tag = data.read_tag().unwrap();
/// assert_eq!(tag.field_number.as_u32(), 1);
/// ```
pub trait ReadExtTag {
    /// Read a tag from this reader.
    ///
    /// Empty or truncated input is [`ProtobufError::UnexpectedEof`].
    fn read_tag(&mut self) -> Result<Tag>;
}

impl<R> ReadExtTag for R
where
    R: Read,
{
    #[inline]
    fn read_tag(&mut self) -> Result<Tag> {
        use crate::varint::ReadExtVarint;

        Tag::try_from_varint(self.read_varint_likely_short()?)
    }
}

#[cfg(test)]
mod tests {
    use super::Tag;
    use crate::ProtobufError;
    use crate::field_number::FieldNumber;
    use crate::varint::Varint;
    use crate::wire_format::WireType;

    // ============================================================================
    // Tag construction / parsing
    // ============================================================================

    #[test]
    fn test_tag_struct() {
        let tag = Tag {
            field_number: FieldNumber::try_new(1).unwrap(),
            wire_type: WireType::Varint,
        };
        assert_eq!(tag.field_number.as_u32(), 1);
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[test]
    fn test_tag_build_and_parse() {
        let tag = Tag {
            field_number: FieldNumber::try_new(1).unwrap(),
            wire_type: WireType::Varint,
        };
        let encoded = tag.to_varint();
        assert_eq!(encoded.to_uint64(), 8);
        let parsed = Tag::try_from_varint(encoded).unwrap();
        assert_eq!(parsed, tag);
    }

    #[test]
    fn test_from_varint() {
        let varint = Varint::from_uint64(8);
        let tag = Tag::try_from(varint).unwrap();
        assert_eq!(tag.field_number.as_u32(), 1);
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[test]
    fn test_try_from_varint_invalid() {
        // Field number 0 is invalid.
        let varint = Varint::from_uint64(0);
        assert!(matches!(
            Tag::try_from(varint),
            Err(ProtobufError::FieldNumberOutOfRange { .. })
        ));
    }

    #[test]
    fn test_try_from_varint_invalid_wire_type() {
        // Wire type 6 is invalid (bits 0-2 = 6, field number 1 → 0x0E).
        let varint = Varint::from_uint64(0x0E);
        assert!(matches!(
            Tag::try_from(varint),
            Err(ProtobufError::InvalidWireType { value: 6 })
        ));
    }

    #[test]
    fn test_parse_tag_invalid_field_number() {
        // Field number 0 is invalid.
        let varint = Varint::from_uint32(0);
        let result = Tag::try_from_varint(varint);
        assert!(matches!(
            result,
            Err(ProtobufError::FieldNumberOutOfRange { value: 0 })
        ));
    }

    #[test]
    fn test_parse_tag_invalid_wire_type() {
        let varint = Varint::from_uint64(0x0F); // field 1, wire type 7
        assert!(matches!(
            Tag::try_from_varint(varint),
            Err(ProtobufError::InvalidWireType { value: 7 })
        ));
    }

    #[test]
    fn test_tag_roundtrip_every_wire_type() {
        let cases = [
            (1u32, WireType::Varint, 0x08u64),
            (1, WireType::Int64, 0x09),
            (1, WireType::Len, 0x0A),
            (1, WireType::SGroup, 0x0B),
            (1, WireType::EGroup, 0x0C),
            (1, WireType::Int32, 0x0D),
            (16, WireType::Varint, 128),
            (
                FieldNumber::MAX.as_u32(),
                WireType::Int32,
                u64::from(FieldNumber::MAX.as_u32() << 3) | 5,
            ),
        ];

        for (number, wire_type, encoded_value) in cases {
            let tag = Tag {
                field_number: FieldNumber::try_new(number).unwrap(),
                wire_type,
            };
            let encoded = tag.to_varint();
            assert_eq!(encoded.to_uint64(), encoded_value, "field {number}");
            assert_eq!(Tag::try_from_varint(encoded).unwrap(), tag);
            let via_from = Tag::try_from(Varint::from(tag)).unwrap();
            assert_eq!(via_from, tag);
        }
    }

    // ============================================================================
    // ReadExtTag tests
    // ============================================================================

    #[test]
    fn test_read_ext_tag_trait() {
        use super::ReadExtTag;
        use ::std::io::Cursor;

        let data = vec![0x08]; // tag 1:0 (field 1, wire type 0)
        let mut reader = Cursor::new(data);
        let tag = reader.read_tag().unwrap();
        assert_eq!(tag.field_number, FieldNumber::try_new(1).unwrap());
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[test]
    fn test_read_ext_tag_two_and_three_byte() {
        use super::ReadExtTag;
        use ::std::io::Cursor;

        // Field 16, wire type varint: (16 << 3) = 128.
        let mut reader = Cursor::new(vec![0x80, 0x01]);
        let tag = reader.read_tag().unwrap();
        assert_eq!(tag.field_number.as_u32(), 16);
        assert_eq!(tag.wire_type, WireType::Varint);
        assert_eq!(reader.position(), 2);

        // Field 2048, wire type varint: (2048 << 3) = 16384.
        let mut reader = Cursor::new(vec![0x80, 0x80, 0x01]);
        let tag = reader.read_tag().unwrap();
        assert_eq!(tag.field_number.as_u32(), 2048);
        assert_eq!(tag.wire_type, WireType::Varint);
        assert_eq!(reader.position(), 3);

        // Overlong 2-byte encoding of field 1, wire type varint (canonical 0x08).
        let mut reader = Cursor::new(vec![0x88, 0x00]);
        let tag = reader.read_tag().unwrap();
        assert_eq!(tag.field_number.as_u32(), 1);
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[test]
    fn test_read_ext_tag_empty() {
        use super::ReadExtTag;
        use ::std::io::Cursor;

        let data = vec![];
        let mut reader = Cursor::new(data);
        assert!(matches!(
            reader.read_tag(),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn test_read_ext_tag_u32_overflow() {
        use super::ReadExtTag;
        use ::std::io::Cursor;

        // u32::MAX = 4,294,967,295 (0xFFFFFFFF)
        // Use 0x100000000 (4,294,967,296) which exceeds u32::MAX
        let bytes = vec![0x80, 0x80, 0x80, 0x80, 0x10]; // Value: 0x100000000
        let mut reader = Cursor::new(bytes);
        let result = reader.read_tag();

        assert!(result.is_err());
        if let Err(ProtobufError::VarintDowncastOutOfRange { value, target_type }) = result {
            assert_eq!(value, 0x100000000);
            assert_eq!(target_type, "u32");
        } else {
            panic!("Expected VarintDowncastOutOfRange error");
        }
    }

    #[test]
    fn test_read_ext_tag_truncated() {
        use super::ReadExtTag;
        use ::std::io::{Cursor, Read};

        let mut reader = Cursor::new(vec![0x80u8, 0x01]);
        let result = reader.by_ref().take(1).read_tag();
        assert!(matches!(result, Err(ProtobufError::UnexpectedEof)));
        assert_eq!(reader.position(), 1);
    }
}
