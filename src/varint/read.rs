//! Varint decoding (reading) from `std::io::Read`.
//!
//! This module provides extension traits and types for reading varints from
//! readers, including partial decoding and resume support. The reader's EOF
//! (or an existing [`std::io::Take`] bound) is the byte limit; methods do not
//! take a separate `len` argument.

use crate::wire_format::{MAX_VARINT_BYTES, VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK};
use crate::{ProtobufError, Result};
use ::std::io::Read;
use ::std::iter::Iterator;

use super::Varint;

/// Parser state when varint decoding runs out of bytes before completing.
///
/// Returned in [`DecodeOutcome::Incomplete`]. Use with [`read_varint_resume`] to continue parsing.
///
/// [`read_varint_resume`]: ReadExtVarint::read_varint_resume
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DecodeState {
    decoded_value: u64,
    shift: u32,
    bytes_consumed: usize,
}

/// Result of a varint decode step.
///
/// Returned by [`ReadExtVarint::read_varint_partial`] and
/// [`ReadExtVarint::read_varint_resume`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeOutcome {
    /// Parsing completed successfully.
    Complete(Varint),
    /// Parser ran out of input. Use [`read_varint_resume`] with the state
    /// to continue from another reader/stream.
    ///
    /// [`read_varint_resume`]: ReadExtVarint::read_varint_resume
    Incomplete(DecodeState),
    /// No bytes were read (empty input).
    Empty,
}

impl DecodeState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)] // Used by tests and potentially by future resume APIs
    pub(crate) fn bytes_consumed(&self) -> usize {
        self.bytes_consumed
    }

    #[inline]
    pub(crate) fn feed<I, E>(mut self, iter: I) -> Result<DecodeOutcome>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        for byte_result in iter.take(MAX_VARINT_BYTES - self.bytes_consumed) {
            let byte = byte_result.map_err(Into::into)?;
            self.bytes_consumed += 1;

            let value = (byte & VARINT_PAYLOAD_MASK) as u64;
            // Bytes 1-9 contribute 7 bits each. The 10th byte may only set bit 63.
            if self.shift >= 64 {
                return Err(ProtobufError::VarintTooLong);
            }
            let bits_left = 64 - self.shift;
            if bits_left < 7 && value >> bits_left != 0 {
                return Err(ProtobufError::VarintTooLong);
            }
            self.decoded_value |= value << self.shift;

            if byte & VARINT_CONTINUATION_BIT == 0 {
                let result_bytes = self.decoded_value.to_le_bytes();
                return Ok(DecodeOutcome::Complete(Varint::new(result_bytes)));
            }
            self.shift += 7;
        }

        if self.bytes_consumed == 0 {
            return Ok(DecodeOutcome::Empty);
        }

        if self.bytes_consumed >= MAX_VARINT_BYTES {
            return Err(ProtobufError::VarintTooLong);
        }

        Ok(DecodeOutcome::Incomplete(self))
    }
}

/// Iterator that reads varints from a `Read` until EOF.
///
/// Yields [`Result<Varint>`] for each varint. Stops when the reader is empty or an
/// error occurs. Returned by [`ReadExtVarint::read_varints`].
pub struct VarintIterFromRead<R> {
    reader: R,
}

impl<R: Read> Iterator for VarintIterFromRead<R> {
    type Item = Result<Varint>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_varint() {
            Ok(Some(v)) => Some(Ok(v)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

// ============================================================================
// ReadExtVarint
// ============================================================================

/// Feed `state` from `reader`, optionally tee-ing every consumed wire byte into `buf`.
///
/// Single decode kernel for complete reads and chunked partial/resume:
/// - `Some(buf)` — copy wire bytes; return `&buf[..n]` for this call
/// - `None` — decode only (partial / resume)
#[inline]
#[allow(clippy::unbuffered_bytes)] // Varint needs byte-at-a-time; prefer BufReader upstream.
fn feed_varint_with_raw<'a, R: Read>(
    reader: &mut R,
    state: DecodeState,
    buf: Option<&'a mut [u8; MAX_VARINT_BYTES]>,
) -> Result<(DecodeOutcome, Option<&'a [u8]>)> {
    let mut n = 0usize;
    match buf {
        Some(buf) => {
            let outcome = state.feed(reader.by_ref().bytes().map(|r| {
                r.map(|b| {
                    // `feed` caps at `MAX_VARINT_BYTES - state.bytes_consumed`; `n` starts at 0.
                    buf[n] = b;
                    n += 1;
                    b
                })
            }))?;
            Ok((outcome, Some(&buf[..n])))
        }
        None => Ok((state.feed(reader.by_ref().bytes())?, None)),
    }
}

/// Extension trait for reading varints from [`Read`] instances.
///
/// Reads until the reader's EOF (or an existing [`std::io::Take`] bound).
/// Methods do not take a separate byte-budget argument.
///
/// All decode entry points share one private feed helper:
///
/// - [`read_varint_with_raw`](Self::read_varint_with_raw) / [`read_varint`](Self::read_varint)
///   / [`read_varints`](Self::read_varints) — complete reads (`Incomplete` →
///   [`UnexpectedEof`](ProtobufError::UnexpectedEof); raw capture when requested)
/// - [`read_varint_partial`](Self::read_varint_partial) /
///   [`read_varint_resume`](Self::read_varint_resume) — same feed, chunked
///   [`DecodeOutcome`] (no raw capture)
///
/// # Performance Note
///
/// When reading from file handles or network streams, consider wrapping the reader
/// with [`BufReader`](std::io::BufReader) to avoid inefficient byte-by-byte system calls:
///
/// ```no_run
/// use std::io::{BufReader, Read};
/// use protobuf_core::{DecodeOutcome, ReadExtVarint};
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let file = std::fs::File::open("data.bin")?;
/// let mut reader = BufReader::new(file);
/// let outcome = reader.read_varint_partial()?;
/// if let DecodeOutcome::Complete(varint) = outcome {
///     // use varint
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Example
/// ```
/// use ::std::io::Cursor;
/// use ::protobuf_core::{DecodeOutcome, ReadExtVarint, Varint};
///
/// let data = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut reader = Cursor::new(data);
/// let outcome = reader.read_varint_partial().unwrap();
/// let varint = match outcome {
///     DecodeOutcome::Complete(v) => v,
///     _ => panic!("expected complete"),
/// };
/// assert_eq!(varint.to_uint64(), 150);
/// ```
pub trait ReadExtVarint: Read {
    /// Read a varint and copy its exact wire bytes into `buf`.
    ///
    /// Returns `Ok(Some((varint, raw)))` where `raw` is `&buf[..n]` (including
    /// non-canonical encodings), `Ok(None)` on empty input, and `Err` when the
    /// input is incomplete or invalid.
    ///
    /// # Example
    /// ```
    /// use ::std::io::Cursor;
    /// use ::protobuf_core::{ReadExtVarint, MAX_VARINT_BYTES};
    ///
    /// let mut reader = Cursor::new(vec![0x96, 0x01, 0x08]);
    /// let mut buf = [0u8; MAX_VARINT_BYTES];
    /// let (varint, raw) = reader.read_varint_with_raw(&mut buf).unwrap().unwrap();
    /// assert_eq!(varint.to_uint64(), 150);
    /// assert_eq!(raw, &[0x96, 0x01]);
    /// ```
    #[inline]
    fn read_varint_with_raw<'a>(
        &'a mut self,
        buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<Option<(Varint, &'a [u8])>>
    where
        Self: Sized,
    {
        let (outcome, raw) = feed_varint_with_raw(self, DecodeState::new(), Some(buf))?;
        match outcome {
            DecodeOutcome::Complete(v) => Ok(Some((v, raw.expect("raw capture requested")))),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(_) => Err(ProtobufError::UnexpectedEof),
        }
    }

    /// Read a varint until EOF or a complete value.
    ///
    /// Via [`read_varint_with_raw`](Self::read_varint_with_raw) (wire bytes
    /// discarded). Returns `Ok(None)` on empty input.
    #[inline]
    fn read_varint(&mut self) -> Result<Option<Varint>>
    where
        Self: Sized,
    {
        let mut buf = [0u8; MAX_VARINT_BYTES];
        Ok(self.read_varint_with_raw(&mut buf)?.map(|(v, _)| v))
    }

    /// Create an iterator that reads multiple varints until EOF.
    ///
    /// Each item comes from [`read_varint`](Self::read_varint).
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

    /// Read a varint, supporting incomplete input ([`DecodeOutcome::Incomplete`]).
    #[inline]
    fn read_varint_partial(&mut self) -> Result<DecodeOutcome>
    where
        Self: Sized,
    {
        Ok(feed_varint_with_raw(self, DecodeState::new(), None)?.0)
    }

    /// Resume varint decoding with additional bytes from this reader.
    #[inline]
    fn read_varint_resume(&mut self, state: DecodeState) -> Result<DecodeOutcome>
    where
        Self: Sized,
    {
        Ok(feed_varint_with_raw(self, state, None)?.0)
    }
}

impl<R: Read> ReadExtVarint for R {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::{DecodeOutcome, DecodeState, ReadExtVarint};
    use crate::ProtobufError;
    use crate::varint::Varint;

    #[test]
    fn test_read_ext_varint_from_slice() {
        let mut slice = &[0x96u8, 0x01][..];
        let outcome = slice.read_varint_partial().unwrap();
        let varint = match outcome {
            DecodeOutcome::Complete(v) => v,
            _ => panic!("expected Complete"),
        };
        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_read_varint_with_raw_preserves_noncanonical_bytes() {
        use crate::wire_format::MAX_VARINT_BYTES;
        use ::std::io::Cursor;

        // Overlong encoding of 1: 0x81 0x00
        let mut reader = Cursor::new(vec![0x81, 0x00, 0x08]);
        let mut buf = [0u8; MAX_VARINT_BYTES];
        let (varint, raw) = reader.read_varint_with_raw(&mut buf).unwrap().unwrap();
        assert_eq!(varint.to_uint64(), 1);
        assert_eq!(raw, &[0x81, 0x00]);
        assert_eq!(reader.read_varint().unwrap().unwrap().to_uint64(), 8);
    }

    #[test]
    fn test_read_varint_with_raw_on_slice_via_read_ext() {
        use crate::wire_format::MAX_VARINT_BYTES;

        let mut slice: &[u8] = &[0x81, 0x00, 0x08];
        let mut buf = [0u8; MAX_VARINT_BYTES];
        let (varint, raw) = slice.read_varint_with_raw(&mut buf).unwrap().unwrap();
        assert_eq!(varint.to_uint64(), 1);
        assert_eq!(raw, &[0x81, 0x00]);
        assert_eq!(slice, &[0x08]);
    }

    #[test]
    fn test_read_varint_with_raw_empty_is_none() {
        use crate::wire_format::MAX_VARINT_BYTES;
        use ::std::io::Cursor;

        let mut reader = Cursor::new(Vec::<u8>::new());
        let mut buf = [0u8; MAX_VARINT_BYTES];
        assert!(reader.read_varint_with_raw(&mut buf).unwrap().is_none());
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
        let outcome = slice.read_varint_partial().unwrap();
        assert_eq!(outcome, DecodeOutcome::Empty);
    }

    #[test]
    fn test_read_ext_varint_from_slice_incomplete() {
        let mut slice = &[0x80u8][..];
        let result = slice.read_varint_partial();
        assert!(result.is_ok());
        assert!(matches!(result, Ok(DecodeOutcome::Incomplete(_))));
    }

    #[test]
    fn test_read_ext_varint_short_slice_incomplete() {
        // Full encoding of 150 is [0x96, 0x01]; a one-byte view yields Incomplete.
        let bytes = [0x96u8, 0x01];
        let mut slice = &bytes[..1];
        let outcome = slice.read_varint_partial().unwrap();
        assert!(matches!(
            outcome,
            DecodeOutcome::Incomplete(state) if state.bytes_consumed() == 1
        ));
        assert!(slice.is_empty());
    }

    #[test]
    fn test_decode_state_feed_incomplete() {
        use ::std::convert::Infallible;

        let slice = &[0x80u8][..];
        let result = DecodeState::new().feed(slice.iter().copied().map(Ok::<u8, Infallible>));
        assert!(result.is_ok());
        assert!(matches!(result, Ok(DecodeOutcome::Incomplete(_))));
    }

    #[test]
    fn test_decode_state_feed_resume_complete() {
        use ::std::convert::Infallible;

        let buf1 = &[0x80u8][..];
        let Ok(DecodeOutcome::Incomplete(state)) =
            DecodeState::new().feed(buf1.iter().copied().map(Ok::<u8, Infallible>))
        else {
            panic!("Expected Incomplete");
        };

        let buf2 = &[0x01u8][..];
        let Ok(DecodeOutcome::Complete(varint)) =
            state.feed(buf2.iter().copied().map(Ok::<u8, Infallible>))
        else {
            panic!("Expected Complete");
        };
        assert_eq!(varint.to_uint64(), 128);
    }

    #[test]
    fn test_decode_state_feed_resume_incomplete() {
        use ::std::convert::Infallible;

        let buf1 = &[0x80u8][..];
        let Ok(DecodeOutcome::Incomplete(state)) =
            DecodeState::new().feed(buf1.iter().copied().map(Ok::<u8, Infallible>))
        else {
            panic!("Expected Incomplete");
        };

        let buf2 = &[0x80u8][..];
        let result = state.feed(buf2.iter().copied().map(Ok::<u8, Infallible>));
        assert!(result.is_ok());
        assert!(matches!(result, Ok(DecodeOutcome::Incomplete(_))));
    }

    #[test]
    fn test_read_ext_varint_trait() {
        use ::std::io::Cursor;

        let input = [0x96, 0x01];
        let mut reader = Cursor::new(input);
        let outcome = reader.read_varint_partial().unwrap();
        let varint = match outcome {
            DecodeOutcome::Complete(v) => v,
            _ => panic!("expected Complete"),
        };
        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_read_ext_varint_empty() {
        use ::std::io::Cursor;

        let mut reader = Cursor::new(vec![]);
        let outcome = reader.read_varint_partial().unwrap();
        assert_eq!(outcome, DecodeOutcome::Empty);
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
    fn test_write_varint_roundtrip() {
        use crate::varint::WriteExtVarint;
        use ::std::io::Cursor;

        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            let varint = Varint::from_uint64(value);

            let mut buffer = Vec::new();
            buffer.write_varint(&varint).unwrap();

            let mut reader = Cursor::new(buffer);
            let outcome = reader.read_varint_partial().unwrap();
            let decoded_varint = match outcome {
                DecodeOutcome::Complete(v) => v,
                _ => panic!("expected Complete"),
            };
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
        let varint = reader.read_varint().unwrap().unwrap();
        assert_eq!(varint.to_uint64(), u64::MAX);

        // Overlong encoding of 0: the 10th byte contributes no bits. Parsers accept it.
        let bytes = vec![0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
        let mut reader = Cursor::new(bytes);
        let varint = reader.read_varint().unwrap().unwrap();
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
            let decoded = reader.read_varint().unwrap().unwrap();
            assert_eq!(decoded.to_uint64(), value, "roundtrip {value:#x}");
        }
    }

    #[test]
    fn test_read_varint_truncated_and_public_resume() {
        use ::std::io::Cursor;

        let mut reader = Cursor::new(vec![0x80u8]);
        let truncated = reader.read_varint();
        assert!(matches!(truncated, Err(ProtobufError::UnexpectedEof)));

        let mut first = Cursor::new(vec![0x80u8]);
        let state = match first.read_varint_partial().unwrap() {
            DecodeOutcome::Incomplete(state) => state,
            other => panic!("expected Incomplete, got {other:?}"),
        };
        let mut second = Cursor::new(vec![0x01u8]);
        match second.read_varint_resume(state).unwrap() {
            DecodeOutcome::Complete(varint) => assert_eq!(varint.to_uint64(), 128),
            other => panic!("expected Complete, got {other:?}"),
        }
    }
}
