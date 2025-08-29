//! Protocol Buffers wire format constants and definitions.
//!
//! This module provides the fundamental constants and types needed for
//! implementing Protocol Buffers encoding and decoding according to the
//! official wire format specification.

use crate::field_number::FieldNumber;
use ::std::convert::TryFrom;

/// Wire types used in Protocol Buffers encoding.
///
/// The wire type tells the parser how big the payload is and how to interpret it.
/// This allows old parsers to skip over new fields they don't understand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WireType {
    /// Variable-width integers (int32, int64, uint32, uint64, sint32, sint64, bool, enum)
    Varint = 0,
    /// 64-bit fixed-width values (fixed64, sfixed64, double)
    I64 = 1,
    /// Length-delimited values (string, bytes, embedded messages, packed repeated fields)
    Len = 2,
    /// Start group (deprecated feature)
    SGroup = 3,
    /// End group (deprecated feature)
    EGroup = 4,
    /// 32-bit fixed-width values (fixed32, sfixed32, float)
    I32 = 5,
}

impl From<WireType> for u8 {
    #[inline]
    fn from(wire_type: WireType) -> Self {
        wire_type as u8
    }
}

impl TryFrom<u8> for WireType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WireType::Varint),
            1 => Ok(WireType::I64),
            2 => Ok(WireType::Len),
            3 => Ok(WireType::SGroup),
            4 => Ok(WireType::EGroup),
            5 => Ok(WireType::I32),
            _ => Err(()),
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
pub const MAX_MESSAGE_SIZE: usize = 2 * 1024 * 1024 * 1024;

/// Maximum string/bytes field size (2 GiB).
pub const MAX_STRING_SIZE: usize = 2 * 1024 * 1024 * 1024;

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
pub const MAX_VARINT_SIZE: usize = 10;

/// Maximum variable-length integer value that can be encoded in 9 bytes.
///
/// This is used for optimization in variable-length integer encoding/decoding.
pub const MAX_9_BYTE_VL_INT: u64 = 0x7FFF_FFFF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 8 bytes.
pub const MAX_8_BYTE_VL_INT: u64 = 0xFFFF_FFFF_FFFF_FF;

/// Maximum variable-length integer value that can be encoded in 7 bytes.
pub const MAX_7_BYTE_VL_INT: u64 = 0x1FFFF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 6 bytes.
pub const MAX_6_BYTE_VL_INT: u64 = 0x3FF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 5 bytes.
pub const MAX_5_BYTE_VL_INT: u64 = 0x7_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 4 bytes.
pub const MAX_4_BYTE_VL_INT: u64 = 0xFFFFFFF;

/// Maximum variable-length integer value that can be encoded in 3 bytes.
pub const MAX_3_BYTE_VL_INT: u64 = 0x1FFFFF;

/// Maximum variable-length integer value that can be encoded in 2 bytes.
pub const MAX_2_BYTE_VL_INT: u64 = 0x3FFF;

/// Maximum variable-length integer value that can be encoded in 1 byte.
pub const MAX_1_BYTE_VL_INT: u64 = 0x7F;

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
pub const FIXED32_SIZE: usize = 4;

/// Size of a 64-bit fixed-width value in bytes.
///
/// Used for fixed64, sfixed64, and double types.
pub const FIXED64_SIZE: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_type_from_trait() {
        assert_eq!(u8::from(WireType::Varint), 0);
        assert_eq!(u8::from(WireType::I64), 1);
        assert_eq!(u8::from(WireType::Len), 2);
        assert_eq!(u8::from(WireType::SGroup), 3);
        assert_eq!(u8::from(WireType::EGroup), 4);
        assert_eq!(u8::from(WireType::I32), 5);
    }

    #[test]
    fn test_wire_type_try_from_trait() {
        assert_eq!(WireType::try_from(0), Ok(WireType::Varint));
        assert_eq!(WireType::try_from(1), Ok(WireType::I64));
        assert_eq!(WireType::try_from(2), Ok(WireType::Len));
        assert_eq!(WireType::try_from(3), Ok(WireType::SGroup));
        assert_eq!(WireType::try_from(4), Ok(WireType::EGroup));
        assert_eq!(WireType::try_from(5), Ok(WireType::I32));
        assert_eq!(WireType::try_from(6), Err(()));
        assert_eq!(WireType::try_from(255), Err(()));
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_FIELD_NUMBER, FieldNumber::MAX);
        assert_eq!(MIN_FIELD_NUMBER, FieldNumber::MIN);
        assert_eq!(MAX_MESSAGE_SIZE, 2 * 1024 * 1024 * 1024);
        assert_eq!(WIRE_TYPE_MASK, 0b111);
        assert_eq!(FIELD_NUMBER_SHIFT, 3);
        assert_eq!(MAX_VARINT_SIZE, 10);
        assert_eq!(VARINT_CONTINUATION_BIT, 0x80);
        assert_eq!(VARINT_PAYLOAD_MASK, 0x7F);
    }

    #[test]
    fn test_varint_size_constants() {
        assert_eq!(MAX_1_BYTE_VL_INT, 0x7F);
        assert_eq!(MAX_2_BYTE_VL_INT, 0x3FFF);
        assert_eq!(MAX_3_BYTE_VL_INT, 0x1FFFFF);
        assert_eq!(MAX_4_BYTE_VL_INT, 0xFFFFFFF);
        assert_eq!(MAX_5_BYTE_VL_INT, 0x7_FFFF_FFFF);
        assert_eq!(MAX_6_BYTE_VL_INT, 0x3FF_FFFF_FFFF);
        assert_eq!(MAX_7_BYTE_VL_INT, 0x1FFFF_FFFF_FFFF);
        assert_eq!(MAX_8_BYTE_VL_INT, 0xFFFF_FFFF_FFFF_FF);
        assert_eq!(MAX_9_BYTE_VL_INT, 0x7FFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_fixed_size_constants() {
        assert_eq!(FIXED32_SIZE, 4);
        assert_eq!(FIXED64_SIZE, 8);
    }

    #[test]
    fn test_varint_size_constants_mathematical() {
        // Each constant should be exactly 2^(7*N) - 1 where N is the number of bytes
        let constants = [
            MAX_1_BYTE_VL_INT,
            MAX_2_BYTE_VL_INT,
            MAX_3_BYTE_VL_INT,
            MAX_4_BYTE_VL_INT,
            MAX_5_BYTE_VL_INT,
            MAX_6_BYTE_VL_INT,
            MAX_7_BYTE_VL_INT,
            MAX_8_BYTE_VL_INT,
            MAX_9_BYTE_VL_INT,
        ];

        for (i, &constant) in constants.iter().enumerate() {
            let expected_bits = (i + 1) * 7;
            let expected_value = (1u64 << expected_bits) - 1;
            assert_eq!(
                constant,
                expected_value,
                "MAX_{}_BYTE_VL_INT should be 2^{} - 1 = {}",
                i + 1,
                expected_bits,
                expected_value
            );
        }
    }
}
