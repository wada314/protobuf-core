//! Variant encoding and decoding logic for Protocol Buffers.
//!
//! This module provides basic variant operations including encoding, decoding,
//! and conversion to various protobuf integer types.
//!
//! # Implementation Philosophy
//!
//! This is a **reference implementation** designed for clarity and educational value.
//! While functional and correct, it is not optimized for performance. Implementors
//! are encouraged to:
//! - Use platform-specific optimizations (e.g., SIMD instructions)
//! - Replace manual loops with built-in methods where appropriate
//! - Profile and optimize based on their specific use cases
//!
//! # Design Decisions
//!
//! ## read_variant function
//! The function returns the Variant if successfully read.
//! Returns `Ok(None)` if no input is available (empty iterator).
//! Returns `Err(VariantError::TooLong)` if the variant exceeds MAX_VARINT_SIZE.
//!
//! ## Error Handling Strategy
//! The read_variant function has two distinct failure scenarios:
//! 1. **No input available**: The iterator is empty (no bytes to read) → `Ok(None)`
//! 2. **Input too long**: The variant exceeds MAX_VARINT_SIZE bytes → `Err(VariantError::TooLong)`
//!
//! ## Conversion Method Design
//! Conversion methods that can fail (e.g., when values exceed target type limits)
//! return `Result<T, VariantError>` to make error handling explicit.
//! Methods that cannot fail (like `to_uint64` and `to_bool`) return their values directly.

use crate::wire_format::MAX_VARINT_SIZE;
use crate::{ProtobufError, Result};

/// A fixed-size array wrapper for variant values.
///
/// This type represents the intermediate 8-byte value from serialized bytes
/// to protobuf integer types. It stores the raw bytes and provides conversion
/// methods to various protobuf integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Variant([u8; 8]);

impl Variant {
    /// Create a new Variant from raw bytes.
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
    /// Since the Variant contains the decoded value (not encoded varint),
    /// this method simply converts the 8-byte array to u64 using little-endian interpretation.
    pub fn to_uint64(&self) -> u64 {
        // Use Rust's built-in method to convert bytes to u64
        u64::from_le_bytes(self.0)
    }

    /// Convert to protobuf UInt32 type (32-bit unsigned integer).
    ///
    /// This conversion can fail if the value exceeds u32::MAX.
    /// Returns Err(ProtobufError::VariantDowncastOutOfRange) if the value is too large.
    pub fn to_uint32(&self) -> Result<u32> {
        let value = self.to_uint64();
        u32::try_from(value).map_err(|_| ProtobufError::VariantDowncastOutOfRange {
            value,
            target_type: "u32",
        })
    }

    /// Convert to protobuf Int32 type (32-bit signed integer, no encoding).
    ///
    /// Protobuf Int32 interprets the first 4 bytes as a signed 32-bit integer.
    /// This conversion can fail if the upper 4 bytes contain non-zero values.
    /// Returns Err(ProtobufError::VariantDowncastOutOfRange) if the value exceeds 32-bit range.
    pub fn to_int32(&self) -> Result<i32> {
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
    /// Returns Err(ProtobufError::VariantDowncastOutOfRange) if the value is too large.
    pub fn to_sint32(&self) -> Result<i32> {
        let sint64 = self.to_sint64();
        i32::try_from(sint64).map_err(|_| ProtobufError::VariantDowncastOutOfRange {
            value: sint64 as u64,
            target_type: "i32",
        })
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
    pub fn to_bool(&self) -> Result<bool> {
        let value = self.to_uint64();
        Ok(value != 0)
    }

    /// Create a Variant from a u64 value for debugging and testing purposes.
    ///
    /// This method is intended for creating test data and debugging scenarios.
    /// It converts the u64 value to its little-endian byte representation.
    ///
    /// # Safety
    /// This method is safe but should only be used in debug/test contexts.
    /// For production code, use `read_variant` to decode actual varint data.
    ///
    /// # Example
    /// ```
    /// use protobuf_core::variant::Variant;
    ///
    /// let variant = Variant::debug_from_u64(150);
    /// assert_eq!(variant.to_uint64(), 150);
    /// ```
    #[cfg(test)]
    pub fn debug_from_u64(value: u64) -> Self {
        let bytes = value.to_le_bytes();
        Self(bytes)
    }
}

/// Read a variant from a byte iterator.
///
/// Returns the Variant if successfully read.
/// Returns `Ok(None)` if no input is available (empty iterator).
/// Returns `Err(VariantError::TooLong)` if the variant exceeds MAX_VARINT_SIZE.
///
/// # Error Cases
/// This function has two distinct failure scenarios:
/// 1. **No input available**: The iterator is empty (no bytes to read) → `Ok(None)`
/// 2. **Input too long**: The variant exceeds MAX_VARINT_SIZE bytes → `Err(VariantError::TooLong)`
///
/// The `Result<Option<T>, E>` pattern clearly distinguishes between:
/// - Successful reads with data: `Ok(Some(Variant))`
/// - No data available: `Ok(None)`
/// - Error conditions: `Err(VariantError)`
pub fn read_variant<I>(iter: &mut I) -> Result<Option<Variant>>
where
    I: Iterator<Item = u8>,
{
    let mut bytes_read = 0;
    let mut decoded_value = 0u64;
    let mut shift = 0;

    for byte in iter {
        if bytes_read >= MAX_VARINT_SIZE {
            return Err(ProtobufError::VariantDowncastOutOfRange {
                value: 0,
                target_type: "variant (too long)",
            }); // Variant too long
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
    let result_bytes = decoded_value.to_le_bytes();

    Ok(Some(Variant::new(result_bytes)))
}

/// Write a variant to a writer.
///
/// Encodes a Variant as a varint and writes it to the provided writer.
/// Returns the number of bytes written on success.
///
/// # Arguments
/// * `writer` - The writer to write the encoded varint to
/// * `value` - The Variant to encode
///
/// # Returns
/// * `Ok(usize)` - Number of bytes written
/// * `Err(::std::io::Error)` - I/O error from the writer
///
/// # Example
/// ```
/// use ::std::io::Write;
/// use protobuf_core::variant::{write_variant, Variant};
///
/// let variant = Variant::new([150, 0, 0, 0, 0, 0, 0, 0]);
/// let mut buffer = Vec::new();
/// write_variant(&mut buffer, &variant).unwrap();
/// assert_eq!(buffer, vec![0x96, 0x01]);
/// ```
///
/// # Implementation Notes
/// This implementation uses encode_variant internally for efficiency,
/// then writes all bytes at once to the writer. This approach:
/// - Avoids multiple write operations
/// - Leverages the optimized encode_variant function
/// - Maintains compatibility with ::std::io::Write trait
/// - Provides type consistency with read_variant
pub fn write_variant<W>(writer: &mut W, value: &Variant) -> ::std::io::Result<usize>
where
    W: ::std::io::Write,
{
    let u64_value = value.to_uint64();
    let (bytes, count) = encode_variant(u64_value);
    writer.write_all(&bytes[..count])?;
    Ok(count)
}

/// Encode a variant and return the bytes with count.
///
/// Returns a tuple of (bytes, count) where:
/// - bytes: fixed-size array containing the encoded varint
/// - count: actual number of bytes used (1-10)
///
/// This is the most efficient method as it avoids any memory allocation
/// and returns a fixed-size array that can be easily copied or sliced.
///
/// # Example
/// ```
/// use protobuf_core::variant::encode_variant;
///
/// let (bytes, count) = encode_variant(150);
/// assert_eq!(count, 2);
/// assert_eq!(&bytes[..count], &[0x96, 0x01]);
/// ```
///
/// # Implementation Notes
/// This is a reference implementation that processes one byte at a time.
/// Implementors may optimize by:
/// - Using platform-specific SIMD instructions
/// - Processing multiple bytes in parallel
/// - Using lookup tables for common values
pub fn encode_variant(value: u64) -> ([u8; 10], usize) {
    let mut bytes = [0u8; 10];
    let mut bytes_written = 0;
    let mut remaining_value = value;

    for byte in bytes.iter_mut() {
        *byte = (remaining_value & 0x7F) as u8;
        remaining_value >>= 7;
        bytes_written += 1;

        if remaining_value == 0 {
            break;
        } else {
            *byte |= 0x80; // continuation bit
        }
    }

    (bytes, bytes_written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_value_creation() {
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0];
        let variant = Variant::new(bytes);
        assert_eq!(variant.as_bytes(), &bytes);
    }

    #[test]
    fn test_variant_conversions() {
        // 406 in little-endian: 0x96, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0];
        let variant = Variant::new(bytes);

        // Test all integer conversions
        assert_eq!(variant.to_uint64(), 406);
        match variant.to_uint32() {
            Ok(value) => assert_eq!(value, 406),
            Err(e) => panic!("Expected Ok(406), got error: {:?}", e),
        }
        assert_eq!(variant.to_int64(), 406);
        match variant.to_int32() {
            Ok(value) => assert_eq!(value, 406),
            Err(e) => panic!("Expected Ok(406), got error: {:?}", e),
        }
        match variant.to_bool() {
            Ok(value) => assert_eq!(value, true),
            Err(e) => panic!("Expected Ok(true), got error: {:?}", e),
        }
    }

    #[test]
    fn test_signed_integer_conversions() {
        // -1 in ZigZag encoding: 1
        let bytes = [0x01, 0, 0, 0, 0, 0, 0, 0];
        let variant = Variant::new(bytes);

        assert_eq!(variant.to_sint64(), -1);
        match variant.to_sint32() {
            Ok(value) => assert_eq!(value, -1),
            Err(e) => panic!("Expected Ok(-1), got error: {:?}", e),
        }
    }

    #[test]
    fn test_read_variant() {
        let input = [0x96, 0x01];
        let mut iter = input.iter().copied();
        let variant = read_variant(&mut iter).unwrap().unwrap();

        assert_eq!(variant.to_uint64(), 150);
    }

    #[test]
    fn test_write_variant() {
        // Test encoding small values
        let variant = Variant::debug_from_u64(150);
        let mut buffer = Vec::new();
        let bytes_written = write_variant(&mut buffer, &variant).unwrap();
        assert_eq!(bytes_written, 2);
        assert_eq!(buffer, vec![0x96, 0x01]);

        // Test encoding single-byte values
        let variant = Variant::debug_from_u64(127);
        let mut buffer = Vec::new();
        let bytes_written = write_variant(&mut buffer, &variant).unwrap();
        assert_eq!(bytes_written, 1);
        assert_eq!(buffer, vec![0x7F]);

        // Test encoding zero
        let variant = Variant::debug_from_u64(0);
        let mut buffer = Vec::new();
        let bytes_written = write_variant(&mut buffer, &variant).unwrap();
        assert_eq!(bytes_written, 1);
        assert_eq!(buffer, vec![0x00]);

        // Test encoding large values
        let variant = Variant::debug_from_u64(0x7FFFFFFFFFFFFFFF);
        let mut buffer = Vec::new();
        let bytes_written = write_variant(&mut buffer, &variant).unwrap();
        assert_eq!(bytes_written, 9); // 9-byte varint

        // Test encoding maximum varint (10 bytes)
        let variant = Variant::debug_from_u64(0xFFFFFFFFFFFFFFFF);
        let mut buffer = Vec::new();
        let bytes_written = write_variant(&mut buffer, &variant).unwrap();
        assert_eq!(bytes_written, 10); // Maximum varint size
    }

    #[test]
    fn test_write_variant_roundtrip() {
        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            // Create Variant from the test value
            let variant = Variant::debug_from_u64(value);

            let mut buffer = Vec::new();
            write_variant(&mut buffer, &variant).unwrap();

            let mut iter = buffer.iter().copied();
            let decoded_variant = read_variant(&mut iter).unwrap().unwrap();
            let decoded_value = decoded_variant.to_uint64();

            assert_eq!(decoded_value, value, "Roundtrip failed for value {}", value);
        }
    }

    #[test]
    fn test_encode_variant() {
        // Test encoding small values
        let (bytes, count) = encode_variant(150);
        assert_eq!(count, 2);
        assert_eq!(&bytes[..count], &[0x96, 0x01]);

        // Test encoding single-byte values
        let (bytes, count) = encode_variant(127);
        assert_eq!(count, 1);
        assert_eq!(&bytes[..count], &[0x7F]);

        // Test encoding zero
        let (bytes, count) = encode_variant(0);
        assert_eq!(count, 1);
        assert_eq!(&bytes[..count], &[0x00]);

        // Test encoding large values
        let (bytes, count) = encode_variant(0x7FFFFFFFFFFFFFFF);
        assert_eq!(count, 9);
        assert_eq!(
            &bytes[..count],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]
        );

        // Test encoding maximum varint (10 bytes)
        let (bytes, count) = encode_variant(0xFFFFFFFFFFFFFFFF);
        assert_eq!(count, 10);
        assert_eq!(
            &bytes[..count],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]
        );
    }

    #[test]
    fn test_all_encoding_methods_consistency() {
        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            // Method 1: encode_variant
            let (array_bytes, array_count) = encode_variant(value);

            // Method 2: write_variant (std::io::Write)
            let variant = Variant::debug_from_u64(value);

            let mut vec_buffer = Vec::new();
            let vec_count = write_variant(&mut vec_buffer, &variant).unwrap();

            // Both methods should produce the same result
            assert_eq!(array_count, vec_count);
            assert_eq!(&array_bytes[..array_count], &vec_buffer[..]);
        }
    }
}
