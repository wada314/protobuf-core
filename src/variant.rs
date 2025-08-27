//! Variant encoding and decoding logic for Protocol Buffers.
//!
//! This module provides basic variant operations including encoding, decoding,
//! and conversion to various protobuf integer types.

use crate::wire_format::MAX_VARINT_SIZE;

/// A fixed-size array wrapper for variant values.
///
/// This type ensures that variant values are always stored in a consistent format
/// and provides conversion methods to various protobuf integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VariantValue([u8; 8]);

impl VariantValue {
    /// Create a new VariantValue from a protobuf UInt64 value.
    pub fn new(value: u64) -> Self {
        let mut bytes = [0u8; 8];
        let mut val = value;
        let mut i = 0;

        while val >= 0x80 && i < 7 {
            bytes[i] = (val as u8) | 0x80;
            val >>= 7;
            i += 1;
        }
        bytes[i] = val as u8;

        Self(bytes)
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

    /// Convert to protobuf UInt64 type.
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

    /// Convert to protobuf SInt64 type (handles ZigZag decoding).
    pub fn to_sint64(&self) -> i64 {
        let value = self.to_uint64();
        ((value >> 1) as i64) ^ (-((value & 1) as i64))
    }

    /// Convert to protobuf UInt32 type.
    pub fn to_uint32(&self) -> u32 {
        self.to_uint64() as u32
    }

    /// Convert to protobuf SInt32 type (handles ZigZag decoding).
    pub fn to_sint32(&self) -> i32 {
        self.to_sint64() as i32
    }

    /// Convert to protobuf Bool type.
    pub fn to_bool(&self) -> bool {
        self.to_uint64() != 0
    }
}

impl From<u64> for VariantValue {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl From<i64> for VariantValue {
    fn from(value: i64) -> Self {
        // ZigZag encoding for SInt64
        let unsigned = ((value << 1) ^ (value >> 63)) as u64;
        Self::new(unsigned)
    }
}

impl From<u32> for VariantValue {
    fn from(value: u32) -> Self {
        Self::new(value as u64)
    }
}

impl From<i32> for VariantValue {
    fn from(value: i32) -> Self {
        // ZigZag encoding for SInt32
        let unsigned = ((value << 1) ^ (value >> 31)) as u64;
        Self::new(unsigned as u64)
    }
}

impl From<bool> for VariantValue {
    fn from(value: bool) -> Self {
        Self::new(if value { 1 } else { 0 })
    }
}

/// Read a variant from a byte iterator.
///
/// Returns the decoded UInt64 value and the number of bytes consumed.
/// Returns None if the variant is malformed or exceeds MAX_VARINT_SIZE.
pub fn read_variant<I>(iter: &mut I) -> Option<(u64, usize)>
where
    I: Iterator<Item = u8>,
{
    let mut result = 0u64;
    let mut shift = 0;
    let mut bytes_read = 0;

    for byte in iter {
        if bytes_read >= MAX_VARINT_SIZE {
            return None; // Variant too long
        }

        let value = (byte & 0x7F) as u64;
        result |= value << shift;
        bytes_read += 1;

        if byte & 0x80 == 0 {
            return Some((result, bytes_read));
        }

        shift += 7;
        if shift >= 64 {
            return None; // Shift overflow
        }
    }

    None // Unexpected end of input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_value() {
        // Test UInt64 conversion
        let variant = VariantValue::from(150u64);
        assert_eq!(variant.to_uint64(), 150);
        assert_eq!(variant.len(), 2);

        // Test SInt64 conversion (ZigZag encoding)
        let variant = VariantValue::from(-1i64);
        assert_eq!(variant.to_sint64(), -1);

        // Test Bool conversion
        let variant = VariantValue::from(true);
        assert_eq!(variant.to_bool(), true);
    }

    #[test]
    fn test_read_variant() {
        let bytes = vec![0x96, 0x01]; // 150 in variant encoding
        let mut iter = bytes.into_iter();
        let (value, bytes_read) = read_variant(&mut iter).unwrap();
        assert_eq!(value, 150);
        assert_eq!(bytes_read, 2);
    }
}
