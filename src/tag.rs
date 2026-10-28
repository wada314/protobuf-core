//! Tag encoding and decoding for Protocol Buffers.
//!
//! This module provides functions for building and parsing protobuf tags,
//! which combine field numbers with wire types.

use crate::ProtobufError;
use crate::Result;
use crate::field_number::FieldNumber;
use crate::varint::{DecodeState, Varint};
use crate::wire_format::{FIELD_NUMBER_SHIFT, WIRE_TYPE_MASK, WireType};
use ::std::convert::{From, TryFrom};
use ::std::io::Read;

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
        let field_number = FieldNumber::try_new(field_number_value).map_err(|value| {
            ProtobufError::FieldNumberOutOfRange {
                value: i64::from(value),
            }
        })?;

        // Parse wire type
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
        tag.to_encoded()
    }
}

impl TryFrom<Varint> for Tag {
    type Error = ProtobufError;

    fn try_from(encoded: Varint) -> Result<Self> {
        Self::from_encoded(encoded)
    }
}

/// Result of a tag read step.
///
/// Returned by [`ReadExtTag::read_tag_partial`] and
/// [`ReadExtTag::read_tag_resume`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Parsing completed successfully.
    Complete(Tag),
    /// No bytes were read (empty input).
    Empty,
    /// Parser ran out of input. Use [`read_tag_resume`] with the state
    /// to continue from another reader/stream.
    ///
    /// [`read_tag_resume`]: ReadExtTag::read_tag_resume
    Incomplete(DecodeState),
}

/// Extension trait for reading tags from Read instances.
///
/// Reads until the reader's EOF (or an existing [`std::io::Take`] bound).
/// Methods do not take a separate byte-budget argument; they forward to
/// [`ReadExtVarint`].
///
/// [`ReadExtVarint`]: crate::varint::ReadExtVarint
///
/// # Example
/// ```
/// use ::std::io::Cursor;
/// use ::protobuf_core::ReadExtTag;
///
/// let data = vec![0x08]; // tag 1:0 (field 1, wire type 0)
/// let mut reader = Cursor::new(data);
/// let tag = reader.read_tag().unwrap();
/// assert_eq!(tag.field_number.as_u32(), 1);
/// ```
pub trait ReadExtTag {
    /// Read a tag from this reader until a complete value.
    ///
    /// Empty or truncated input is [`ProtobufError::UnexpectedEof`]; check a
    /// length bound first for soft end.
    fn read_tag(&mut self) -> Result<Tag>;

    /// Read a tag from this reader, supporting incomplete input.
    fn read_tag_partial(&mut self) -> Result<Outcome>;

    /// Resume tag decoding with additional bytes from this reader.
    fn read_tag_resume(&mut self, state: DecodeState) -> Result<Outcome>;
}

impl<R> ReadExtTag for R
where
    R: Read,
{
    fn read_tag(&mut self) -> Result<Tag> {
        use crate::varint::{DecodeOutcome, ReadExtVarint};

        match self.read_varint_partial()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint),
            DecodeOutcome::Empty | DecodeOutcome::Incomplete(_) => {
                Err(ProtobufError::UnexpectedEof)
            }
        }
    }

    fn read_tag_partial(&mut self) -> Result<Outcome> {
        use crate::varint::{DecodeOutcome, ReadExtVarint};

        match self.read_varint_partial()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Outcome::Complete),
            DecodeOutcome::Empty => Ok(Outcome::Empty),
            DecodeOutcome::Incomplete(s) => Ok(Outcome::Incomplete(s)),
        }
    }

    fn read_tag_resume(&mut self, state: DecodeState) -> Result<Outcome> {
        use crate::varint::{DecodeOutcome, ReadExtVarint};

        match self.read_varint_resume(state)? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Outcome::Complete),
            DecodeOutcome::Empty => Ok(Outcome::Empty),
            DecodeOutcome::Incomplete(s) => Ok(Outcome::Incomplete(s)),
        }
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
    // Basic Tag tests (no traits)
    // ============================================================================

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
    fn test_parse_tag_invalid_field_number() {
        // Test with field number 0 (invalid)
        let tag = Varint::from_uint32(0); // field_number = 0, wire_type = 0
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
        let tag = Varint::from_uint32((1 << 3) | 6); // field_number = 1, wire_type = 6
        let result = Tag::from_encoded(tag);
        assert!(result.is_err());

        if let Err(ProtobufError::InvalidWireType { value }) = result {
            assert_eq!(value, 6);
        } else {
            panic!("Expected InvalidWireType error");
        }
    }

    // ============================================================================
    // From / Into / TryFrom tests
    // ============================================================================

    #[test]
    fn test_from_varint() {
        use ::std::convert::{Into, TryFrom};

        let field_number = FieldNumber::try_new(5).unwrap();
        let wire_type = WireType::Varint;
        let tag = Tag {
            field_number,
            wire_type,
        };
        let varint: Varint = tag.into();
        let decoded_tag = Tag::try_from(varint).unwrap();
        assert_eq!(decoded_tag, tag);
    }

    #[test]
    fn test_try_from_varint_invalid() {
        use ::std::convert::TryFrom;

        // Test with invalid field number 0
        let varint = Varint::from_uint32(0);
        let result = Tag::try_from(varint);
        assert!(result.is_err());

        if let Err(ProtobufError::FieldNumberOutOfRange { value }) = result {
            assert_eq!(value, 0);
        } else {
            panic!("Expected FieldNumberOutOfRange error");
        }
    }

    #[test]
    fn test_try_from_varint_invalid_wire_type() {
        use ::std::convert::TryFrom;

        // Test with invalid wire type
        let varint = Varint::from_uint32((1 << 3) | 6);
        let result = Tag::try_from(varint);
        assert!(result.is_err());

        if let Err(ProtobufError::InvalidWireType { value }) = result {
            assert_eq!(value, 6);
        } else {
            panic!("Expected InvalidWireType error");
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
            let encoded = tag.to_encoded();
            assert_eq!(encoded.to_uint64(), encoded_value, "field {number}");
            assert_eq!(Tag::from_encoded(encoded).unwrap(), tag);
            let via_from = Tag::try_from(Varint::from(tag)).unwrap();
            assert_eq!(via_from, tag);
        }
    }

    #[test]
    fn test_read_ext_tag_partial_resume() {
        use super::{Outcome, ReadExtTag};
        use ::std::io::Cursor;

        let mut reader = Cursor::new(vec![0x80u8]);
        let state = match reader.read_tag_partial().unwrap() {
            Outcome::Incomplete(state) => state,
            other => panic!("expected Incomplete, got {other:?}"),
        };
        let mut reader = Cursor::new(vec![0x01u8]);
        match reader.read_tag_resume(state).unwrap() {
            Outcome::Complete(tag) => {
                assert_eq!(tag.field_number.as_u32(), 16);
                assert_eq!(tag.wire_type, WireType::Varint);
            }
            other => panic!("expected Complete, got {other:?}"),
        }
    }

    #[test]
    fn test_read_ext_tag_short_reader_incomplete() {
        use super::{Outcome, ReadExtTag};
        use ::std::io::{Cursor, Read};

        // 2-byte tag encoding; Take of 1 yields Incomplete.
        let mut reader = Cursor::new(vec![0x80u8, 0x01]);
        let outcome = reader.by_ref().take(1).read_tag_partial().unwrap();
        assert!(matches!(outcome, Outcome::Incomplete(_)));
        assert_eq!(reader.position(), 1);
    }
}
