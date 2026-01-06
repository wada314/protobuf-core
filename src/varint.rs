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

//! Varint encoding and decoding logic for Protocol Buffers.
//!
//! This module provides basic varint operations including encoding, decoding,
//! and conversion to various protobuf integer types.
//!
//! This is a **reference implementation**. Not optimized for performance.

use crate::slice::SliceAdvance;
use crate::wire_format::{MAX_VARINT_BYTES, VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK};
use crate::{ProtobufError, Result};
use ::std::convert::{AsRef, Infallible, TryFrom};
use ::std::io::{Read, Write};

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
    pub fn new(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Get the underlying byte array, the little-endian encoded u64.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    // ============================================================================
    // from / to protobuf integer types
    // Important: Intending NOT from / to the Rust's integer types,
    // but from / to the protobuf integer types!
    // Thus, we don't provide the `From` / `Into` traits for the Rust's integer types.
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
        let zigzag_value = if value < 0 {
            ((-value) as u64) * 2 - 1
        } else {
            (value as u64) * 2
        };
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
            (64 - value.leading_zeros() as usize + 6) / 7
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

/// Decode a varint from a sequence of bytes.
///
/// This is a helper function that implements the core varint decoding logic.
/// It reads bytes from the iterator until it finds a byte with the continuation bit cleared.
///
/// Returns `Ok(Some(Varint))` if successfully decoded.
/// Returns `Ok(None)` if no bytes were read (empty iterator).
/// Returns `Err(ProtobufError::VarintTooLong)` if the varint exceeds MAX_VARINT_BYTES.
/// Returns `Err(ProtobufError)` if an error occurs while reading bytes (the error type `E` is converted via `Into<ProtobufError>`).
fn decode_varint_from_bytes<I, E>(bytes: I) -> Result<Option<Varint>>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    let mut decoded_value = 0u64;
    let mut shift = 0;
    let mut has_data = false;

    for byte_result in bytes.take(MAX_VARINT_BYTES) {
        let byte = byte_result.map_err(Into::into)?;
        has_data = true;

        let value = (byte & VARINT_PAYLOAD_MASK) as u64;
        decoded_value |= value << shift;

        if byte & VARINT_CONTINUATION_BIT == 0 {
            let result_bytes = decoded_value.to_le_bytes();
            return Ok(Some(Varint::new(result_bytes)));
        }
        shift += 7;
    }

    if !has_data {
        return Ok(None);
    }

    Err(ProtobufError::VarintTooLong)
}

/// Extension trait for collecting varints from byte iterators.
///
/// This trait provides convenient methods to collect varints directly from
/// any iterator that yields bytes.
///
/// # Example
/// ```
/// use ::protobuf_core::{IteratorExtVarint, Varint};
///
/// let bytes = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut iter = bytes.into_iter();
/// let varint: Option<Varint> = iter.try_collect_varint().unwrap();
/// assert_eq!(varint.unwrap().to_uint64(), 150);
/// ```
pub trait IteratorExtVarint {
    /// Collect a varint from this iterator.
    ///
    /// Returns the Varint `Ok(Some(varint))` if successfully read.
    /// Returns `Ok(None)` if no input is available (empty iterator).
    /// Returns `Err(ProtobufError::VarintTooLong)` if the varint exceeds MAX_VARINT_BYTES.
    fn try_collect_varint(self) -> Result<Option<Varint>>;
}

impl<I> IteratorExtVarint for I
where
    I: Iterator<Item = u8>,
{
    fn try_collect_varint(self) -> Result<Option<Varint>> {
        decode_varint_from_bytes(self.map(|b| Ok::<u8, Infallible>(b)))
    }
}

/// Extension trait for collecting varints from byte iterators that yield `Result<u8, E>`.
///
/// This trait provides convenient methods to collect varints directly from
/// any iterator that yields `Result<u8, E>`, allowing proper error propagation.
/// The error type `E` must implement `Into<ProtobufError>`.
///
/// # Example
/// ```
/// use ::std::io::{Cursor, Read};
/// use ::protobuf_core::{TryIteratorExtVarint, Varint};
///
/// let data = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut reader = Cursor::new(data);
/// let iter = reader.bytes(); // Iterator<Item = Result<u8, io::Error>>
/// let varint = iter.try_collect_varint().unwrap().unwrap();
/// assert_eq!(varint.to_uint64(), 150);
/// ```
pub trait TryIteratorExtVarint {
    /// Collect a varint from this iterator.
    ///
    /// Returns the Varint `Ok(Some(varint))` if successfully read.
    /// Returns `Ok(None)` if no input is available (empty iterator).
    /// Returns `Err(ProtobufError::VarintTooLong)` if the varint exceeds MAX_VARINT_BYTES.
    /// Returns `Err(ProtobufError)` if an error occurs while reading bytes (the error type `E` is converted via `Into<ProtobufError>`).
    fn try_collect_varint(self) -> Result<Option<Varint>>;
}

impl<I, E> TryIteratorExtVarint for I
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn try_collect_varint(self) -> Result<Option<Varint>> {
        decode_varint_from_bytes(self)
    }
}

/// Extension trait for reading varints from Read instances.
///
/// This trait provides a convenient method to read varints directly from
/// any type that implements `std::io::Read`.
///
/// # Example
/// ```
/// use ::std::io::Cursor;
/// use ::protobuf_core::{ReadExtVarint, Varint};
///
/// let data = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut reader = Cursor::new(data);
/// let varint = reader.read_varint().unwrap().unwrap();
/// assert_eq!(varint.to_uint64(), 150);
/// ```
pub trait ReadExtVarint {
    /// Read a varint from this reader.
    ///
    /// Returns the Varint `Ok(Some(varint))` if successfully read.
    /// Returns `Ok(None)` if no input is available (EOF).
    /// Returns `Err(ProtobufError::VarintTooLong)` if the varint exceeds MAX_VARINT_BYTES.
    /// Returns `Err(ProtobufError::IoError)` if an I/O error occurs.
    fn read_varint(&mut self) -> Result<Option<Varint>>;
}

impl<R> ReadExtVarint for R
where
    R: Read,
{
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        // Use Bytes iterator which returns Iterator<Item = Result<u8, io::Error>>
        // decode_varint_from_bytes will handle errors and stop when varint ends
        decode_varint_from_bytes(self.bytes().take(MAX_VARINT_BYTES))
    }
}

/// Extension trait for reading varints from slices.
///
/// This trait provides a convenient method to read varints directly from
/// byte slices. The slice is automatically advanced after reading.
///
/// # Example
/// ```
/// use ::protobuf_core::{SliceExtVarint, Varint};
///
/// let data = [0x96, 0x01]; // 150 in varint encoding
/// let mut slice = &data[..];
/// let varint = slice.read_varint().unwrap().unwrap();
/// assert_eq!(varint.to_uint64(), 150);
/// ```
pub trait SliceExtVarint {
    /// Read a varint from this slice.
    ///
    /// The slice is automatically advanced to point to the byte after the varint.
    /// To get the number of bytes consumed, compare the slice length before and after.
    ///
    /// Returns `Ok(Some(varint))` if successfully read.
    /// Returns `Ok(None)` if no input is available (empty slice).
    /// Returns `Err(ProtobufError::VarintTooLong)` if the varint exceeds MAX_VARINT_BYTES.
    fn read_varint(&mut self) -> Result<Option<Varint>>;
}

impl<S> SliceExtVarint for S
where
    S: AsRef<[u8]> + SliceAdvance,
{
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        let slice = self.as_ref();
        if slice.is_empty() {
            return Ok(None);
        }

        // Track the number of bytes read by enumerating the iterator
        let mut bytes_read = 0;
        let mut iter = slice.iter().copied().enumerate();
        let result = decode_varint_from_bytes(
            iter.by_ref()
                .map(|(idx, b)| {
                    bytes_read = idx + 1;
                    Ok::<u8, Infallible>(b)
                })
                .take(MAX_VARINT_BYTES),
        )?;

        match result {
            Some(varint) => {
                // Advance using SliceAdvance
                self.advance(bytes_read);
                Ok(Some(varint))
            }
            None => Ok(None),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_value_creation() {
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0];
        let varint = Varint::new(bytes);
        assert_eq!(varint.as_bytes(), &bytes);
    }

    #[test]
    fn test_varint_conversions() {
        // 406 in little-endian: 0x96, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        let bytes = [0x96, 0x01, 0, 0, 0, 0, 0, 0];
        let varint = Varint::new(bytes);

        // Test all integer conversions
        assert_eq!(varint.to_uint64(), 406);
        match varint.try_to_uint32() {
            Ok(value) => assert_eq!(value, 406),
            Err(e) => panic!("Expected Ok(406), got error: {:?}", e),
        }
        let varint = Varint::new(bytes); // Create new varint for next test
        // 406 in ZigZag encoding represents 203 in signed value
        assert_eq!(varint.to_sint64(), 203);
        let varint = Varint::new(bytes); // Create new varint for next test
        match varint.try_to_sint32() {
            Ok(value) => assert_eq!(value, 203),
            Err(e) => panic!("Expected Ok(203), got error: {:?}", e),
        }
        let varint = Varint::new(bytes); // Create new varint for next test
        assert_eq!(varint.to_bool(), true);
    }

    #[test]
    fn test_signed_integer_conversions() {
        // -1 in ZigZag encoding: 1
        let bytes = [0x01, 0, 0, 0, 0, 0, 0, 0];
        let varint = Varint::new(bytes);

        assert_eq!(varint.to_sint64(), -1);
        let varint = Varint::new(bytes); // Create new varint for next test
        match varint.try_to_sint32() {
            Ok(value) => assert_eq!(value, -1),
            Err(e) => panic!("Expected Ok(-1), got error: {:?}", e),
        }
    }

    #[test]
    fn test_read_varint_from_iterator() {
        let input = [0x96, 0x01];
        let iter = input.iter().copied();
        let varint = iter.try_collect_varint().unwrap().unwrap();

        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_read_varint_from_reader() {
        use ::std::io::Cursor;
        let input = [0x96, 0x01];
        let mut reader = Cursor::new(input);
        let varint = reader.read_varint().unwrap().unwrap();

        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_read_ext_varint_trait() {
        use ::std::io::Cursor;
        let input = [0x96, 0x01];
        let mut reader = Cursor::new(input);
        let varint = reader.read_varint().unwrap().unwrap();

        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_write_ext_varint_trait() {
        let varint = Varint::from_uint64(150);
        let mut writer = Vec::new();
        let bytes_written = writer.write_varint(&varint).unwrap();

        assert_eq!(bytes_written, 2);
        assert_eq!(writer, vec![0x96, 0x01]);
    }

    #[test]
    fn test_iterator_ext_varint_trait() {
        let bytes = vec![0x96, 0x01]; // 150 in varint encoding
        let iter = bytes.into_iter();
        let varint = iter.try_collect_varint().unwrap().unwrap();

        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_iterator_ext_varint_empty() {
        use super::IteratorExtVarint;

        let bytes = vec![];
        let iter = bytes.into_iter();
        let varint = IteratorExtVarint::try_collect_varint(iter).unwrap();

        assert_eq!(varint, None);
    }

    #[test]
    fn test_write_varint() {
        // Test encoding small values
        let varint = Varint::from_uint64(150);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, 2);
        assert_eq!(buffer, vec![0x96, 0x01]);

        // Test encoding single-byte values
        let varint = Varint::from_uint64(127);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, 1);
        assert_eq!(buffer, vec![0x7F]);

        // Test encoding zero
        let varint = Varint::from_uint64(0);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, 1);
        assert_eq!(buffer, vec![0x00]);

        // Test encoding large values
        let varint = Varint::from_uint64(0x7FFFFFFFFFFFFFFF);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, 9); // 9-byte varint

        // Test encoding maximum varint (MAX_VARINT_BYTES bytes)
        let varint = Varint::from_uint64(0xFFFFFFFFFFFFFFFF);
        let mut buffer = Vec::new();
        let bytes_written = buffer.write_varint(&varint).unwrap();
        assert_eq!(bytes_written, MAX_VARINT_BYTES); // Maximum varint size
    }

    #[test]
    fn test_write_varint_roundtrip() {
        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            // Create Varint from the test value
            let varint = Varint::from_uint64(value);

            let mut buffer = Vec::new();
            buffer.write_varint(&varint).unwrap();

            let iter = buffer.iter().copied();
            let decoded_varint = iter.try_collect_varint().unwrap().unwrap();
            let decoded_value = decoded_varint.to_uint64();

            assert_eq!(decoded_value, value, "Roundtrip failed for value {}", value);
        }
    }

    #[test]
    fn test_encode_varint() {
        // Test encoding small values
        let varint = Varint::from_uint64(150);
        let (bytes, count) = varint.encode();
        assert_eq!(count, 2);
        assert_eq!(&bytes[..count], &[0x96, 0x01]);

        // Test encoding single-byte values
        let varint = Varint::from_uint64(127);
        let (bytes, count) = varint.encode();
        assert_eq!(count, 1);
        assert_eq!(&bytes[..count], &[0x7F]);

        // Test encoding zero
        let varint = Varint::from_uint64(0);
        let (bytes, count) = varint.encode();
        assert_eq!(count, 1);
        assert_eq!(&bytes[..count], &[0x00]);

        // Test encoding large values
        let varint = Varint::from_uint64(0x7FFFFFFFFFFFFFFF);
        let (bytes, count) = varint.encode();
        assert_eq!(count, 9);
        assert_eq!(
            &bytes[..count],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]
        );

        // Test encoding maximum varint (MAX_VARINT_BYTES bytes)
        let varint = Varint::from_uint64(0xFFFFFFFFFFFFFFFF);
        let (bytes, count) = varint.encode();
        assert_eq!(count, MAX_VARINT_BYTES);
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
            let varint = Varint::from_uint64(value);
            let (array_bytes, array_count) = varint.encode();

            // Method 2: write_varint (std::io::Write)
            let varint2 = Varint::from_uint64(value);
            let mut vec_buffer = Vec::new();
            let vec_count = vec_buffer.write_varint(&varint2).unwrap();

            // Both methods should produce the same result
            assert_eq!(array_count, vec_count);
            assert_eq!(&array_bytes[..array_count], &vec_buffer[..]);
        }
    }

    #[test]
    fn test_from_traits() {
        // Test From<u64> for Varint
        let varint = Varint::from_uint64(150);
        assert_eq!(varint.to_uint64(), 150);

        // Test from_uint32
        let varint = Varint::from_uint32(150);
        assert_eq!(varint.to_uint64(), 150);

        // Test from_sint64
        let varint = Varint::from_sint64(150);
        assert_eq!(varint.to_sint64(), 150);

        // Test from_sint64 with negative value
        let varint = Varint::from_sint64(-1);
        assert_eq!(varint.to_sint64(), -1);

        // Test from_bool
        let varint = Varint::from_bool(true);
        assert_eq!(varint.to_bool(), true);

        // Test from_int32 (non-ZigZag)
        let varint = Varint::from_int32(150);
        assert_eq!(varint.to_int64(), 150);

        // Test from_int64 (non-ZigZag)
        let varint = Varint::from_int64(150);
        assert_eq!(varint.to_int64(), 150);
    }

    #[test]
    fn test_to_methods() {
        let bytes = [150, 0, 0, 0, 0, 0, 0, 0]; // 150 in little-endian
        let varint = Varint::new(bytes);

        // Test to_uint32
        assert_eq!(varint.try_to_uint32().unwrap(), 150);

        // Test to_sint32 (150 in ZigZag encoding represents 75 in signed value)
        assert_eq!(varint.try_to_sint32().unwrap(), 75);

        // Test to_sint64 (150 in ZigZag encoding represents 75 in signed value)
        assert_eq!(varint.to_sint64(), 75);

        // Test to_bool
        assert_eq!(varint.to_bool(), true);

        // Test to_int32 (non-ZigZag)
        assert_eq!(varint.try_to_int32().unwrap(), 150);

        // Test to_int64 (non-ZigZag)
        assert_eq!(varint.to_int64(), 150);
    }

    #[test]
    fn test_roundtrip_conversions() {
        // Test roundtrip for u64
        let original = 150u64;
        let varint = Varint::from_uint64(original);
        assert_eq!(varint.to_uint64(), original);

        // Test roundtrip for u32
        let original = 150u32;
        let varint = Varint::from_uint32(original);
        let converted = varint.try_to_uint32().unwrap();
        assert_eq!(converted, original);

        // Test roundtrip for i64 (ZigZag)
        let original = -1i64;
        let varint = Varint::from_sint64(original);
        let converted = varint.to_sint64();
        assert_eq!(converted, original);

        // Test roundtrip for i64 (non-ZigZag)
        let original = 150i64;
        let varint = Varint::from_int64(original);
        let converted = varint.to_int64();
        assert_eq!(converted, original);

        // Test roundtrip for i32 (non-ZigZag)
        let original = 150i32;
        let varint = Varint::from_int32(original);
        let converted = varint.try_to_int32().unwrap();
        assert_eq!(converted, original);

        // Test roundtrip for bool
        let original = true;
        let varint = Varint::from_bool(original);
        let converted = varint.to_bool();
        assert_eq!(converted, original);
    }

    #[test]
    fn test_try_iterator_ext_varint() {
        use super::TryIteratorExtVarint;
        use ::std::io::Cursor;

        let data = vec![0x96, 0x01]; // 150 in varint encoding
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let varint = TryIteratorExtVarint::try_collect_varint(iter)
            .unwrap()
            .unwrap();
        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_try_iterator_ext_varint_empty() {
        use super::TryIteratorExtVarint;
        use ::std::io::Cursor;

        let data = vec![];
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let varint = TryIteratorExtVarint::try_collect_varint(iter).unwrap();
        assert_eq!(varint, None);
    }
}
