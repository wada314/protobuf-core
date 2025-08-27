//! Variant encoding and decoding logic for Protocol Buffers.
//!
//! This module provides basic variant operations including encoding, decoding,
//! and conversion to various protobuf integer types.
//!
//! # Design Decisions
//!
//! ## read_variant function
//! The function returns both the VariantValue and the number of bytes consumed.
//! While the iterator could theoretically track the number of .next() calls,
//! returning the length provides several benefits:
//! 1. Clear indication of how many bytes were actually consumed
//! 2. Useful for callers who need to advance other buffers or track position
//! 3. Makes the API more explicit and self-documenting
//!
//! ## Error Handling Strategy
//! The read_variant function has two distinct failure cases:
//! 1. No input available (empty iterator)
//! 2. Input too long (exceeds MAX_VARINT_SIZE)
//! These are distinguished by returning None in both cases, but the caller
//! can check the iterator state to determine which case occurred.
//!
//! ## Conversion Method Design
//! Conversion methods that can fail (e.g., when values exceed target type limits)
//! return Option<T> to make error handling explicit. Methods that cannot fail
//! (like to_uint64 and to_bool) return their values directly.

use crate::wire_format::MAX_VARINT_SIZE;

/// Error types that can occur during variant reading and conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VariantError {
    /// Input exceeds MAX_VARINT_SIZE bytes
    #[error("Variant input too long (exceeds {0} bytes)")]
    TooLong(usize),
    /// Value exceeds target type range
    #[error("Value exceeds target type range")]
    ValueOutOfRange,
    /// Invalid conversion (e.g., upper bytes non-zero for Int32)
    #[error("Invalid conversion (upper bytes non-zero)")]
    InvalidConversion,
}

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

    /// Get the underlying byte array.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// Convert to protobuf UInt64 type (64-bit unsigned integer).
    ///
    /// Since the VariantValue contains the decoded value (not encoded varint),
    /// this method simply converts the 8-byte array to u64 using little-endian interpretation.
    pub fn to_uint64(&self) -> u64 {
        // Use Rust's built-in method to convert bytes to u64
        u64::from_le_bytes(self.0)
    }

    /// Convert to protobuf UInt32 type (32-bit unsigned integer).
    ///
    /// This conversion can fail if the value exceeds u32::MAX.
    /// Returns Err(VariantError::ValueOutOfRange) if the value is too large.
    pub fn to_uint32(&self) -> Result<u32, VariantError> {
        let value = self.to_uint64();
        u32::try_from(value).map_err(|_| VariantError::ValueOutOfRange)
    }

    /// Convert to protobuf Int32 type (32-bit signed integer, no encoding).
    ///
    /// Protobuf Int32 interprets the first 4 bytes as a signed 32-bit integer.
    /// This conversion can fail if the upper 4 bytes contain non-zero values.
    /// Returns Err(VariantError::InvalidConversion) if the value exceeds 32-bit range.
    pub fn to_int32(&self) -> Result<i32, VariantError> {
        // Use to_uint32 to check upper 4 bytes and get the value
        let uint32_value = self.to_uint32()?;
        // Convert from u32 to i32 (this always succeeds)
        Ok(uint32_value as i32)
    }

    /// Convert to protobuf Int64 type (64-bit signed integer, no encoding).
    ///
    /// Protobuf Int64 interprets all 8 bytes as a signed 64-bit integer.
    /// This conversion cannot fail as all 8 bytes are used.
    pub fn to_int64(&self) -> i64 {
        // Interpret all 8 bytes as i64 (little-endian)
        i64::from_le_bytes(self.0)
    }

    /// Convert to protobuf SInt32 type (32-bit signed integer with ZigZag decoding).
    ///
    /// This conversion can fail if the ZigZag decoded value exceeds i32::MAX.
    /// Returns Err(VariantError::ValueOutOfRange) if the value is too large.
    pub fn to_sint32(&self) -> Result<i32, VariantError> {
        let sint64 = self.to_sint64();
        i32::try_from(sint64).map_err(|_| VariantError::ValueOutOfRange)
    }

    /// Convert to protobuf SInt64 type (64-bit signed integer with ZigZag decoding).
    pub fn to_sint64(&self) -> i64 {
        let value = self.to_uint64();
        ((value >> 1) as i64) ^ (-((value & 1) as i64))
    }

    /// Convert to protobuf Bool type.
    ///
    /// Any non-zero value is considered true, zero is false.
    /// This conversion cannot fail, so it always returns Ok(bool).
    pub fn to_bool(&self) -> Result<bool, VariantError> {
        let value = self.to_uint64();
        Ok(value != 0)
    }
}

/// Read a variant from a byte iterator.
///
/// Returns the VariantValue and the number of bytes consumed.
/// Returns `Ok(None)` if no input is available (empty iterator).
/// Returns `Err(VariantError::TooLong)` if the variant exceeds MAX_VARINT_SIZE.
///
/// # Error Cases
/// This function has two distinct failure scenarios:
/// 1. **No input available**: The iterator is empty (no bytes to read) → `Ok(None)`
/// 2. **Input too long**: The variant exceeds MAX_VARINT_SIZE bytes → `Err(VariantError::TooLong)`
///
/// The `Result<Option<T>, E>` pattern clearly distinguishes between:
/// - Successful reads with data: `Ok(Some((VariantValue, usize)))`
/// - No data available: `Ok(None)`
/// - Error conditions: `Err(VariantError)`
pub fn read_variant<I>(iter: &mut I) -> Result<Option<(VariantValue, usize)>, VariantError>
where
    I: Iterator<Item = u8>,
{
    let mut bytes_read = 0;
    let mut decoded_value = 0u64;
    let mut shift = 0;

    for byte in iter {
        if bytes_read >= MAX_VARINT_SIZE {
            return Err(VariantError::TooLong(MAX_VARINT_SIZE)); // Variant too long
        }

        let value = (byte & 0x7F) as u64;
        decoded_value |= value << shift;
        bytes_read += 1;

        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    if bytes_read == 0 {
        return Ok(None); // No bytes read
    }

    // Convert the decoded u64 value to 8-byte array (little-endian)
    let mut result_bytes = [0u8; 8];
    for i in 0..8 {
        result_bytes[i] = ((decoded_value >> (i * 8)) & 0xFF) as u8;
    }

    Ok(Some((VariantValue::new(result_bytes), bytes_read)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_value_creation() {
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0];
        let variant = VariantValue::new(bytes);
        assert_eq!(variant.as_bytes(), &bytes);
    }

    #[test]
    fn test_variant_conversions() {
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0]; // 0x0196 = 406
        let variant = VariantValue::new(bytes);

        // Test all integer conversions
        assert_eq!(variant.to_uint64(), 406);
        assert_eq!(variant.to_uint32(), Ok(406));
        assert_eq!(variant.to_int64(), 406);
        assert_eq!(variant.to_int32(), Ok(406));
        assert_eq!(variant.to_bool(), Ok(true));
    }

    #[test]
    fn test_signed_integer_conversions() {
        // -1 in ZigZag encoding: 1
        let bytes = [0x01, 0, 0, 0, 0, 0, 0, 0];
        let variant = VariantValue::new(bytes);

        assert_eq!(variant.to_sint64(), -1);
        assert_eq!(variant.to_sint32(), Ok(-1));
    }

    #[test]
    fn test_read_variant() {
        let input = [0x96, 0x01];
        let mut iter = input.iter().copied();
        let (variant, bytes_read) = read_variant(&mut iter).unwrap().unwrap();

        assert_eq!(variant.to_uint64(), 150);
        assert_eq!(bytes_read, 2);
    }
}
