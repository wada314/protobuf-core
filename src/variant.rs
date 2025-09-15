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
use std::convert::TryFrom;
use std::io::{Read, Write};

/// A fixed-size array wrapper for variant values.
///
/// This type represents the intermediate 8-byte value from serialized bytes
/// to protobuf integer types. It stores the raw bytes and provides conversion
/// methods to various protobuf integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Variant([u8; 8]);

impl Variant {
    // ============================================================================
    // Converting from / to [u8; 8]
    // ============================================================================

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

    // ============================================================================
    // from / to protobuf integer types
    // ============================================================================

    /// Create a Variant from an unsigned 64-bit integer (u64).
    ///
    /// This method creates a Variant from a u64 value.
    pub fn from_uint64(value: u64) -> Self {
        let bytes = value.to_le_bytes();
        Self(bytes)
    }

    /// Create a Variant from an unsigned 32-bit integer (u32).
    ///
    /// This method creates a Variant from a u32 value.
    pub fn from_uint32(value: u32) -> Self {
        let bytes = (value as u64).to_le_bytes();
        Self(bytes)
    }

    /// Create a Variant from a signed 64-bit integer (i64) using ZigZag encoding.
    ///
    /// This method creates a Variant from an i64 value using ZigZag encoding.
    pub fn from_sint64(value: i64) -> Self {
        let zigzag_value = if value < 0 {
            ((-value) as u64) * 2 - 1
        } else {
            (value as u64) * 2
        };
        let bytes = zigzag_value.to_le_bytes();
        Self(bytes)
    }

    /// Create a Variant from a signed 32-bit integer (i32) using ZigZag encoding.
    ///
    /// This method creates a Variant from an i32 value using ZigZag encoding.
    pub fn from_sint32(value: i32) -> Self {
        Self::from_sint64(value as i64)
    }

    /// Create a Variant from a signed 64-bit integer (i64) without ZigZag encoding.
    ///
    /// This method creates a Variant from an i64 value without ZigZag encoding.
    pub fn from_int64(value: i64) -> Self {
        let bytes = (value as u64).to_le_bytes();
        Self(bytes)
    }

    /// Create a Variant from a signed 32-bit integer (i32) without ZigZag encoding.
    ///
    /// This method creates a Variant from an i32 value without ZigZag encoding.
    pub fn from_int32(value: i32) -> Self {
        let bytes = (value as u64).to_le_bytes();
        Self(bytes)
    }

    /// Create a Variant from a boolean.
    ///
    /// This method creates a Variant from a boolean value.
    pub fn from_bool(value: bool) -> Self {
        let bytes = (if value { 1u64 } else { 0u64 }).to_le_bytes();
        Self(bytes)
    }

    /// Convert to protobuf UInt64 type (64-bit unsigned integer).
    ///
    /// Since the Variant contains the decoded value (not encoded varint),
    /// this method simply converts the 8-byte array to u64 using little-endian interpretation.
    pub fn to_uint64(&self) -> u64 {
        // Use Rust's built-in method to convert bytes to u64
        u64::from_le_bytes(self.0)
    }

    /// Convert to unsigned 32-bit integer (u32).
    ///
    /// This method interprets the variant value as an unsigned 32-bit integer.
    /// Returns an error if the value is out of range for u32.
    pub fn to_uint32(&self) -> Result<u32> {
        let value = self.to_uint64();
        u32::try_from(value).map_err(|_| ProtobufError::VariantDowncastOutOfRange {
            value,
            target_type: "u32",
        })
    }

    /// Convert to signed 64-bit integer (i64) using ZigZag decoding.
    ///
    /// This method interprets the variant value as a ZigZag-encoded signed 64-bit integer.
    pub fn to_sint64(&self) -> i64 {
        let value = self.to_uint64();
        ((value >> 1) as i64) ^ (-((value & 1) as i64))
    }

    /// Convert to signed 32-bit integer (i32) using ZigZag decoding.
    ///
    /// This method interprets the variant value as a ZigZag-encoded signed 32-bit integer.
    /// Returns an error if the value is out of range for i32.
    pub fn to_sint32(&self) -> Result<i32> {
        let sint64_value = self.to_sint64();
        i32::try_from(sint64_value).map_err(|_| ProtobufError::VariantDowncastOutOfRange {
            value: sint64_value as u64,
            target_type: "i32",
        })
    }

    /// Convert to signed 64-bit integer (i64) without ZigZag decoding.
    ///
    /// This method interprets the variant value as a regular signed 64-bit integer.
    pub fn to_int64(&self) -> i64 {
        i64::from_le_bytes(self.0)
    }

    /// Convert to signed 32-bit integer (i32) without ZigZag decoding.
    ///
    /// This method interprets the variant value as a regular signed 32-bit integer.
    /// Returns an error if the value is out of range for i32.
    pub fn to_int32(&self) -> Result<i32> {
        let value = self.to_int64();
        i32::try_from(value).map_err(|_| ProtobufError::VariantDowncastOutOfRange {
            value: value as u64,
            target_type: "i32",
        })
    }

    /// Convert to boolean.
    ///
    /// This method interprets the variant value as a boolean.
    /// Returns true if the value is non-zero, false otherwise.
    pub fn to_bool(&self) -> bool {
        self.to_uint64() != 0
    }

    // ============================================================================
    // serialization
    // ============================================================================

    /// Get the size of this variant when encoded as a varint.
    ///
    /// This method calculates the exact number of bytes needed to encode
    /// the underlying value as a protobuf varint.
    pub fn varint_size(&self) -> usize {
        let value = self.to_uint64();
        if value == 0 {
            1
        } else {
            (64 - value.leading_zeros() as usize + 6) / 7
        }
    }

    /// Encode this variant as a varint and return the bytes with count.
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
    /// use protobuf_core::variant::Variant;
    ///
    /// let variant = Variant::from_uint64(150);
    /// let (bytes, count) = variant.encode();
    /// assert_eq!(count, 2);
    /// assert_eq!(&bytes[..count], &[0x96, 0x01]);
    /// ```
    pub fn encode(&self) -> ([u8; 10], usize) {
        let value = self.to_uint64();
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
}

/// Extension trait for collecting variants from byte iterators.
///
/// This trait provides convenient methods to collect variants directly from
/// any iterator that yields bytes.
///
/// # Example
/// ```
/// use protobuf_core::variant::{IteratorExtVariant, Variant};
///
/// let bytes = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut iter = bytes.into_iter();
/// let variant: Option<Variant> = iter.collect_variant().unwrap();
/// assert_eq!(variant.unwrap().to_uint64(), 150);
/// ```
pub trait IteratorExtVariant {
    /// Collect a variant from this iterator.
    ///
    /// Returns the Variant `Ok(Some(variant))` if successfully read.
    /// Returns `Ok(None)` if no input is available (empty iterator).
    /// Returns `Err(VariantError::TooLong)` if the variant exceeds MAX_VARINT_SIZE.
    fn try_collect_variant(self) -> Result<Option<Variant>>;
}

impl<I> IteratorExtVariant for I
where
    I: Iterator<Item = u8>,
{
    fn try_collect_variant(self) -> Result<Option<Variant>> {
        let mut bytes_read = 0;
        let mut decoded_value = 0u64;
        let mut shift = 0;

        for byte in self {
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
}

/// Extension trait for reading variants from Read instances.
///
/// This trait provides a convenient method to read variants directly from
/// any type that implements `std::io::Read`.
///
/// # Example
/// ```
/// use std::io::Cursor;
/// use protobuf_core::variant::{ReadExtVariant, Variant};
///
/// let data = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut reader = Cursor::new(data);
/// let variant = reader.read_variant().unwrap().unwrap();
/// assert_eq!(variant.to_uint64(), 150);
/// ```
pub trait ReadExtVariant {
    /// Read a variant from this reader.
    ///
    /// Returns the Variant `Ok(Some(variant))` if successfully read.
    /// Returns `Ok(None)` if no input is available (EOF).
    /// Returns `Err(VariantError::TooLong)` if the variant exceeds MAX_VARINT_SIZE.
    /// Returns `Err(ProtobufError::IoError)` if an I/O error occurs.
    ///
    /// # Error Cases
    /// This method has three distinct failure scenarios:
    /// 1. **No input available**: EOF reached → `Ok(None)`
    /// 2. **Input too long**: The variant exceeds MAX_VARINT_SIZE bytes → `Err(VariantError::TooLong)`
    /// 3. **I/O error**: Read operation failed → `Err(ProtobufError::IoError)`
    fn read_variant(&mut self) -> Result<Option<Variant>>;
}

impl<R> ReadExtVariant for R
where
    R: Read,
{
    fn read_variant(&mut self) -> Result<Option<Variant>> {
        let mut decoded_value = 0u64;
        let mut shift = 0;
        let mut buffer = [0u8; 1];

        for _ in 0..MAX_VARINT_SIZE {
            let n = self.read(&mut buffer)?;
            if n == 0 {
                return Ok(None); // EOF
            }

            let byte = buffer[0];
            let value = (byte & 0x7F) as u64;
            decoded_value |= value << shift;

            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }

        let result_bytes = decoded_value.to_le_bytes();
        Ok(Some(Variant::new(result_bytes)))
    }
}

/// Extension trait for writing variants to Write instances.
///
/// This trait provides a convenient method to write variants directly to
/// any type that implements `std::io::Write`.
///
/// # Example
/// ```
/// use std::io::Write;
/// use protobuf_core::variant::{WriteExtVariant, Variant};
///
/// let variant = Variant::from_uint64(150);
/// let mut writer = Vec::new();
/// writer.write_variant(&variant).unwrap();
/// assert_eq!(writer, vec![0x96, 0x01]);
/// ```
pub trait WriteExtVariant {
    /// Write a variant to this writer.
    ///
    /// Encodes a Variant as a varint and writes it to this writer.
    /// Returns the number of bytes written on success.
    ///
    /// # Arguments
    /// * `value` - The Variant to encode and write
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of bytes written
    /// * `Err(::std::io::Error)` - I/O error from the writer
    ///
    /// # Example
    /// ```
    /// use std::io::Write;
    /// use protobuf_core::variant::{WriteExtVariant, Variant};
    ///
    /// let variant = Variant::from_uint64(150);
    /// let mut buffer = Vec::new();
    /// let bytes_written = buffer.write_variant(&variant).unwrap();
    /// assert_eq!(bytes_written, 2);
    /// assert_eq!(buffer, vec![0x96, 0x01]);
    /// ```
    fn write_variant(&mut self, value: &Variant) -> std::io::Result<usize>;
}

impl<W> WriteExtVariant for W
where
    W: Write,
{
    fn write_variant(&mut self, value: &Variant) -> std::io::Result<usize> {
        let (bytes, count) = value.encode();
        self.write_all(&bytes[..count])?;
        Ok(count)
    }
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
        let variant = Variant::new(bytes); // Create new variant for next test
                                           // 406 in ZigZag encoding represents 203 in signed value
        assert_eq!(variant.to_sint64(), 203);
        let variant = Variant::new(bytes); // Create new variant for next test
        match variant.to_sint32() {
            Ok(value) => assert_eq!(value, 203),
            Err(e) => panic!("Expected Ok(203), got error: {:?}", e),
        }
        let variant = Variant::new(bytes); // Create new variant for next test
        assert_eq!(variant.to_bool(), true);
    }

    #[test]
    fn test_signed_integer_conversions() {
        // -1 in ZigZag encoding: 1
        let bytes = [0x01, 0, 0, 0, 0, 0, 0, 0];
        let variant = Variant::new(bytes);

        assert_eq!(variant.to_sint64(), -1);
        let variant = Variant::new(bytes); // Create new variant for next test
        match variant.to_sint32() {
            Ok(value) => assert_eq!(value, -1),
            Err(e) => panic!("Expected Ok(-1), got error: {:?}", e),
        }
    }

    #[test]
    fn test_read_variant_from_iterator() {
        let input = [0x96, 0x01];
        let iter = input.iter().copied();
        let variant = iter.try_collect_variant().unwrap().unwrap();

        assert_eq!(variant.to_uint64(), 150);
    }

    #[test]
    fn test_read_variant_from_reader() {
        use std::io::Cursor;
        let input = [0x96, 0x01];
        let mut reader = Cursor::new(input);
        let variant = reader.read_variant().unwrap().unwrap();

        assert_eq!(variant.to_uint64(), 150);
    }

    #[test]
    fn test_read_ext_variant_trait() {
        use std::io::Cursor;
        let input = [0x96, 0x01];
        let mut reader = Cursor::new(input);
        let variant = reader.read_variant().unwrap().unwrap();

        assert_eq!(variant.to_uint64(), 150);
    }

    #[test]
    fn test_write_ext_variant_trait() {
        let variant = Variant::from_uint64(150);
        let mut writer = Vec::new();
        let bytes_written = writer.write_variant(&variant).unwrap();

        assert_eq!(bytes_written, 2);
        assert_eq!(writer, vec![0x96, 0x01]);
    }

    #[test]
    fn test_iterator_ext_variant_trait() {
        let bytes = vec![0x96, 0x01]; // 150 in varint encoding
        let iter = bytes.into_iter();
        let variant = iter.try_collect_variant().unwrap().unwrap();

        assert_eq!(variant.to_uint64(), 150);
    }

    #[test]
    fn test_iterator_ext_variant_empty() {
        let bytes = vec![];
        let iter = bytes.into_iter();
        let variant = iter.try_collect_variant().unwrap();

        assert_eq!(variant, None);
    }

    #[test]
    fn test_write_variant() {
        // Test encoding small values
        let variant = Variant::from_uint64(150);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_variant(&variant).unwrap();
        assert_eq!(bytes_written, 2);
        assert_eq!(buffer, vec![0x96, 0x01]);

        // Test encoding single-byte values
        let variant = Variant::from_uint64(127);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_variant(&variant).unwrap();
        assert_eq!(bytes_written, 1);
        assert_eq!(buffer, vec![0x7F]);

        // Test encoding zero
        let variant = Variant::from_uint64(0);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_variant(&variant).unwrap();
        assert_eq!(bytes_written, 1);
        assert_eq!(buffer, vec![0x00]);

        // Test encoding large values
        let variant = Variant::from_uint64(0x7FFFFFFFFFFFFFFF);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_variant(&variant).unwrap();
        assert_eq!(bytes_written, 9); // 9-byte varint

        // Test encoding maximum varint (10 bytes)
        let variant = Variant::from_uint64(0xFFFFFFFFFFFFFFFF);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_variant(&variant).unwrap();
        assert_eq!(bytes_written, 10); // Maximum varint size
    }

    #[test]
    fn test_write_variant_roundtrip() {
        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            // Create Variant from the test value
            let variant = Variant::from_uint64(value);

            let mut buffer = Vec::new();
            buffer.write_variant(&variant).unwrap();

            let iter = buffer.iter().copied();
            let decoded_variant = iter.try_collect_variant().unwrap().unwrap();
            let decoded_value = decoded_variant.to_uint64();

            assert_eq!(decoded_value, value, "Roundtrip failed for value {}", value);
        }
    }

    #[test]
    fn test_encode_variant() {
        // Test encoding small values
        let variant = Variant::from_uint64(150);
        let (bytes, count) = variant.encode();
        assert_eq!(count, 2);
        assert_eq!(&bytes[..count], &[0x96, 0x01]);

        // Test encoding single-byte values
        let variant = Variant::from_uint64(127);
        let (bytes, count) = variant.encode();
        assert_eq!(count, 1);
        assert_eq!(&bytes[..count], &[0x7F]);

        // Test encoding zero
        let variant = Variant::from_uint64(0);
        let (bytes, count) = variant.encode();
        assert_eq!(count, 1);
        assert_eq!(&bytes[..count], &[0x00]);

        // Test encoding large values
        let variant = Variant::from_uint64(0x7FFFFFFFFFFFFFFF);
        let (bytes, count) = variant.encode();
        assert_eq!(count, 9);
        assert_eq!(
            &bytes[..count],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]
        );

        // Test encoding maximum varint (10 bytes)
        let variant = Variant::from_uint64(0xFFFFFFFFFFFFFFFF);
        let (bytes, count) = variant.encode();
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
            // Method 1: encode method
            let variant = Variant::from_uint64(value);
            let (array_bytes, array_count) = variant.encode();

            // Method 2: write_variant (std::io::Write)
            let variant2 = Variant::from_uint64(value);
            let mut vec_buffer = Vec::new();
            let vec_count = vec_buffer.write_variant(&variant2).unwrap();

            // Both methods should produce the same result
            assert_eq!(array_count, vec_count);
            assert_eq!(&array_bytes[..array_count], &vec_buffer[..]);
        }
    }

    #[test]
    fn test_from_traits() {
        // Test From<u64> for Variant
        let variant = Variant::from_uint64(150);
        assert_eq!(variant.to_uint64(), 150);

        // Test from_uint32
        let variant = Variant::from_uint32(150);
        assert_eq!(variant.to_uint64(), 150);

        // Test from_sint64
        let variant = Variant::from_sint64(150);
        assert_eq!(variant.to_sint64(), 150);

        // Test from_sint64 with negative value
        let variant = Variant::from_sint64(-1);
        assert_eq!(variant.to_sint64(), -1);

        // Test from_bool
        let variant = Variant::from_bool(true);
        assert_eq!(variant.to_bool(), true);

        // Test from_int32 (non-ZigZag)
        let variant = Variant::from_int32(150);
        assert_eq!(variant.to_int64(), 150);

        // Test from_int64 (non-ZigZag)
        let variant = Variant::from_int64(150);
        assert_eq!(variant.to_int64(), 150);
    }

    #[test]
    fn test_to_methods() {
        let bytes = [150, 0, 0, 0, 0, 0, 0, 0]; // 150 in little-endian
        let variant = Variant::new(bytes);

        // Test to_uint32
        assert_eq!(variant.to_uint32().unwrap(), 150);

        // Test to_sint32 (150 in ZigZag encoding represents 75 in signed value)
        assert_eq!(variant.to_sint32().unwrap(), 75);

        // Test to_sint64 (150 in ZigZag encoding represents 75 in signed value)
        assert_eq!(variant.to_sint64(), 75);

        // Test to_bool
        assert_eq!(variant.to_bool(), true);

        // Test to_int32 (non-ZigZag)
        assert_eq!(variant.to_int32().unwrap(), 150);

        // Test to_int64 (non-ZigZag)
        assert_eq!(variant.to_int64(), 150);
    }

    #[test]
    fn test_roundtrip_conversions() {
        // Test roundtrip for u64
        let original = 150u64;
        let variant = Variant::from_uint64(original);
        assert_eq!(variant.to_uint64(), original);

        // Test roundtrip for u32
        let original = 150u32;
        let variant = Variant::from_uint32(original);
        let converted = variant.to_uint32().unwrap();
        assert_eq!(converted, original);

        // Test roundtrip for i64 (ZigZag)
        let original = -1i64;
        let variant = Variant::from_sint64(original);
        let converted = variant.to_sint64();
        assert_eq!(converted, original);

        // Test roundtrip for i64 (non-ZigZag)
        let original = 150i64;
        let variant = Variant::from_int64(original);
        let converted = variant.to_int64();
        assert_eq!(converted, original);

        // Test roundtrip for i32 (non-ZigZag)
        let original = 150i32;
        let variant = Variant::from_int32(original);
        let converted = variant.to_int32().unwrap();
        assert_eq!(converted, original);

        // Test roundtrip for bool
        let original = true;
        let variant = Variant::from_bool(original);
        let converted = variant.to_bool();
        assert_eq!(converted, original);
    }
}
