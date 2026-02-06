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
use crate::varint::Varint;
use crate::wire_format::{FIELD_NUMBER_SHIFT, WIRE_TYPE_MASK, WireType};
use ::std::convert::{From, Into, TryFrom};
use ::std::io::Read;

#[cfg(feature = "futures")]
use crate::varint::StreamExtVarint;
#[cfg(feature = "futures")]
use ::futures_util::stream::TryStream;
#[cfg(feature = "futures")]
use ::std::future::Future;
#[cfg(feature = "futures")]
use ::std::pin::Pin;

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
    ///
    /// Returns the Tag `Ok(Some(tag))` if successfully read.
    /// Returns `Ok(None)` if no input is available (empty iterator).
    /// Returns `Err(ProtobufError)` if the tag is malformed.
    fn read_tag(&mut self) -> Result<Option<Tag>>;
}

impl<I> IteratorExtTag for I
where
    I: Iterator<Item = u8>,
{
    fn read_tag(&mut self) -> Result<Option<Tag>> {
        use crate::varint::{DecodeOutcome, IteratorExtVarint};

        match self.read_varint()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Some),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(s) => Err(ProtobufError::IncompleteVarint(s)),
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
    ///
    /// Returns the Tag `Ok(Some(tag))` if successfully read.
    /// Returns `Ok(None)` if no input is available (empty iterator).
    /// Returns `Err(ProtobufError)` if the tag is malformed or an I/O error occurs.
    fn read_tag(&mut self) -> Result<Option<Tag>>;
}

impl<I, E> TryIteratorExtTag for I
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn read_tag(&mut self) -> Result<Option<Tag>> {
        use crate::varint::{DecodeOutcome, TryIteratorExtVarint};

        match self.read_varint()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Some),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(s) => Err(ProtobufError::IncompleteVarint(s)),
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
        use crate::varint::{DecodeOutcome, ReadExtVarint};

        match self.read_varint()? {
            DecodeOutcome::Complete(varint) => Tag::from_encoded(varint).map(Some),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(s) => Err(ProtobufError::IncompleteVarint(s)),
        }
    }
}

// ============================================================================
// Async Stream extension (TryStream)
// ============================================================================

/// Extension trait for reading tags from async byte streams.
///
/// This trait provides `read_tag` for types implementing
/// `TryStream<Ok = u8, Error = E>` where `E: Into<ProtobufError>`.
/// It reads a varint and parses it as a protobuf tag.
///
/// Uses `Pin<&mut Self>` so that both `Unpin` and `!Unpin` streams are supported.
///
/// # Example
///
/// ```ignore
/// use futures_util::stream;
/// use protobuf_core::{StreamExtTag, Tag};
/// use std::pin::Pin;
///
/// let bytes = vec![Ok(0x08u8)]; // tag 1:0 (field 1, wire type 0)
/// let mut stream = stream::iter(bytes);
/// let tag = Pin::new(&mut stream).read_tag().await?.expect("one tag");
/// assert_eq!(tag.field_number.as_u32(), 1);
/// ```
#[cfg(feature = "futures")]
pub trait StreamExtTag: TryStream<Ok = u8> {
    /// Read the next tag from the stream.
    ///
    /// - `Ok(Some(tag))` when a tag is successfully decoded.
    /// - `Ok(None)` on EOF (no bytes read).
    /// - `Err(...)` on parse or I/O error.
    fn read_tag(self: Pin<&mut Self>) -> impl Future<Output = Result<Option<Tag>>> + Send
    where
        Self::Error: Into<ProtobufError>;
}

#[cfg(feature = "futures")]
impl<S, E> StreamExtTag for S
where
    S: TryStream<Ok = u8, Error = E> + Send,
    E: Into<ProtobufError>,
{
    fn read_tag(self: Pin<&mut Self>) -> impl Future<Output = Result<Option<Tag>>> + Send {
        use crate::varint::DecodeOutcome;

        async move {
            let outcome = StreamExtVarint::read_varint(self).await?;
            match outcome {
                DecodeOutcome::Complete(v) => Tag::from_encoded(v).map(Some),
                DecodeOutcome::Empty => Ok(None),
                DecodeOutcome::Incomplete(s) => Err(ProtobufError::IncompleteVarint(s)),
            }
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
            assert_eq!(value, "0");
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

    // ============================================================================
    // StreamExtTag tests (requires futures feature)
    // ============================================================================

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_tag_complete() {
        use super::StreamExtTag;
        use ::futures_executor::block_on;
        use ::futures_util::stream;
        use ::std::convert::Infallible;
        use ::std::pin::Pin;

        let bytes: Vec<Result<u8, Infallible>> = vec![Ok(0x08)]; // tag 1:0 (field 1, wire type 0)
        let mut stream = stream::iter(bytes);

        let result = block_on(async { Pin::new(&mut stream).read_tag().await });
        assert!(result.is_ok());
        let tag = result.unwrap().unwrap();
        assert_eq!(tag.field_number, FieldNumber::try_new(1).unwrap());
        assert_eq!(tag.wire_type, WireType::Varint);
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_tag_eof() {
        use super::StreamExtTag;
        use ::futures_executor::block_on;
        use ::futures_util::stream;
        use ::std::convert::Infallible;
        use ::std::pin::Pin;

        let mut stream = stream::empty::<Result<u8, Infallible>>();

        let result = block_on(async { Pin::new(&mut stream).read_tag().await });
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_tag_invalid_wire_type() {
        use super::StreamExtTag;
        use ::futures_executor::block_on;
        use ::futures_util::stream;
        use ::std::convert::Infallible;
        use ::std::pin::Pin;

        // tag field 1 wire type 6 (invalid)
        let bytes: Vec<Result<u8, Infallible>> = vec![Ok(0x0E)]; // (1 << 3) | 6 = 14 = 0x0E
        let mut stream = stream::iter(bytes);

        let result = block_on(async { Pin::new(&mut stream).read_tag().await });
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ProtobufError::InvalidWireType { value: 6 })
        ));
    }
}
