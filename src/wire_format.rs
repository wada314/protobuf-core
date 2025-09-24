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
    /// Variable-width integers (Int32, Int64, UInt32, UInt64, SInt32, SInt64, Bool, Enum)
    Varint = 0,
    /// 64-bit fixed-width values (Fixed64, SFixed64, Double)
    I64 = 1,
    /// Length-delimited values (String, Bytes, embedded messages, packed repeated fields)
    Len = 2,
    /// Start group (deprecated feature)
    SGroup = 3,
    /// End group (deprecated feature)
    EGroup = 4,
    /// 32-bit fixed-width values (Fixed32, SFixed32, Float)
    I32 = 5,
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
            1 => Ok(WireType::I64),
            2 => Ok(WireType::Len),
            3 => Ok(WireType::SGroup),
            4 => Ok(WireType::EGroup),
            5 => Ok(WireType::I32),
            _ => Err(crate::ProtobufError::InvalidWireType { value }),
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
pub const MAX_9_BYTE_VARINT: u64 = 0x7FFF_FFFF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 8 bytes.
pub const MAX_8_BYTE_VARINT: u64 = 0xFF_FFFF_FFFF_FFFF;

/// Maximum variable-length integer value that can be encoded in 7 bytes.
pub const MAX_7_BYTE_VARINT: u64 = 0x1FFFF_FFFF_FFFF;

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
        // Test valid wire types
        assert_eq!(WireType::try_from(0).unwrap(), WireType::Varint);
        assert_eq!(WireType::try_from(1).unwrap(), WireType::I64);
        assert_eq!(WireType::try_from(2).unwrap(), WireType::Len);
        assert_eq!(WireType::try_from(3).unwrap(), WireType::SGroup);
        assert_eq!(WireType::try_from(4).unwrap(), WireType::EGroup);
        assert_eq!(WireType::try_from(5).unwrap(), WireType::I32);

        // Test invalid wire types
        let result = WireType::try_from(6);
        assert!(result.is_err());
        if let Err(crate::ProtobufError::InvalidWireType { value }) = result {
            assert_eq!(value, 6);
        } else {
            panic!("Expected InvalidWireType error");
        }

        let result = WireType::try_from(255);
        assert!(result.is_err());
        if let Err(crate::ProtobufError::InvalidWireType { value }) = result {
            assert_eq!(value, 255);
        } else {
            panic!("Expected InvalidWireType error");
        }
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
        assert_eq!(MAX_1_BYTE_VARINT, 0x7F);
        assert_eq!(MAX_2_BYTE_VARINT, 0x3FFF);
        assert_eq!(MAX_3_BYTE_VARINT, 0x1FFFFF);
        assert_eq!(MAX_4_BYTE_VARINT, 0xFFFFFFF);
        assert_eq!(MAX_5_BYTE_VARINT, 0x7_FFFF_FFFF);
        assert_eq!(MAX_6_BYTE_VARINT, 0x3FF_FFFF_FFFF);
        assert_eq!(MAX_7_BYTE_VARINT, 0x1FFFF_FFFF_FFFF);
        assert_eq!(MAX_8_BYTE_VARINT, 0xFFFF_FFFF_FFFF_FF);
        assert_eq!(MAX_9_BYTE_VARINT, 0x7FFF_FFFF_FFFF_FFFF);
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
            MAX_1_BYTE_VARINT,
            MAX_2_BYTE_VARINT,
            MAX_3_BYTE_VARINT,
            MAX_4_BYTE_VARINT,
            MAX_5_BYTE_VARINT,
            MAX_6_BYTE_VARINT,
            MAX_7_BYTE_VARINT,
            MAX_8_BYTE_VARINT,
            MAX_9_BYTE_VARINT,
        ];

        for (i, &constant) in constants.iter().enumerate() {
            let expected_bits = (i + 1) * 7;
            let expected_value = (1u64 << expected_bits) - 1;
            assert_eq!(
                constant,
                expected_value,
                "MAX_{}_BYTE_VARINT should be 2^{} - 1 = {}",
                i + 1,
                expected_bits,
                expected_value
            );
        }
    }
}
