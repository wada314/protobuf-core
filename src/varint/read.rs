//! Varint decoding (reading) from `std::io::Read`.
//!
//! The reader's EOF (or an existing [`std::io::Take`] bound) is the byte limit;
//! methods do not take a separate `len` argument. Callers that receive input in
//! chunks should buffer until a terminator or [`MAX_VARINT_BYTES`] bytes are
//! available, then call the complete-read APIs.

use crate::wire_format::{MAX_VARINT_BYTES, VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK};
use crate::{ProtobufError, Result};
use ::std::io::Read;

use super::Varint;

/// Iterator that reads varints from a `Read` until soft EOF.
///
/// Yields [`Result<Varint>`] for each varint. Soft-stops when the reader has no
/// bytes left; a truncated mid-varint is [`ProtobufError::UnexpectedEof`].
/// Returned by [`ReadExtVarint::read_varints`].
pub struct VarintIterFromRead<R> {
    reader: R,
}

impl<R: Read> Iterator for VarintIterFromRead<R> {
    type Item = Result<Varint>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = [0u8; MAX_VARINT_BYTES];
        match try_feed_varint_with_raw(&mut self.reader, &mut buf) {
            Ok(None) => None,
            Ok(Some((v, _))) => Some(Ok(v)),
            Err(e) => Some(Err(e)),
        }
    }
}

// ============================================================================
// ReadExtVarint
// ============================================================================

/// Decode one varint from `reader`, copying every consumed wire byte into `buf`.
///
/// - `Ok(None)` — soft empty (no bytes available)
/// - `Ok(Some((varint, raw)))` — complete value; `raw` is `&buf[..n]`
/// - `Err(UnexpectedEof)` — started a varint but ran out of bytes
/// - other `Err` — invalid encoding
#[inline]
#[allow(clippy::unbuffered_bytes)] // Varint needs byte-at-a-time; prefer BufReader upstream.
fn try_feed_varint_with_raw<'a, R: Read>(
    reader: &'a mut R,
    buf: &'a mut [u8; MAX_VARINT_BYTES],
) -> Result<Option<(Varint, &'a [u8])>> {
    let mut decoded_value = 0u64;
    let mut shift = 0u32;
    let mut n = 0usize;

    for byte_result in reader.by_ref().bytes().take(MAX_VARINT_BYTES) {
        let byte = byte_result?;
        buf[n] = byte;
        n += 1;

        let value = (byte & VARINT_PAYLOAD_MASK) as u64;
        // Bytes 1-9 contribute 7 bits each. The 10th byte may only set bit 63.
        if shift >= 64 {
            return Err(ProtobufError::VarintTooLong);
        }
        let bits_left = 64 - shift;
        if bits_left < 7 && value >> bits_left != 0 {
            return Err(ProtobufError::VarintTooLong);
        }
        decoded_value |= value << shift;

        if byte & VARINT_CONTINUATION_BIT == 0 {
            let result_bytes = decoded_value.to_le_bytes();
            return Ok(Some((Varint::new(result_bytes), &buf[..n])));
        }
        shift += 7;
    }

    if n == 0 {
        return Ok(None);
    }
    if n >= MAX_VARINT_BYTES {
        return Err(ProtobufError::VarintTooLong);
    }
    Err(ProtobufError::UnexpectedEof)
}

/// Extension trait for reading varints from [`Read`] instances.
///
/// Reads until the reader's EOF (or an existing [`std::io::Take`] bound).
/// Methods do not take a separate byte-budget argument.
///
/// - [`read_varint_with_raw`](Self::read_varint_with_raw) /
///   [`read_varint`](Self::read_varint) — complete reads; empty or truncated
///   input is [`UnexpectedEof`](ProtobufError::UnexpectedEof) (check a length
///   bound / `remaining` first for soft end)
/// - [`read_varints`](Self::read_varints) — soft-stops when no bytes remain;
///   truncated mid-varint is [`UnexpectedEof`](ProtobufError::UnexpectedEof)
///
/// Chunked input: buffer until a terminating byte or [`MAX_VARINT_BYTES`] are
/// available, then call a complete-read method. There is no mid-varint resume
/// state; fixed-width wire types already follow the same model.
///
/// # Performance Note
///
/// When reading from file handles or network streams, consider wrapping the reader
/// with [`BufReader`](std::io::BufReader) to avoid inefficient byte-by-byte system calls:
///
/// ```no_run
/// use std::io::{BufReader, Read};
/// use protobuf_core::ReadExtVarint;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let file = std::fs::File::open("data.bin")?;
/// let mut reader = BufReader::new(file);
/// let varint = reader.read_varint()?;
/// # let _ = varint;
/// # Ok(())
/// # }
/// ```
///
/// # Example
/// ```
/// use ::std::io::Cursor;
/// use ::protobuf_core::ReadExtVarint;
///
/// let data = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut reader = Cursor::new(data);
/// let varint = reader.read_varint().unwrap();
/// assert_eq!(varint.to_uint64(), 150);
/// ```
pub trait ReadExtVarint: Read {
    /// Read a varint and copy its exact wire bytes into `buf`.
    ///
    /// Returns `(varint, raw)` where `raw` is `&buf[..n]` (including
    /// non-canonical encodings). Empty or truncated input is
    /// [`UnexpectedEof`](ProtobufError::UnexpectedEof); check a length bound
    /// first for soft end.
    ///
    /// # Example
    /// ```
    /// use ::std::io::Cursor;
    /// use ::protobuf_core::{ReadExtVarint, MAX_VARINT_BYTES};
    ///
    /// let mut reader = Cursor::new(vec![0x96, 0x01, 0x08]);
    /// let mut buf = [0u8; MAX_VARINT_BYTES];
    /// let (varint, raw) = reader.read_varint_with_raw(&mut buf).unwrap();
    /// assert_eq!(varint.to_uint64(), 150);
    /// assert_eq!(raw, &[0x96, 0x01]);
    /// ```
    #[inline]
    fn read_varint_with_raw<'a>(
        &'a mut self,
        buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'a [u8])>
    where
        Self: Sized,
    {
        try_feed_varint_with_raw(self, buf)?.ok_or(ProtobufError::UnexpectedEof)
    }

    /// Read a varint until a complete value.
    ///
    /// Via [`read_varint_with_raw`](Self::read_varint_with_raw) (wire bytes
    /// discarded). Empty or truncated input is
    /// [`UnexpectedEof`](ProtobufError::UnexpectedEof).
    #[inline]
    fn read_varint(&mut self) -> Result<Varint>
    where
        Self: Sized,
    {
        let mut buf = [0u8; MAX_VARINT_BYTES];
        Ok(self.read_varint_with_raw(&mut buf)?.0)
    }

    /// Create an iterator that reads multiple varints until soft EOF.
    ///
    /// Stops when the reader yields no bytes; a truncated mid-varint is
    /// [`UnexpectedEof`](ProtobufError::UnexpectedEof).
    ///
    /// # Example
    /// ```
    /// use ::std::io::Cursor;
    /// use ::protobuf_core::{ReadExtVarint, Varint};
    ///
    /// let data = vec![0x96, 0x01, 0x7F]; // 150 and 127 in varint encoding
    /// let mut reader = Cursor::new(data);
    /// let varints: Vec<Varint> = reader
    ///     .read_varints()
    ///     .collect::<Result<Vec<_>, _>>()
    ///     .unwrap();
    /// assert_eq!(varints.len(), 2);
    /// assert_eq!(varints[0].to_uint64(), 150);
    /// assert_eq!(varints[1].to_uint64(), 127);
    /// ```
    #[inline]
    fn read_varints(&mut self) -> VarintIterFromRead<&mut Self>
    where
        Self: Sized,
    {
        VarintIterFromRead { reader: self }
    }
}

impl<R: Read> ReadExtVarint for R {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::ReadExtVarint;
    use crate::ProtobufError;
    use crate::varint::Varint;

    #[test]
    fn test_read_ext_varint_from_slice() {
        let mut slice = &[0x96u8, 0x01][..];
        let varint = slice.read_varint().unwrap();
        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_read_varint_with_raw_preserves_noncanonical_bytes() {
        use crate::wire_format::MAX_VARINT_BYTES;
        use ::std::io::Cursor;

        // Overlong encoding of 1: 0x81 0x00
        let mut reader = Cursor::new(vec![0x81, 0x00, 0x08]);
        let mut buf = [0u8; MAX_VARINT_BYTES];
        let (varint, raw) = reader.read_varint_with_raw(&mut buf).unwrap();
        assert_eq!(varint.to_uint64(), 1);
        assert_eq!(raw, &[0x81, 0x00]);
        assert_eq!(reader.read_varint().unwrap().to_uint64(), 8);
    }

    #[test]
    fn test_read_varint_with_raw_on_slice_via_read_ext() {
        use crate::wire_format::MAX_VARINT_BYTES;

        let mut slice: &[u8] = &[0x81, 0x00, 0x08];
        let mut buf = [0u8; MAX_VARINT_BYTES];
        let (varint, raw) = slice.read_varint_with_raw(&mut buf).unwrap();
        assert_eq!(varint.to_uint64(), 1);
        assert_eq!(raw, &[0x81, 0x00]);
        assert_eq!(slice, &[0x08]);
    }

    #[test]
    fn test_read_varint_with_raw_empty_is_unexpected_eof() {
        use crate::wire_format::MAX_VARINT_BYTES;
        use ::std::io::Cursor;

        let mut reader = Cursor::new(Vec::<u8>::new());
        let mut buf = [0u8; MAX_VARINT_BYTES];
        assert!(matches!(
            reader.read_varint_with_raw(&mut buf),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn test_read_varint_with_raw_incomplete_is_unexpected_eof() {
        use crate::wire_format::MAX_VARINT_BYTES;
        use ::std::io::Cursor;

        let mut reader = Cursor::new(vec![0x80]);
        let mut buf = [0u8; MAX_VARINT_BYTES];
        assert!(matches!(
            reader.read_varint_with_raw(&mut buf),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn test_read_ext_varint_from_empty_slice() {
        let mut slice: &[u8] = &[];
        assert!(matches!(
            slice.read_varint(),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn test_read_ext_varint_from_slice_incomplete() {
        let mut slice = &[0x80u8][..];
        assert!(matches!(
            slice.read_varint(),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn test_read_ext_varint_short_slice_incomplete() {
        // Full encoding of 150 is [0x96, 0x01]; a one-byte view yields UnexpectedEof.
        let bytes = [0x96u8, 0x01];
        let mut slice = &bytes[..1];
        assert!(matches!(
            slice.read_varint(),
            Err(ProtobufError::UnexpectedEof)
        ));
        assert!(slice.is_empty());
    }

    #[test]
    fn test_read_ext_varint_trait() {
        use ::std::io::Cursor;

        let input = [0x96, 0x01];
        let mut reader = Cursor::new(input);
        let varint = reader.read_varint().unwrap();
        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_read_ext_varint_empty() {
        use ::std::io::Cursor;

        let mut reader = Cursor::new(vec![]);
        assert!(matches!(
            reader.read_varint(),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn test_read_ext_varint_read_varints() {
        use ::std::io::Cursor;

        let data = vec![0x96, 0x01, 0x7F, 0x01];
        let mut reader = Cursor::new(data);
        let varints: Vec<Varint> = reader
            .read_varints()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(varints.len(), 3);
        assert_eq!(varints[0].to_uint64(), 150);
        assert_eq!(varints[1].to_uint64(), 127);
        assert_eq!(varints[2].to_uint64(), 1);
    }

    #[test]
    fn test_read_ext_varint_read_varints_bounded_with_take() {
        use ::std::io::{Cursor, Read};

        // Two complete varints; Take covers only the first (2 bytes).
        let data = vec![0x96, 0x01, 0x7F];
        let mut reader = Cursor::new(data);
        let varints: Vec<Varint> = reader
            .by_ref()
            .take(2)
            .read_varints()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(varints.len(), 1);
        assert_eq!(varints[0].to_uint64(), 150);
        assert_eq!(reader.position(), 2);
    }

    #[test]
    fn test_read_ext_varint_read_varints_truncated_mid_is_error() {
        use ::std::io::Cursor;

        let mut reader = Cursor::new(vec![0x96, 0x01, 0x80]);
        let result: Result<Vec<_>, _> = reader.read_varints().collect();
        assert!(matches!(result, Err(ProtobufError::UnexpectedEof)));
    }

    #[test]
    fn test_read_ext_varint_read_varints_empty_is_empty() {
        use ::std::io::Cursor;

        let mut reader = Cursor::new(Vec::<u8>::new());
        let varints: Vec<Varint> = reader
            .read_varints()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(varints.is_empty());
    }

    #[test]
    fn test_write_varint_roundtrip() {
        use crate::varint::WriteExtVarint;
        use ::std::io::Cursor;

        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            let varint = Varint::from_uint64(value);

            let mut buffer = Vec::new();
            buffer.write_varint(&varint).unwrap();

            let mut reader = Cursor::new(buffer);
            let decoded_varint = reader.read_varint().unwrap();
            let decoded_value = decoded_varint.to_uint64();

            assert_eq!(decoded_value, value, "Roundtrip failed for value {}", value);
        }
    }

    #[test]
    fn test_read_varint_tenth_byte_overflow() {
        use ::std::io::Cursor;

        // Nine continuation bytes, then a payload that does not fit in bit 63.
        let bytes = vec![0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x02];
        let mut reader = Cursor::new(bytes);
        let result = reader.read_varint();
        assert!(matches!(result, Err(ProtobufError::VarintTooLong)));

        let bytes = vec![0x80u8; 10];
        let mut reader = Cursor::new(bytes);
        let result = reader.read_varint();
        assert!(matches!(result, Err(ProtobufError::VarintTooLong)));
    }

    #[test]
    fn test_read_varint_u64_max_and_overlong_zero() {
        use ::std::io::Cursor;

        let bytes = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];
        let mut reader = Cursor::new(bytes);
        let varint = reader.read_varint().unwrap();
        assert_eq!(varint.to_uint64(), u64::MAX);

        // Overlong encoding of 0: the 10th byte contributes no bits. Parsers accept it.
        let bytes = vec![0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
        let mut reader = Cursor::new(bytes);
        let varint = reader.read_varint().unwrap();
        assert_eq!(varint.to_uint64(), 0);
    }

    #[test]
    fn test_varint_length_boundaries_roundtrip() {
        use crate::varint::WriteExtVarint;
        use crate::wire_format::{
            MAX_1_BYTE_VARINT, MAX_2_BYTE_VARINT, MAX_3_BYTE_VARINT, MAX_4_BYTE_VARINT,
            MAX_5_BYTE_VARINT, MAX_6_BYTE_VARINT, MAX_7_BYTE_VARINT, MAX_8_BYTE_VARINT,
            MAX_9_BYTE_VARINT,
        };
        use ::std::io::Cursor;

        let bounds = [
            (0u64, 1usize),
            (MAX_1_BYTE_VARINT, 1),
            (MAX_1_BYTE_VARINT + 1, 2),
            (MAX_2_BYTE_VARINT, 2),
            (MAX_2_BYTE_VARINT + 1, 3),
            (MAX_3_BYTE_VARINT, 3),
            (MAX_3_BYTE_VARINT + 1, 4),
            (MAX_4_BYTE_VARINT, 4),
            (MAX_4_BYTE_VARINT + 1, 5),
            (MAX_5_BYTE_VARINT, 5),
            (MAX_5_BYTE_VARINT + 1, 6),
            (MAX_6_BYTE_VARINT, 6),
            (MAX_6_BYTE_VARINT + 1, 7),
            (MAX_7_BYTE_VARINT, 7),
            (MAX_7_BYTE_VARINT + 1, 8),
            (MAX_8_BYTE_VARINT, 8),
            (MAX_8_BYTE_VARINT + 1, 9),
            (MAX_9_BYTE_VARINT, 9),
            (MAX_9_BYTE_VARINT + 1, 10),
            (u64::MAX, 10),
        ];

        for (value, size) in bounds {
            let varint = Varint::from_uint64(value);
            assert_eq!(varint.varint_size(), size, "size of {value:#x}");
            let (bytes, count) = varint.encode();
            assert_eq!(count, size, "encode length of {value:#x}");

            let mut buffer = Vec::new();
            assert_eq!(buffer.write_varint(&varint).unwrap(), size);
            assert_eq!(buffer, bytes[..count]);

            let mut reader = Cursor::new(buffer);
            let decoded = reader.read_varint().unwrap();
            assert_eq!(decoded.to_uint64(), value, "roundtrip {value:#x}");
        }
    }

    #[test]
    fn test_read_varint_truncated() {
        use ::std::io::Cursor;

        let mut reader = Cursor::new(vec![0x80u8]);
        assert!(matches!(
            reader.read_varint(),
            Err(ProtobufError::UnexpectedEof)
        ));
    }
}
