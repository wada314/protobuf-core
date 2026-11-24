//! Protocol Buffers wire format constants and definitions.
//!
//! This module provides the fundamental constants and types needed for
//! implementing Protocol Buffers encoding and decoding according to the
//! official wire format specification.

use crate::ProtobufError;
use crate::field_number::FieldNumber;
use ::std::convert::TryFrom;

/// Wire types used in Protocol Buffers encoding.
///
/// The wire type tells how large the following payload is and how to read it.
/// Unknown wire-type values (outside 0–5) are an error in this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WireType {
    /// Variable-width integers (Int32, Int64, `UInt32`, `UInt64`, `SInt32`, `SInt64`, Bool, Enum).
    Varint = 0,
    /// 64-bit fixed-width values (Fixed64, `SFixed64`, Double).
    Int64 = 1,
    /// Length-delimited values (String, Bytes, embedded messages, packed repeated fields).
    Len = 2,
    /// Start group. Groups are deprecated in the Protocol Buffers language
    /// since proto2 (prefer nested messages). The group body follows on the
    /// wire; discard it with `skip_until()` + `until_group_ends()`, preserve
    /// with `take_until()` (slice) or `copy_until()` (Read) and the same
    /// predicate, or read nested fields until the matching end-group.
    SGroup = 3,
    /// End group. Groups are deprecated in the Protocol Buffers language since
    /// proto2 (prefer nested messages).
    EGroup = 4,
    /// 32-bit fixed-width values (Fixed32, `SFixed32`, Float).
    Int32 = 5,
}

impl From<WireType> for u8 {
    #[inline]
    fn from(wire_type: WireType) -> Self {
        wire_type as u8
    }
}

impl TryFrom<u8> for WireType {
    type Error = crate::ProtobufError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WireType::Varint),
            1 => Ok(WireType::Int64),
            2 => Ok(WireType::Len),
            3 => Ok(WireType::SGroup),
            4 => Ok(WireType::EGroup),
            5 => Ok(WireType::Int32),
            _ => Err(ProtobufError::InvalidWireType { value }),
        }
    }
}

/// Maximum field number allowed in Protocol Buffers.
///
/// Field numbers must be in the range [1, 2^29 - 1].
pub const MAX_FIELD_NUMBER: FieldNumber = FieldNumber::MAX;

/// Minimum field number allowed in Protocol Buffers.
pub const MIN_FIELD_NUMBER: FieldNumber = FieldNumber::MIN;

/// Maximum message size when serialized (2 GiB).
///
/// [`crate::ReadExtProtobuf::read_protobuf_fields()`] and
/// [`crate::AsRefExtProtobuf::read_protobuf_fields()`] reject a larger budget
/// or slice with [`ProtobufError::SizeLimitExceeded`]. The check runs on the
/// first read (`next`, `skip_until`, `copy_until`), and that read consumes no
/// bytes. A length equal to this limit is allowed.
pub const MAX_MESSAGE_SIZE: usize = 2 * 1024 * 1024 * 1024;

/// Maximum length-delimited payload size (2 GiB).
///
/// Applies to every wire-type `Len` length prefix. This crate does not
/// distinguish string, bytes, embedded message, and packed repeated. A larger
/// decoded length is [`ProtobufError::SizeLimitExceeded`] after the tag and
/// the length varint are consumed. A prefix above `u32::MAX` is
/// [`ProtobufError::VarintDowncastOutOfRange`] instead. A prefix within this
/// limit whose payload is missing is [`ProtobufError::UnexpectedEof`].
/// A length equal to this limit is allowed.
pub const MAX_STRING_SIZE: usize = 2 * 1024 * 1024 * 1024;

/// Reject a top-level message budget or slice longer than [`MAX_MESSAGE_SIZE`].
#[inline]
pub(crate) fn ensure_message_size(len: usize) -> crate::Result<()> {
    if len > MAX_MESSAGE_SIZE {
        Err(ProtobufError::SizeLimitExceeded {
            value: u64::try_from(len).unwrap_or(u64::MAX),
            limit: MAX_MESSAGE_SIZE,
        })
    } else {
        Ok(())
    }
}

/// Reject a wire-type `Len` payload longer than [`MAX_STRING_SIZE`].
///
/// Call after `Varint::try_to_uint32()` so a prefix above `u32::MAX` stays
/// [`ProtobufError::VarintDowncastOutOfRange`].
#[inline]
pub(crate) fn ensure_len_payload_size(len: u32) -> crate::Result<u32> {
    // `u32` lengths fit in `usize` on every target that can hold `MAX_STRING_SIZE` (2 GiB).
    #[allow(clippy::cast_possible_truncation)]
    let len_usize = len as usize;
    if len_usize > MAX_STRING_SIZE {
        Err(ProtobufError::SizeLimitExceeded {
            value: u64::from(len),
            limit: MAX_STRING_SIZE,
        })
    } else {
        Ok(len)
    }
}

/// Bit mask for extracting the wire type from a tag.
///
/// The wire type is stored in the least significant 3 bits of the tag.
pub const WIRE_TYPE_MASK: u32 = 0b111;

/// Bit shift for extracting the field number from a tag.
///
/// The field number is stored in the upper bits of the tag.
pub const FIELD_NUMBER_SHIFT: u32 = 3;

/// Maximum varint size in bytes.
///
/// A varint can use anywhere between 1 and 10 bytes.
pub const MAX_VARINT_BYTES: usize = 10;

/// Maximum variable-length integer value that can be encoded in 9 bytes.
///
/// This is used for optimization in variable-length integer encoding/decoding.
pub const MAX_9_BYTE_VARINT: u64 = 0x7FFF_FFFF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 8 bytes.
pub const MAX_8_BYTE_VARINT: u64 = 0xFF_FFFF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 7 bytes.
pub const MAX_7_BYTE_VARINT: u64 = 0x0001_FFFF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 6 bytes.
pub const MAX_6_BYTE_VARINT: u64 = 0x3FF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 5 bytes.
pub const MAX_5_BYTE_VARINT: u64 = 0x7_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 4 bytes.
pub const MAX_4_BYTE_VARINT: u64 = 0xFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 3 bytes.
pub const MAX_3_BYTE_VARINT: u64 = 0x1F_FFFF;

/// Maximum variable-length integer value that can be encoded in 2 bytes.
pub const MAX_2_BYTE_VARINT: u64 = 0x3FFF;

/// Maximum variable-length integer value that can be encoded in 1 byte.
pub const MAX_1_BYTE_VARINT: u64 = 0x7F;

/// Continuation bit mask for varint encoding.
///
/// The most significant bit (MSB) of each byte indicates if more bytes follow.
pub const VARINT_CONTINUATION_BIT: u8 = 0x80;

/// Payload bit mask for varint encoding.
///
/// The lower 7 bits of each byte contain the actual data.
pub const VARINT_PAYLOAD_MASK: u8 = 0x7F;

/// Size of a 32-bit fixed-width value in bytes.
///
/// Used for fixed32, sfixed32, and float types.
pub const FIXED32_BYTES: usize = 4;

/// Size of a 64-bit fixed-width value in bytes.
///
/// Used for fixed64, sfixed64, and double types.
pub const FIXED64_BYTES: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_type_try_from_trait() {
        // Test valid wire types
        assert_eq!(WireType::try_from(0).unwrap(), WireType::Varint);
        assert_eq!(WireType::try_from(1).unwrap(), WireType::Int64);
        assert_eq!(WireType::try_from(2).unwrap(), WireType::Len);
        assert_eq!(WireType::try_from(3).unwrap(), WireType::SGroup);
        assert_eq!(WireType::try_from(4).unwrap(), WireType::EGroup);
        assert_eq!(WireType::try_from(5).unwrap(), WireType::Int32);

        // Test invalid wire types
        let result = WireType::try_from(6);
        assert!(result.is_err());
        if let Err(ProtobufError::InvalidWireType { value }) = result {
            assert_eq!(value, 6);
        } else {
            panic!("Expected InvalidWireType error");
        }

        let result = WireType::try_from(255);
        assert!(result.is_err());
        if let Err(ProtobufError::InvalidWireType { value }) = result {
            assert_eq!(value, 255);
        } else {
            panic!("Expected InvalidWireType error");
        }
    }

    #[test]
    fn size_limits_are_inclusive() {
        assert!(ensure_message_size(MAX_MESSAGE_SIZE).is_ok());
        let over_message = MAX_MESSAGE_SIZE + 1;
        assert!(matches!(
            ensure_message_size(over_message),
            Err(ProtobufError::SizeLimitExceeded { value, limit })
                if value == u64::try_from(over_message).unwrap() && limit == MAX_MESSAGE_SIZE
        ));

        let max_string = u32::try_from(MAX_STRING_SIZE).unwrap();
        assert_eq!(ensure_len_payload_size(max_string).unwrap(), max_string);
        let over_string = max_string + 1;
        assert!(matches!(
            ensure_len_payload_size(over_string),
            Err(ProtobufError::SizeLimitExceeded { value, limit })
                if value == u64::from(over_string) && limit == MAX_STRING_SIZE
        ));
        assert!(matches!(
            ensure_len_payload_size(u32::MAX),
            Err(ProtobufError::SizeLimitExceeded { value, limit })
                if value == u64::from(u32::MAX) && limit == MAX_STRING_SIZE
        ));
    }
}
