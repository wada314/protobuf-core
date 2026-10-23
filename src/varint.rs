//! Varint encoding and decoding logic for Protocol Buffers.
//!
//! This module provides basic varint operations including encoding, decoding,
//! and conversion to various protobuf integer types.
//!
//! This is a **reference implementation**. Not optimized for performance.

use crate::wire_format::{MAX_VARINT_BYTES, VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK};
use crate::{ProtobufError, Result};
use ::std::io::Write;

mod read;

// Iterator return types for `read_varints` (re-exported at the crate root).
#[allow(unused_imports)]
pub use read::{
    DecodeOutcome, DecodeState, IteratorExtVarint, ReadExtVarint, TryIteratorExtVarint,
    VarintIteratorFromBytes, VarintIteratorFromTryBytes,
};
/// A deserialized varint value.
///
/// This type represents the decoded 8-byte value from serialized bytes
/// to protobuf integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Varint([u8; 8]);

impl Varint {
    // ============================================================================
    // Converting from / to [u8; 8]
    // ============================================================================

    /// Create a new Varint from raw bytes.
    ///
    /// The bytes given are, essentially, a little-endian encoded u64.
    /// Note that this is NOT the "protobuf encoded" varint bytes.
    pub fn new(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Get the underlying byte array, the little-endian encoded u64.
    /// Note that this is NOT the "protobuf encoded" varint bytes.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    // ============================================================================
    // from / to rust integer types, using certain protobuf integer types formats.
    // ============================================================================

    /// Create a Varint from `u64`, assuming `UInt64` protobuf type.
    pub fn from_uint64(value: u64) -> Self {
        let bytes = value.to_le_bytes();
        Self(bytes)
    }

    /// Create a Varint from `u32`, assuming `UInt32` protobuf type.
    pub fn from_uint32(value: u32) -> Self {
        let bytes = (value as u64).to_le_bytes();
        Self(bytes)
    }

    /// Create a Varint from `i64`, assuming `SInt64` protobuf type.
    pub fn from_sint64(value: i64) -> Self {
        // ZigZag on unsigned bits. Negating `i64::MIN` overflows, so do not use `-value`.
        let zigzag_value = ((value as u64) << 1) ^ ((value >> 63) as u64);
        let bytes = zigzag_value.to_le_bytes();
        Self(bytes)
    }

    /// Create a Varint from `i32`, assuming `SInt32` protobuf type.
    pub fn from_sint32(value: i32) -> Self {
        Self::from_sint64(value as i64)
    }

    /// Create a Varint from `i64`, assuming `Int64` protobuf type.
    pub fn from_int64(value: i64) -> Self {
        let bytes = (value as u64).to_le_bytes();
        Self(bytes)
    }

    /// Create a Varint from `i32`, assuming `Int32` protobuf type.
    pub fn from_int32(value: i32) -> Self {
        let bytes = (value as u64).to_le_bytes();
        Self(bytes)
    }

    /// Create a Varint from `bool`, assuming `Bool` protobuf type.
    pub fn from_bool(value: bool) -> Self {
        let bytes = (if value { 1u64 } else { 0u64 }).to_le_bytes();
        Self(bytes)
    }

    /// Convert to `u64`, assuming `UInt64` protobuf type.
    pub fn to_uint64(&self) -> u64 {
        u64::from_le_bytes(self.0)
    }

    /// Convert to `u32`, assuming `UInt32` protobuf type.
    /// Returns an error if the value is out of range for `u32`.
    pub fn try_to_uint32(&self) -> Result<u32> {
        let value = self.to_uint64();
        u32::try_from(value).map_err(|_| ProtobufError::VarintDowncastOutOfRange {
            value,
            target_type: "u32",
        })
    }

    /// Convert to `i64`, assuming `SInt64` protobuf type.
    pub fn to_sint64(&self) -> i64 {
        let value = self.to_uint64();
        ((value >> 1) as i64) ^ (-((value & 1) as i64))
    }

    /// Convert to `i32`, assuming `SInt32` protobuf type.
    /// Returns an error if the value is out of range for `i32`.
    pub fn try_to_sint32(&self) -> Result<i32> {
        let sint64_value = self.to_sint64();
        i32::try_from(sint64_value).map_err(|_| ProtobufError::VarintDowncastOutOfRange {
            value: sint64_value as u64,
            target_type: "i32",
        })
    }

    /// Convert to `i64`, assuming `Int64` protobuf type.
    pub fn to_int64(&self) -> i64 {
        i64::from_le_bytes(self.0)
    }

    /// Convert to `i32`, assuming `Int32` protobuf type.
    /// Returns an error if the value is out of range for `i32`.
    pub fn try_to_int32(&self) -> Result<i32> {
        let value = self.to_int64();
        i32::try_from(value).map_err(|_| ProtobufError::VarintDowncastOutOfRange {
            value: value as u64,
            target_type: "i32",
        })
    }

    /// Convert to `bool`, assuming `Bool` protobuf type.
    pub fn to_bool(&self) -> bool {
        self.to_uint64() != 0
    }

    // ============================================================================
    // serialization
    // ============================================================================

    /// Get the size of this varint when encoded as a varint.
    ///
    /// This method calculates the exact number of bytes needed to encode
    /// the underlying value as a protobuf varint.
    pub fn varint_size(&self) -> usize {
        let value = self.to_uint64();
        if value == 0 {
            1
        } else {
            (64 - value.leading_zeros() as usize).div_ceil(7)
        }
    }

    /// Encode this varint as a varint and return the bytes with count.
    ///
    /// Returns a tuple of (bytes, count) where:
    /// - bytes: fixed-size array containing the encoded varint
    /// - count: actual number of bytes used (1-MAX_VARINT_BYTES)
    ///
    /// # Example
    /// ```
    /// use ::protobuf_core::Varint;
    ///
    /// let varint = Varint::from_uint64(150);
    /// let (bytes, count) = varint.encode();
    /// assert_eq!(count, 2);
    /// assert_eq!(&bytes[..count], &[0x96, 0x01]);
    /// ```
    pub fn encode(&self) -> ([u8; MAX_VARINT_BYTES], usize) {
        let value = self.to_uint64();
        let mut bytes = [0u8; MAX_VARINT_BYTES];
        let mut bytes_written = 0;
        let mut remaining_value = value;

        for byte in bytes.iter_mut() {
            *byte = (remaining_value & VARINT_PAYLOAD_MASK as u64) as u8;
            remaining_value >>= 7;
            bytes_written += 1;

            if remaining_value == 0 {
                break;
            } else {
                *byte |= VARINT_CONTINUATION_BIT; // continuation bit
            }
        }

        (bytes, bytes_written)
    }
}

/// Extension trait for writing varints to Write instances.
///
/// This trait provides a convenient method to write varints directly to
/// any type that implements `std::io::Write`.
///
/// # Example
/// ```
/// use ::std::io::Write;
/// use ::protobuf_core::{WriteExtVarint, Varint};
///
/// let varint = Varint::from_uint64(150);
/// let mut writer = Vec::new();
/// writer.write_varint(&varint).unwrap();
/// assert_eq!(writer, vec![0x96, 0x01]);
/// ```
pub trait WriteExtVarint {
    /// Write a varint to this writer.
    ///
    /// Encodes a Varint as a varint and writes it to this writer.
    /// Returns the number of bytes written on success.
    ///
    /// # Arguments
    /// * `value` - The Varint to encode and write
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of bytes written
    /// * `Err(::std::io::Error)` - I/O error from the writer
    ///
    /// # Example
    /// ```
    /// use ::std::io::Write;
    /// use ::protobuf_core::{WriteExtVarint, Varint};
    ///
    /// let varint = Varint::from_uint64(150);
    /// let mut buffer = Vec::new();
    /// let bytes_written = buffer.write_varint(&varint).unwrap();
    /// assert_eq!(bytes_written, 2);
    /// assert_eq!(buffer, vec![0x96, 0x01]);
    /// ```
    fn write_varint(&mut self, value: &Varint) -> ::std::io::Result<usize>;
}

impl<W> WriteExtVarint for W
where
    W: Write,
{
    fn write_varint(&mut self, value: &Varint) -> ::std::io::Result<usize> {
        let (bytes, count) = value.encode();
        self.write_all(&bytes[..count])?;
        Ok(count)
    }
}

// ============================================================================
// Tests (Varint type and WriteExtVarint)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::{Varint, WriteExtVarint};
    use crate::wire_format::MAX_VARINT_BYTES;

    #[test]
    fn test_varint_value_creation() {
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0];
        let varint = Varint::new(bytes);
        assert_eq!(varint.as_bytes(), &bytes);
    }

    #[test]
    fn test_varint_conversions() {
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0];
        let varint = Varint::new(bytes);

        assert_eq!(varint.to_uint64(), 406);
        match varint.try_to_uint32() {
            Ok(value) => assert_eq!(value, 406),
            Err(e) => panic!("Expected Ok(406), got error: {:?}", e),
        }

        let varint = Varint::new(bytes);
        assert_eq!(varint.to_sint64(), 203);

        let varint = Varint::new(bytes);
        match varint.try_to_sint32() {
            Ok(value) => assert_eq!(value, 203),
            Err(e) => panic!("Expected Ok(203), got error: {:?}", e),
        }

        let varint = Varint::new(bytes);
        assert!(varint.to_bool());
    }

    #[test]
    fn test_signed_integer_conversions() {
        let bytes = [0x01, 0, 0, 0, 0, 0, 0, 0];
        let varint = Varint::new(bytes);

        assert_eq!(varint.to_sint64(), -1);

        let varint = Varint::new(bytes);
        match varint.try_to_sint32() {
            Ok(value) => assert_eq!(value, -1),
            Err(e) => panic!("Expected Ok(-1), got error: {:?}", e),
        }
    }

    #[test]
    fn test_from_traits() {
        let varint = Varint::from_uint64(150);
        assert_eq!(varint.to_uint64(), 150);

        let varint = Varint::from_uint32(150);
        assert_eq!(varint.to_uint64(), 150);

        let varint = Varint::from_sint64(150);
        assert_eq!(varint.to_sint64(), 150);

        let varint = Varint::from_sint64(-1);
        assert_eq!(varint.to_sint64(), -1);

        let varint = Varint::from_bool(true);
        assert!(varint.to_bool());

        let varint = Varint::from_int32(150);
        assert_eq!(varint.to_int64(), 150);

        let varint = Varint::from_int64(150);
        assert_eq!(varint.to_int64(), 150);
    }

    #[test]
    fn test_sint64_extremes() {
        let min_varint = Varint::from_sint64(i64::MIN);
        assert_eq!(min_varint.to_uint64(), u64::MAX);
        assert_eq!(min_varint.to_sint64(), i64::MIN);
        let (bytes, count) = min_varint.encode();
        assert_eq!(count, 10);
        assert_eq!(
            &bytes[..count],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]
        );

        let max_varint = Varint::from_sint64(i64::MAX);
        assert_eq!(max_varint.to_uint64(), u64::MAX - 1);
        assert_eq!(max_varint.to_sint64(), i64::MAX);

        assert_eq!(Varint::from_sint64(-1).to_uint64(), 1);
        assert_eq!(
            Varint::from_sint32(i32::MIN).to_sint64(),
            i64::from(i32::MIN)
        );
    }

    #[test]
    fn test_integer_wire_edges() {
        use crate::ProtobufError;

        // ZigZag pairs. 0, -1, 1, -2, 2 are the encoding guide's examples.
        let zigzag = [
            (0i64, 0u64),
            (-1, 1),
            (1, 2),
            (-2, 3),
            (2, 4),
            (i64::from(i32::MAX), 4_294_967_294),
            (i64::from(i32::MIN), 4_294_967_295),
            (i64::MAX, u64::MAX - 1),
            (i64::MIN, u64::MAX),
        ];
        for (signed, wire) in zigzag {
            let encoded = Varint::from_sint64(signed);
            assert_eq!(encoded.to_uint64(), wire, "zigzag encode {signed}");
            assert_eq!(encoded.to_sint64(), signed, "zigzag decode {signed}");
            assert_eq!(Varint::from_uint64(wire).to_sint64(), signed);
        }

        // int32/int64 sign-extend. -1 occupies all 64 bits and is 10 bytes on the wire.
        let neg_one = Varint::from_int32(-1);
        assert_eq!(neg_one.to_uint64(), u64::MAX);
        assert_eq!(neg_one.varint_size(), 10);
        assert_eq!(neg_one.try_to_int32().unwrap(), -1);
        assert_eq!(Varint::from_int64(-1).to_int64(), -1);
        assert_eq!(Varint::from_int64(i64::MIN).to_int64(), i64::MIN);
        assert_eq!(
            Varint::from_int32(i32::MAX).try_to_int32().unwrap(),
            i32::MAX
        );

        assert!(!Varint::from_bool(false).to_bool());
        assert!(Varint::from_bool(true).to_bool());
        assert!(Varint::from_uint64(2).to_bool());

        assert_eq!(
            Varint::from_uint32(u32::MAX).try_to_uint32().unwrap(),
            u32::MAX
        );
        let too_wide = Varint::from_uint64(u64::from(u32::MAX) + 1);
        match too_wide.try_to_uint32() {
            Err(ProtobufError::VarintDowncastOutOfRange { value, target_type }) => {
                assert_eq!(value, u64::from(u32::MAX) + 1);
                assert_eq!(target_type, "u32");
            }
            other => panic!("expected u32 downcast error, got {other:?}"),
        }

        assert!(
            Varint::from_int64(i64::from(i32::MAX) + 1)
                .try_to_int32()
                .is_err()
        );
        assert!(
            Varint::from_int64(i64::from(i32::MIN) - 1)
                .try_to_int32()
                .is_err()
        );

        // sint32 range uses the decoded ZigZag integer in the error payload.
        let wide_sint = Varint::from_sint64(i64::from(i32::MAX) + 1);
        match wide_sint.try_to_sint32() {
            Err(ProtobufError::VarintDowncastOutOfRange { value, target_type }) => {
                assert_eq!(target_type, "i32");
                assert_eq!(value, u64::try_from(i64::from(i32::MAX) + 1).unwrap());
            }
            other => panic!("expected sint32 downcast error, got {other:?}"),
        }
        assert_eq!(
            Varint::from_sint32(i32::MAX).try_to_sint32().unwrap(),
            i32::MAX
        );
        assert_eq!(
            Varint::from_sint32(i32::MIN).try_to_sint32().unwrap(),
            i32::MIN
        );
    }

    #[test]
    fn test_to_methods() {
        let bytes = [150, 0, 0, 0, 0, 0, 0, 0];
        let varint = Varint::new(bytes);

        assert_eq!(varint.try_to_uint32().unwrap(), 150);
        assert_eq!(varint.try_to_sint32().unwrap(), 75);
        assert_eq!(varint.to_sint64(), 75);
        assert!(varint.to_bool());
        assert_eq!(varint.try_to_int32().unwrap(), 150);
        assert_eq!(varint.to_int64(), 150);
    }

    #[test]
    fn test_roundtrip_conversions() {
        let original = 150u64;
        let varint = Varint::from_uint64(original);
        assert_eq!(varint.to_uint64(), original);

        let original = 150u32;
        let varint = Varint::from_uint32(original);
        let converted = varint.try_to_uint32().unwrap();
        assert_eq!(converted, original);

        let original = -1i64;
        let varint = Varint::from_sint64(original);
        let converted = varint.to_sint64();
        assert_eq!(converted, original);

        let original = 150i64;
        let varint = Varint::from_int64(original);
        let converted = varint.to_int64();
        assert_eq!(converted, original);

        let original = 150i32;
        let varint = Varint::from_int32(original);
        let converted = varint.try_to_int32().unwrap();
        assert_eq!(converted, original);

        let original = true;
        let varint = Varint::from_bool(original);
        let converted = varint.to_bool();
        assert_eq!(converted, original);
    }

    #[test]
    fn test_encode_varint() {
        let varint = Varint::from_uint64(150);
        let (bytes, count) = varint.encode();
        assert_eq!(count, 2);
        assert_eq!(&bytes[..count], &[0x96, 0x01]);

        let varint = Varint::from_uint64(127);
        let (bytes, count) = varint.encode();
        assert_eq!(count, 1);
        assert_eq!(&bytes[..count], &[0x7F]);

        let varint = Varint::from_uint64(0);
        let (bytes, count) = varint.encode();
        assert_eq!(count, 1);
        assert_eq!(&bytes[..count], &[0x00]);

        let varint = Varint::from_uint64(0x7FFFFFFFFFFFFFFF);
        let (bytes, count) = varint.encode();
        assert_eq!(count, 9);
        assert_eq!(
            &bytes[..count],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]
        );

        let varint = Varint::from_uint64(0xFFFFFFFFFFFFFFFF);
        let (bytes, count) = varint.encode();
        assert_eq!(count, MAX_VARINT_BYTES);
        assert_eq!(
            &bytes[..count],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]
        );
    }

    #[test]
    fn test_write_varint() {
        let varint = Varint::from_uint64(150);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, 2);
        assert_eq!(buffer, vec![0x96, 0x01]);

        let varint = Varint::from_uint64(127);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, 1);
        assert_eq!(buffer, vec![0x7F]);

        let varint = Varint::from_uint64(0);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, 1);
        assert_eq!(buffer, vec![0x00]);

        let varint = Varint::from_uint64(0x7FFFFFFFFFFFFFFF);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, 9);

        let varint = Varint::from_uint64(0xFFFFFFFFFFFFFFFF);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, MAX_VARINT_BYTES);
    }

    #[test]
    fn test_all_encoding_methods_consistency() {
        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            let varint = Varint::from_uint64(value);
            let (array_bytes, array_count) = varint.encode();

            let varint2 = Varint::from_uint64(value);
            let mut vec_buffer = Vec::new();
            let vec_count = vec_buffer.write_varint(&varint2).unwrap();

            assert_eq!(array_count, vec_count);
            assert_eq!(&array_bytes[..array_count], &vec_buffer[..]);
        }
    }
}
