//! Variant encoding and decoding logic for Protocol Buffers.
//!
//! This module provides basic variant operations including encoding, decoding,
//! and conversion to various protobuf integer types.

use crate::wire_format::MAX_VARINT_SIZE;

/// A fixed-size array wrapper for variant values.
///
/// This type represents the intermediate 8-byte value from serialized bytes
/// to protobuf integer types. It stores the raw bytes and provides conversion
/// methods to various protobuf integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VariantValue([u8; 8]);

impl VariantValue {
    /// Create a new VariantValue from raw bytes.
    ///
    /// This constructor takes the raw bytes as they appear in the serialized data.
    /// The user must ensure the bytes represent a valid variant encoding.
    pub fn new(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Create a new VariantValue from a slice of bytes.
    ///
    /// If the slice is shorter than 8 bytes, the remaining bytes are filled with zeros.
    /// If longer, only the first 8 bytes are used.
    pub fn from_slice(bytes: &[u8]) -> Self {
        let mut result = [0u8; 8];
        let len = bytes.len().min(8);
        result[..len].copy_from_slice(&bytes[..len]);
        Self(result)
    }

    /// Get the underlying byte array.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// Get the actual length of the variant in bytes.
    pub fn len(&self) -> usize {
        let mut len = 0;
        for &byte in &self.0 {
            if byte & 0x80 == 0 {
                break;
            }
            len += 1;
        }
        len + 1
    }

    /// Convert to protobuf UInt64 type (64-bit unsigned integer).
    pub fn to_uint64(&self) -> u64 {
        let mut result = 0u64;
        let mut shift = 0;

        for &byte in &self.0 {
            let value = (byte & 0x7F) as u64;
            result |= value << shift;

            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }

        result
    }

    /// Convert to protobuf UInt32 type (32-bit unsigned integer).
    pub fn to_uint32(&self) -> u32 {
        self.to_uint64() as u32
    }

    /// Convert to protobuf Int32 type (32-bit signed integer, no encoding).
    pub fn to_int32(&self) -> i32 {
        self.to_uint64() as i32
    }

    /// Convert to protobuf Int64 type (64-bit signed integer, no encoding).
    pub fn to_int64(&self) -> i64 {
        self.to_uint64() as i64
    }

    /// Convert to protobuf SInt32 type (32-bit signed integer with ZigZag decoding).
    pub fn to_sint32(&self) -> i32 {
        self.to_sint64() as i32
    }

    /// Convert to protobuf SInt64 type (64-bit signed integer with ZigZag decoding).
    pub fn to_sint64(&self) -> i64 {
        let value = self.to_uint64();
        ((value >> 1) as i64) ^ (-((value & 1) as i64))
    }

    /// Convert to protobuf Bool type.
    pub fn to_bool(&self) -> bool {
        self.to_uint64() != 0
    }
}

/// Read a variant from a byte iterator.
///
/// Returns the VariantValue and the number of bytes consumed.
/// Returns None if the variant is malformed or exceeds MAX_VARINT_SIZE.
pub fn read_variant<I>(iter: &mut I) -> Option<(VariantValue, usize)>
where
    I: Iterator<Item = u8>,
{
    let mut bytes = [0u8; 8];
    let mut bytes_read = 0;

    for byte in iter {
        if bytes_read >= MAX_VARINT_SIZE {
            return None; // Variant too long
        }

        bytes[bytes_read] = byte;
        bytes_read += 1;

        if byte & 0x80 == 0 {
            break;
        }
    }

    if bytes_read == 0 {
        return None; // No bytes read
    }

    Some((VariantValue::new(bytes), bytes_read))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_value_creation() {
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0];
        let variant = VariantValue::new(bytes);
        assert_eq!(variant.as_bytes(), &bytes);
        assert_eq!(variant.len(), 2);
    }

    #[test]
    fn test_variant_value_from_slice() {
        let input = [0x96, 0x01];
        let variant = VariantValue::from_slice(&input);
        assert_eq!(variant.len(), 2);
        assert_eq!(variant.to_uint64(), 150);
    }

    #[test]
    fn test_variant_conversions() {
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0]; // 150
        let variant = VariantValue::new(bytes);

        // Test all integer conversions
        assert_eq!(variant.to_uint64(), 150);
        assert_eq!(variant.to_uint32(), 150);
        assert_eq!(variant.to_int64(), 150);
        assert_eq!(variant.to_int32(), 150);
        assert_eq!(variant.to_bool(), true);
    }

    #[test]
    fn test_signed_integer_conversions() {
        // -1 in ZigZag encoding: 1
        let bytes = [0x01, 0, 0, 0, 0, 0, 0, 0];
        let variant = VariantValue::new(bytes);

        assert_eq!(variant.to_sint64(), -1);
        assert_eq!(variant.to_sint32(), -1);
    }

    #[test]
    fn test_read_variant() {
        let input = [0x96, 0x01];
        let mut iter = input.iter().copied();
        let (variant, bytes_read) = read_variant(&mut iter).unwrap();

        assert_eq!(variant.to_uint64(), 150);
        assert_eq!(bytes_read, 2);
    }
}
