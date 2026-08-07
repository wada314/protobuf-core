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

//! Tag encoding and decoding for Protocol Buffers.
//!
//! This module provides functions for building and parsing protobuf tags,
//! which combine field numbers with wire types.

use crate::ProtobufError;
use crate::Result;
use crate::field_number::FieldNumber;
use crate::varint::{DecodeState, Varint};
use crate::wire_format::{FIELD_NUMBER_SHIFT, WIRE_TYPE_MASK, WireType};
use ::std::convert::{From, Into, TryFrom};
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
        let field_number = FieldNumber::try_new(field_number_value)?;

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
/// Returned by [`IteratorExtTag::read_tag_partial`], [`ReadExtTag::read_tag_partial`],
/// and their `read_tag_resume` variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Parsing completed successfully.
    Complete(Tag),
    /// No bytes were read (empty input).
    Empty,
    /// Parser ran out of input. Use [`read_tag_resume`] with the state
    /// to continue from another reader/iterator/stream.
    ///
    /// [`read_tag_resume`]: ReadExtTag::read_tag_resume
    Incomplete(DecodeState),
}

/// Extension trait for reading tag from byte iterators.
///
/// This trait provides a convenient method to read tag directly from
/// any iterator that yields bytes.
///
/// # Example
/// ```
/// use ::protobuf_core::IteratorExtTag;
///
/// let bytes = vec![0x08]; // tag 1:0 (field 1, wire type 0)
/// let mut iter = bytes.into_iter();
/// let tag = iter.read_tag().unwrap().unwrap();
/// assert_eq!(tag.field_number.as_u32(), 1);
/// ```
pub trait IteratorExtTag {
    /// Read a tag from this iterator.
    fn read_tag(&mut self) -> Result<Option<Tag>>;

    /// Read a tag from this iterator, supporting incomplete input.
    fn read_tag_partial(&mut self) -> Result<Outcome>;

    /// Resume tag decoding with additional bytes from this iterator.
    fn read_tag_resume(&mut self, state: DecodeState) -> Result<Outcome>;
}

impl<I> IteratorExtTag for I
where
    I: Iterator<Item = u8>,
{
    fn read_tag(&mut self) -> Result<Option<Tag>> {
        use crate::varint::{DecodeOutcome, IteratorExtVarint};

        match self.read_varint_partial()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Some),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(_) => Err(ProtobufError::UnexpectedEof),
        }
    }

    fn read_tag_partial(&mut self) -> Result<Outcome> {
        use crate::varint::{DecodeOutcome, IteratorExtVarint};

        match self.read_varint_partial()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Outcome::Complete),
            DecodeOutcome::Empty => Ok(Outcome::Empty),
            DecodeOutcome::Incomplete(s) => Ok(Outcome::Incomplete(s)),
        }
    }

    fn read_tag_resume(&mut self, state: DecodeState) -> Result<Outcome> {
        use crate::varint::{DecodeOutcome, IteratorExtVarint};

        match self.read_varint_resume(state)? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Outcome::Complete),
            DecodeOutcome::Empty => Ok(Outcome::Empty),
            DecodeOutcome::Incomplete(s) => Ok(Outcome::Incomplete(s)),
        }
    }
}

/// Extension trait for reading tag from byte iterators that yield `Result<u8, E>`.
///
/// This trait provides a convenient method to read tag directly from
/// any iterator that yields `Result<u8, E>`, allowing proper error propagation
/// from I/O operations.
///
/// # Example
/// ```
/// use ::std::io::{Cursor, Read};
/// use ::protobuf_core::TryIteratorExtTag;
///
/// let data = vec![0x08]; // tag 1:0 (field 1, wire type 0)
/// let mut reader = Cursor::new(data);
/// let mut iter = reader.bytes(); // Iterator<Item = Result<u8, io::Error>>
/// let tag = iter.read_tag().unwrap().unwrap();
/// assert_eq!(tag.field_number.as_u32(), 1);
/// ```
pub trait TryIteratorExtTag {
    /// Read a tag from this iterator.
    fn read_tag(&mut self) -> Result<Option<Tag>>;

    /// Read a tag from this iterator, supporting incomplete input.
    fn read_tag_partial(&mut self) -> Result<Outcome>;

    /// Resume tag decoding with additional bytes from this iterator.
    fn read_tag_resume(&mut self, state: DecodeState) -> Result<Outcome>;
}

impl<I, E> TryIteratorExtTag for I
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn read_tag(&mut self) -> Result<Option<Tag>> {
        use crate::varint::{DecodeOutcome, TryIteratorExtVarint};

        match self.read_varint_partial()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Some),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(_) => Err(ProtobufError::UnexpectedEof),
        }
    }

    fn read_tag_partial(&mut self) -> Result<Outcome> {
        use crate::varint::{DecodeOutcome, TryIteratorExtVarint};

        match self.read_varint_partial()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Outcome::Complete),
            DecodeOutcome::Empty => Ok(Outcome::Empty),
            DecodeOutcome::Incomplete(s) => Ok(Outcome::Incomplete(s)),
        }
    }

    fn read_tag_resume(&mut self, state: DecodeState) -> Result<Outcome> {
        use crate::varint::{DecodeOutcome, TryIteratorExtVarint};

        match self.read_varint_resume(state)? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Outcome::Complete),
            DecodeOutcome::Empty => Ok(Outcome::Empty),
            DecodeOutcome::Incomplete(s) => Ok(Outcome::Incomplete(s)),
        }
    }
}

/// Extension trait for reading tags from Read instances.
///
/// This trait provides a convenient method to read tags directly from
/// any type that implements `std::io::Read`.
///
/// # Example
/// ```
/// use ::std::io::Cursor;
/// use ::protobuf_core::ReadExtTag;
///
/// let data = vec![0x08]; // tag 1:0 (field 1, wire type 0)
/// let mut reader = Cursor::new(data);
/// let tag = reader.read_tag().unwrap().unwrap();
/// assert_eq!(tag.field_number.as_u32(), 1);
/// ```
pub trait ReadExtTag {
    /// Read a tag from this reader.
    fn read_tag(&mut self) -> Result<Option<Tag>>;

    /// Read a tag from this reader, supporting incomplete input.
    fn read_tag_partial(&mut self) -> Result<Outcome>;

    /// Resume tag decoding with additional bytes from this reader.
    fn read_tag_resume(&mut self, state: DecodeState) -> Result<Outcome>;
}

impl<R> ReadExtTag for R
where
    R: Read,
{
    fn read_tag(&mut self) -> Result<Option<Tag>> {
        use crate::varint::{DecodeOutcome, ReadExtVarint};

        match self.read_varint_partial()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Some),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(_) => Err(ProtobufError::UnexpectedEof),
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
    // IteratorExtTag tests
    // ============================================================================

    #[test]
    fn test_iterator_ext_tag_trait() {
        use super::IteratorExtTag;

        let bytes = vec![0x08]; // tag 1:0 (field 1, wire type 0)
        let mut iter = bytes.into_iter();
        let tag = iter.read_tag().unwrap().unwrap();
        assert_eq!(tag.field_number, FieldNumber::try_new(1).unwrap());
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[test]
    fn test_iterator_ext_tag_empty() {
        use super::IteratorExtTag;

        let tag = IteratorExtTag::read_tag(&mut ::std::iter::empty()).unwrap();
        assert_eq!(tag, None);
    }

    #[test]
    fn test_iterator_ext_tag_partial_complete() {
        use super::{IteratorExtTag, Outcome};

        let bytes = vec![0x08]; // tag 1:0 (field 1, wire type 0)
        let mut iter = bytes.into_iter();
        let outcome = iter.read_tag_partial().unwrap();
        assert!(matches!(outcome, Outcome::Complete(t) if t.field_number.as_u32() == 1));
    }

    #[test]
    fn test_iterator_ext_tag_partial_empty() {
        use super::{IteratorExtTag, Outcome};

        let outcome = IteratorExtTag::read_tag_partial(&mut ::std::iter::empty()).unwrap();
        assert!(matches!(outcome, Outcome::Empty));
    }

    #[test]
    fn test_iterator_ext_tag_partial_resume() {
        use super::{IteratorExtTag, Outcome};

        // Split tag 0x08 across two chunks: first byte is continuation (0x80), second completes
        // Actually 0x08 is a complete 1-byte tag. For incomplete, we need a multi-byte varint.
        // Tag (field 1, wire 0) = 8 = 0x08 - single byte.
        // For a 2-byte varint: 0x80 0x01 = 128 (field 16, wire 0).
        let first_chunk: Vec<u8> = vec![0x80]; // incomplete - continuation bit set
        let second_chunk: Vec<u8> = vec![0x01]; // completes to 128

        let mut iter = first_chunk.into_iter();
        let outcome = iter.read_tag_partial().unwrap();
        let state = match outcome {
            Outcome::Incomplete(s) => s,
            _ => panic!("expected Incomplete, got {:?}", outcome),
        };

        let mut iter2 = second_chunk.into_iter();
        let outcome2 = iter2.read_tag_resume(state).unwrap();
        match outcome2 {
            Outcome::Complete(tag) => {
                assert_eq!(tag.field_number.as_u32(), 16);
                assert_eq!(tag.wire_type, WireType::Varint);
            }
            _ => panic!("expected Complete, got {:?}", outcome2),
        }
    }

    #[test]
    fn test_iterator_ext_tag_u32_overflow() {
        use super::IteratorExtTag;

        // Test case where the varint value exceeds u32::MAX
        // This should trigger VarintDowncastOutOfRange error
        // u32::MAX = 4,294,967,295 (0xFFFFFFFF)
        // Use 0x100000000 (4,294,967,296) which exceeds u32::MAX
        let bytes = vec![0x80, 0x80, 0x80, 0x80, 0x10]; // Value: 0x100000000 (exceeds u32::MAX)
        let mut iter = bytes.into_iter();
        let result = iter.read_tag();

        assert!(result.is_err());
        if let Err(ProtobufError::VarintDowncastOutOfRange { value, target_type }) = result {
            assert_eq!(value, 0x100000000);
            assert_eq!(target_type, "u32");
        } else {
            panic!("Expected VarintDowncastOutOfRange error");
        }
    }

    // ============================================================================
    // TryIteratorExtTag tests
    // ============================================================================

    #[test]
    fn test_try_iterator_ext_tag() {
        use super::TryIteratorExtTag;
        use ::std::io::{Cursor, Read};

        let data = vec![0x08]; // tag 1:0 (field 1, wire type 0)
        let reader = Cursor::new(data);
        let mut iter = reader.bytes();
        let tag = iter.read_tag().unwrap().unwrap();
        assert_eq!(tag.field_number, FieldNumber::try_new(1).unwrap());
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[test]
    fn test_try_iterator_ext_tag_empty() {
        use super::TryIteratorExtTag;
        use ::std::io::{Cursor, Read};

        let data = vec![];
        let reader = Cursor::new(data);
        let mut iter = reader.bytes();
        let tag = iter.read_tag().unwrap();
        assert_eq!(tag, None);
    }

    #[test]
    fn test_try_iterator_ext_tag_error() {
        use super::TryIteratorExtTag;
        use ::std::io::ErrorKind;

        // Create an iterator that returns an error
        let error = ::std::io::Error::new(ErrorKind::UnexpectedEof, "test error");
        let mut iter = ::std::iter::once(Err(error));
        let result = iter.read_tag();

        assert!(result.is_err());
        if let Err(ProtobufError::IoError(io_err)) = result {
            assert_eq!(io_err.kind(), ErrorKind::UnexpectedEof);
        } else {
            panic!("Expected IoError");
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
        let tag = reader.read_tag().unwrap().unwrap();
        assert_eq!(tag.field_number, FieldNumber::try_new(1).unwrap());
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[test]
    fn test_read_ext_tag_empty() {
        use super::ReadExtTag;
        use ::std::io::Cursor;

        let data = vec![];
        let mut reader = Cursor::new(data);
        let tag = reader.read_tag().unwrap();
        assert_eq!(tag, None);
    }
}
