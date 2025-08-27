//! Protocol Buffers wire format constants and definitions.
//!
//! This module provides the fundamental constants and types needed for
//! implementing Protocol Buffers encoding and decoding according to the
//! official wire format specification.

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

impl WireType {
    /// Get the numeric value of the wire type.
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Try to create a WireType from a numeric value.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(WireType::Varint),
            1 => Some(WireType::I64),
            2 => Some(WireType::Len),
            3 => Some(WireType::SGroup),
            4 => Some(WireType::EGroup),
            5 => Some(WireType::I32),
            _ => None,
        }
    }
}

/// Maximum field number allowed in Protocol Buffers.
///
/// Field numbers must be in the range [1, 2^29 - 1].
pub const MAX_FIELD_NUMBER: u32 = 536_870_911; // 2^29 - 1

/// Minimum field number allowed in Protocol Buffers.
pub const MIN_FIELD_NUMBER: u32 = 1;

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

/// Maximum varint value that can be encoded in 9 bytes.
///
/// This is used for optimization in varint encoding/decoding.
pub const MAX_9_BYTE_VARINT: u64 = 0x7F_FFFF_FFFF_FFFF;

/// Maximum varint value that can be encoded in 8 bytes.
pub const MAX_8_BYTE_VARINT: u64 = 0x7F_FFFF_FFFF_FF;

/// Maximum varint value that can be encoded in 7 bytes.
pub const MAX_7_BYTE_VARINT: u64 = 0x7F_FFFF_FFFF;

/// Maximum varint value that can be encoded in 6 bytes.
pub const MAX_6_BYTE_VARINT: u64 = 0x7F_FFFF_FF;

/// Maximum varint value that can be encoded in 5 bytes.
pub const MAX_5_BYTE_VARINT: u64 = 0x7F_FFFF;

/// Maximum varint value that can be encoded in 4 bytes.
pub const MAX_4_BYTE_VARINT: u64 = 0x7F_FF;

/// Maximum varint value that can be encoded in 3 bytes.
pub const MAX_3_BYTE_VARINT: u64 = 0x7F;

/// Maximum varint value that can be encoded in 2 bytes.
pub const MAX_2_BYTE_VARINT: u64 = 0x7F;

/// Maximum varint value that can be encoded in 1 byte.
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
pub const FIXED32_SIZE: usize = 4;

/// Size of a 64-bit fixed-width value in bytes.
pub const FIXED64_SIZE: usize = 8;

/// Size of a float value in bytes.
pub const FLOAT_SIZE: usize = 4;

/// Size of a double value in bytes.
pub const DOUBLE_SIZE: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_type_values() {
        assert_eq!(WireType::Varint.as_u8(), 0);
        assert_eq!(WireType::I64.as_u8(), 1);
        assert_eq!(WireType::Len.as_u8(), 2);
        assert_eq!(WireType::SGroup.as_u8(), 3);
        assert_eq!(WireType::EGroup.as_u8(), 4);
        assert_eq!(WireType::I32.as_u8(), 5);
    }

    #[test]
    fn test_wire_type_from_u8() {
        assert_eq!(WireType::from_u8(0), Some(WireType::Varint));
        assert_eq!(WireType::from_u8(1), Some(WireType::I64));
        assert_eq!(WireType::from_u8(2), Some(WireType::Len));
        assert_eq!(WireType::from_u8(3), Some(WireType::SGroup));
        assert_eq!(WireType::from_u8(4), Some(WireType::EGroup));
        assert_eq!(WireType::from_u8(5), Some(WireType::I32));
        assert_eq!(WireType::from_u8(6), None);
        assert_eq!(WireType::from_u8(255), None);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_FIELD_NUMBER, 536_870_911);
        assert_eq!(MIN_FIELD_NUMBER, 1);
        assert_eq!(MAX_MESSAGE_SIZE, 2 * 1024 * 1024 * 1024);
        assert_eq!(WIRE_TYPE_MASK, 0b111);
        assert_eq!(FIELD_NUMBER_SHIFT, 3);
        assert_eq!(MAX_VARINT_SIZE, 10);
        assert_eq!(VARINT_CONTINUATION_BIT, 0x80);
        assert_eq!(VARINT_PAYLOAD_MASK, 0x7F);
    }

    #[test]
    fn test_varint_size_constants() {
        assert_eq!(MAX_1_BYTE_VARINT, 0x7F);
        assert_eq!(MAX_2_BYTE_VARINT, 0x7F);
        assert_eq!(MAX_3_BYTE_VARINT, 0x7F);
        assert_eq!(MAX_4_BYTE_VARINT, 0x7F_FF);
        assert_eq!(MAX_5_BYTE_VARINT, 0x7F_FFFF);
        assert_eq!(MAX_6_BYTE_VARINT, 0x7F_FFFF_FF);
        assert_eq!(MAX_7_BYTE_VARINT, 0x7F_FFFF_FFFF);
        assert_eq!(MAX_8_BYTE_VARINT, 0x7F_FFFF_FFFF_FF);
        assert_eq!(MAX_9_BYTE_VARINT, 0x7F_FFFF_FFFF_FFFF);
    }
}
