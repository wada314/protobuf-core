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

//! Varint decoding (reading) from iterators, Read, and async streams.
//!
//! This module provides extension traits and types for reading varints from
//! various sources, including partial decoding and resume support.

use crate::wire_format::{MAX_VARINT_BYTES, VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK};
use crate::{ProtobufError, Result};
use ::std::io::Read;
use ::std::iter::Iterator;

#[cfg(feature = "futures")]
use ::futures_util::future::poll_fn;
#[cfg(feature = "futures")]
use ::futures_util::stream::TryStream;
#[cfg(feature = "futures")]
use ::std::future::Future;
#[cfg(feature = "futures")]
use ::std::pin::Pin;
#[cfg(feature = "futures")]
use ::std::task::Poll;

use super::Varint;

/// Parser state when varint decoding runs out of bytes before completing.
///
/// Returned in [`ProtobufError::IncompleteVarint`](crate::ProtobufError::IncompleteVarint).
/// Varint parsing is performed only through the extension traits
/// ([`IteratorExtVarint`], [`TryIteratorExtVarint`], [`ReadExtVarint`], etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DecodeState {
    decoded_value: u64,
    shift: u32,
    bytes_consumed: usize,
}

/// Result of a varint decode step.
///
/// Returned by [`IteratorExtVarint::read_varint_partial`], [`ReadExtVarint::read_varint_partial`],
/// and their `read_varint_resume` variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeOutcome {
    /// Parsing completed successfully.
    Complete(Varint),
    /// Parser ran out of input. Use [`read_varint_resume`] with the state
    /// to continue from another reader/iterator/stream.
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

    pub(crate) fn feed<I, E>(mut self, iter: I) -> Result<DecodeOutcome>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        for byte_result in iter.take(MAX_VARINT_BYTES - self.bytes_consumed) {
            let byte = byte_result.map_err(Into::into)?;
            self.bytes_consumed += 1;

            let value = (byte & VARINT_PAYLOAD_MASK) as u64;
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

/// Iterator that reads multiple varints from a byte iterator.
///
/// This iterator yields `Result<Varint>` for each varint read from the underlying iterator.
/// It stops when there are no more bytes available or an error occurs.
pub struct VarintIterator<I: Iterator> {
    bytes: I,
}

/// Iterator adapter that converts `Iterator<Item = u8>` to `Iterator<Item = Result<u8, Infallible>>`.
///
/// This wraps each `u8` value from the inner iterator with `Ok()`.
pub struct ToResultIterator<I> {
    inner: I,
}

impl<I> Iterator for ToResultIterator<I>
where
    I: Iterator<Item = u8>,
{
    type Item = ::std::result::Result<u8, ::std::convert::Infallible>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Ok)
    }
}

impl<I, E> VarintIterator<I>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn new(bytes: I) -> Self {
        Self { bytes }
    }
}

impl<I, E> Iterator for VarintIterator<I>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    type Item = Result<Varint>;

    fn next(&mut self) -> Option<Self::Item> {
        match DecodeState::new().feed(&mut self.bytes) {
            Ok(DecodeOutcome::Complete(v)) => Some(Ok(v)),
            Ok(DecodeOutcome::Empty) => None,
            Ok(DecodeOutcome::Incomplete(s)) => Some(Err(ProtobufError::IncompleteVarint(s))),
            Err(e) => Some(Err(e)),
        }
    }
}

// ============================================================================
// IteratorExtVarint
// ============================================================================

/// This trait provides convenient methods to collect varints directly from
/// any iterator that yields bytes.
///
/// # Example
/// ```
/// use ::protobuf_core::{DecodeOutcome, IteratorExtVarint, Varint};
///
/// let bytes = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut iter = bytes.into_iter();
/// let outcome = iter.read_varint_partial().unwrap();
/// let varint = match outcome {
///     DecodeOutcome::Complete(v) => v,
///     DecodeOutcome::Empty => panic!("expected varint"),
///     DecodeOutcome::Incomplete(_) => panic!("unexpected incomplete"),
/// };
/// assert_eq!(varint.to_uint64(), 150);
/// ```
pub trait IteratorExtVarint {
    /// Read a varint from this iterator, supporting incomplete input.
    fn read_varint_partial(self) -> Result<DecodeOutcome>;

    /// Resume varint decoding with additional bytes from this iterator.
    ///
    /// Use when a previous [`read_varint_partial`](IteratorExtVarint::read_varint_partial) returned
    /// `DecodeOutcome::Incomplete(state)` and you have more bytes from another source.
    fn read_varint_resume(self, state: DecodeState) -> Result<DecodeOutcome>;

    /// Create an iterator that reads multiple varints from this iterator.
    ///
    /// Returns an iterator that yields `Result<Varint>` for each varint read.
    /// The iterator stops when there are no more bytes available or an error occurs.
    ///
    /// # Example
    /// ```
    /// use ::protobuf_core::{IteratorExtVarint, Varint};
    ///
    /// let bytes = vec![0x96, 0x01, 0x7F]; // 150 and 127 in varint encoding
    /// let iter = bytes.into_iter();
    /// let varints: Vec<Varint> = iter.read_varints().collect::<Result<Vec<_>, _>>().unwrap();
    /// assert_eq!(varints.len(), 2);
    /// assert_eq!(varints[0].to_uint64(), 150);
    /// assert_eq!(varints[1].to_uint64(), 127);
    /// ```
    fn read_varints(self) -> VarintIterator<ToResultIterator<Self>>
    where
        Self: Sized + Iterator<Item = u8>;
}

impl<I> IteratorExtVarint for I
where
    I: Iterator<Item = u8>,
{
    fn read_varint_partial(self) -> Result<DecodeOutcome> {
        DecodeState::new().feed(self.map(Ok::<u8, ::std::convert::Infallible>))
    }

    fn read_varint_resume(self, state: DecodeState) -> Result<DecodeOutcome> {
        state.feed(self.map(Ok::<u8, ::std::convert::Infallible>))
    }

    fn read_varints(self) -> VarintIterator<ToResultIterator<Self>>
    where
        Self: Sized,
    {
        VarintIterator::new(ToResultIterator { inner: self })
    }
}

// ============================================================================
// TryIteratorExtVarint
// ============================================================================

/// Extension trait for reading varints from byte iterators that yield `Result<u8, E>`.
///
/// This trait provides convenient methods to read varints directly from
/// any iterator that yields `Result<u8, E>`, allowing proper error propagation.
/// The error type `E` must implement `Into<ProtobufError>`.
///
/// # Example
/// ```
/// use ::std::io::{Cursor, Read};
/// use ::protobuf_core::{DecodeOutcome, TryIteratorExtVarint, Varint};
///
/// let data = vec![0x96, 0x01]; // 150 in varint encoding
/// let mut reader = Cursor::new(data);
/// let iter = reader.bytes(); // Iterator<Item = Result<u8, io::Error>>
/// let outcome = iter.read_varint_partial().unwrap();
/// let varint = match outcome {
///     DecodeOutcome::Complete(v) => v,
///     _ => panic!("expected complete"),
/// };
/// assert_eq!(varint.to_uint64(), 150);
/// ```
pub trait TryIteratorExtVarint {
    /// Read a varint from this iterator, supporting incomplete input.
    fn read_varint_partial(self) -> Result<DecodeOutcome>;

    /// Resume varint decoding with additional bytes from this iterator.
    fn read_varint_resume(self, state: DecodeState) -> Result<DecodeOutcome>;

    /// Create an iterator that reads multiple varints from this iterator.
    ///
    /// Returns an iterator that yields `Result<Varint>` for each varint read.
    /// The iterator stops when there are no more bytes available or an error occurs.
    ///
    /// # Example
    /// ```
    /// use ::std::io::{Cursor, Read};
    /// use ::protobuf_core::{TryIteratorExtVarint, Varint};
    ///
    /// let data = vec![0x96, 0x01, 0x7F]; // 150 and 127 in varint encoding
    /// let reader = Cursor::new(data);
    /// let iter = reader.bytes();
    /// let varints: Vec<Varint> = iter.read_varints().collect::<Result<Vec<_>, _>>().unwrap();
    /// assert_eq!(varints.len(), 2);
    /// assert_eq!(varints[0].to_uint64(), 150);
    /// assert_eq!(varints[1].to_uint64(), 127);
    /// ```
    fn read_varints(self) -> VarintIterator<Self>
    where
        Self: Sized + Iterator;
}

impl<I, E> TryIteratorExtVarint for I
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn read_varint_partial(self) -> Result<DecodeOutcome> {
        DecodeState::new().feed(self)
    }

    fn read_varint_resume(self, state: DecodeState) -> Result<DecodeOutcome> {
        state.feed(self)
    }

    fn read_varints(self) -> VarintIterator<Self>
    where
        Self: Sized,
    {
        VarintIterator::new(self)
    }
}

// ============================================================================
// ReadExtVarint
// ============================================================================

/// Extension trait for reading varints from Read instances.
///
/// This trait provides a convenient method to read varints directly from
/// any type that implements `std::io::Read`.
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
pub trait ReadExtVarint {
    /// Read a varint from this reader, supporting incomplete input.
    fn read_varint_partial(&mut self) -> Result<DecodeOutcome>;

    /// Resume varint decoding with additional bytes from this reader.
    fn read_varint_resume(&mut self, state: DecodeState) -> Result<DecodeOutcome>;

    /// Create an iterator that reads multiple varints from this reader.
    ///
    /// Returns an iterator that yields `Result<Varint>` for each varint read.
    /// The iterator stops when there are no more bytes available (EOF) or an error occurs.
    ///
    /// # Example
    /// ```
    /// use ::std::io::Cursor;
    /// use ::protobuf_core::{ReadExtVarint, Varint};
    ///
    /// let data = vec![0x96, 0x01, 0x7F]; // 150 and 127 in varint encoding
    /// let mut reader = Cursor::new(data);
    /// let varints: Vec<Varint> = reader.read_varints().collect::<Result<Vec<_>, _>>().unwrap();
    /// assert_eq!(varints.len(), 2);
    /// assert_eq!(varints[0].to_uint64(), 150);
    /// assert_eq!(varints[1].to_uint64(), 127);
    /// ```
    fn read_varints(&mut self) -> VarintIterator<::std::io::Bytes<&mut Self>>
    where
        Self: ::std::io::Read;
}

impl<R> ReadExtVarint for R
where
    R: Read,
{
    #[allow(clippy::unbuffered_bytes)] // Varint parsing requires byte-by-byte reading; caller should use BufReader for efficiency
    fn read_varint_partial(&mut self) -> Result<DecodeOutcome> {
        DecodeState::new().feed(self.bytes())
    }

    #[allow(clippy::unbuffered_bytes)]
    fn read_varint_resume(&mut self, state: DecodeState) -> Result<DecodeOutcome> {
        state.feed(self.bytes())
    }

    #[allow(clippy::unbuffered_bytes)] // Varint parsing requires byte-by-byte reading; caller should use BufReader for efficiency
    fn read_varints(&mut self) -> VarintIterator<::std::io::Bytes<&mut Self>> {
        VarintIterator::new(self.bytes())
    }
}

// ============================================================================
// Async Stream extension (TryStream)
// ============================================================================

/// Extension trait for reading varints from a byte stream.
///
/// This trait provides `read_varint_partial` for types implementing
/// `TryStream<Ok = u8, Error = E>` where `E: Into<ProtobufError>`.
/// Consumers implement `Stream<Item = Result<u8, E>>`; the futures
/// ecosystem provides adapters that implement `TryStream` for such streams.
///
/// Uses `Pin<&mut Self>` so that both `Unpin` and `!Unpin` streams are supported.
///
/// # Example
///
/// ```ignore
/// use futures_util::stream;
/// use protobuf_core::{StreamExtVarint, Varint};
/// use std::pin::Pin;
///
/// let bytes = vec![Ok(0x96u8), Ok(0x01)];  // 150 in varint encoding
/// let mut stream = stream::iter(bytes);
/// let varint = Pin::new(&mut stream).read_varint_partial().await?.expect("one varint");
/// assert_eq!(varint.to_uint64(), 150);
/// ```
#[cfg(feature = "futures")]
pub trait StreamExtVarint: TryStream<Ok = u8> {
    /// Read the next varint from the stream, supporting incomplete input.
    fn read_varint_partial(self: Pin<&mut Self>) -> impl Future<Output = Result<DecodeOutcome>> + Send
    where
        Self::Error: Into<ProtobufError>;

    /// Resume varint decoding with additional bytes from this stream.
    fn read_varint_resume(
        self: Pin<&mut Self>,
        state: DecodeState,
    ) -> impl Future<Output = Result<DecodeOutcome>> + Send
    where
        Self::Error: Into<ProtobufError>;
}

#[cfg(feature = "futures")]
impl<S, E> StreamExtVarint for S
where
    S: TryStream<Ok = u8, Error = E> + Send,
    E: Into<ProtobufError>,
{
    fn read_varint_partial(self: Pin<&mut Self>) -> impl Future<Output = Result<DecodeOutcome>> + Send {
        let mut state = DecodeState::new();
        let mut this = self;
        poll_fn(move |cx| {
            for _ in 0..MAX_VARINT_BYTES {
                let Poll::Ready(item) = this.as_mut().try_poll_next(cx) else {
                    return Poll::Pending;
                };
                match item {
                    Some(Ok(byte)) => match state.feed(::std::iter::once(Ok::<u8, E>(byte))) {
                        Ok(DecodeOutcome::Complete(v)) => {
                            return Poll::Ready(Ok(DecodeOutcome::Complete(v)));
                        }
                        Ok(DecodeOutcome::Incomplete(s)) => state = s,
                        Ok(DecodeOutcome::Empty) => unreachable!(),
                        Err(e) => return Poll::Ready(Err(e)),
                    },
                    Some(Err(e)) => return Poll::Ready(Err(e.into())),
                    None => {
                        if state.bytes_consumed() > 0 {
                            return Poll::Ready(Ok(DecodeOutcome::Incomplete(state)));
                        }
                        return Poll::Ready(Ok(DecodeOutcome::Empty));
                    }
                }
            }
            Poll::Ready(Err(ProtobufError::VarintTooLong))
        })
    }

    fn read_varint_resume(
        self: Pin<&mut Self>,
        mut state: DecodeState,
    ) -> impl Future<Output = Result<DecodeOutcome>> + Send {
        let mut this = self;
        poll_fn(move |cx| {
            for _ in 0..(MAX_VARINT_BYTES - state.bytes_consumed()) {
                let Poll::Ready(item) = this.as_mut().try_poll_next(cx) else {
                    return Poll::Pending;
                };
                match item {
                    Some(Ok(byte)) => match state.feed(::std::iter::once(Ok::<u8, E>(byte))) {
                        Ok(DecodeOutcome::Complete(v)) => {
                            return Poll::Ready(Ok(DecodeOutcome::Complete(v)));
                        }
                        Ok(DecodeOutcome::Incomplete(s)) => state = s,
                        Ok(DecodeOutcome::Empty) => unreachable!(),
                        Err(e) => return Poll::Ready(Err(e)),
                    },
                    Some(Err(e)) => return Poll::Ready(Err(e.into())),
                    None => return Poll::Ready(Ok(DecodeOutcome::Incomplete(state))),
                }
            }
            Poll::Ready(Err(ProtobufError::VarintTooLong))
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::{DecodeOutcome, DecodeState, IteratorExtVarint, ReadExtVarint, TryIteratorExtVarint};
    use crate::ProtobufError;
    use crate::varint::Varint;

    #[cfg(feature = "futures")]
    use super::StreamExtVarint;
    #[cfg(feature = "futures")]
    use ::assert_matches::assert_matches;
    #[cfg(feature = "futures")]
    use ::futures_executor::block_on;
    #[cfg(feature = "futures")]
    use ::futures_util::stream::{self, poll_fn};
    #[cfg(feature = "futures")]
    use ::std::convert::Infallible;
    #[cfg(feature = "futures")]
    use ::std::pin::Pin;
    #[cfg(feature = "futures")]
    use ::std::task::Poll;

    #[test]
    fn test_read_varint_from_iterator() {
        let input = [0x96, 0x01];
        let iter = input.iter().copied();
        let outcome = iter.read_varint_partial().unwrap();
        let varint = match outcome {
            DecodeOutcome::Complete(v) => v,
            _ => panic!("expected Complete"),
        };
        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_iterator_ext_varint_trait() {
        let bytes = vec![0x96, 0x01];
        let iter = bytes.into_iter();
        let outcome = iter.read_varint_partial().unwrap();
        let varint = match outcome {
            DecodeOutcome::Complete(v) => v,
            _ => panic!("expected Complete"),
        };
        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_iterator_ext_varint_empty() {
        let outcome = IteratorExtVarint::read_varint_partial(::std::iter::empty()).unwrap();
        assert_eq!(outcome, DecodeOutcome::Empty);
    }

    #[test]
    fn test_iterator_ext_varint_read_varints() {
        let bytes = vec![0x96, 0x01, 0x7F, 0x01];
        let iter = bytes.into_iter();
        let varints: Vec<Varint> = iter.read_varints().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(varints.len(), 3);
        assert_eq!(varints[0].to_uint64(), 150);
        assert_eq!(varints[1].to_uint64(), 127);
        assert_eq!(varints[2].to_uint64(), 1);
    }

    #[test]
    fn test_iterator_ext_varint_incomplete() {
        let bytes = vec![0x80u8];
        let result = IteratorExtVarint::read_varint_partial(bytes.into_iter());
        assert!(result.is_ok());
        assert!(matches!(
            result,
            Ok(DecodeOutcome::Incomplete(state)) if state.bytes_consumed() == 1
        ));
    }

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
    fn test_try_iterator_ext_varint() {
        use ::std::io::{Cursor, Read};

        let data = vec![0x96, 0x01];
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let outcome = TryIteratorExtVarint::read_varint_partial(iter).unwrap();
        let varint = match outcome {
            DecodeOutcome::Complete(v) => v,
            _ => panic!("expected Complete"),
        };
        assert_eq!(varint.to_uint64(), 150);
    }

    #[test]
    fn test_try_iterator_ext_varint_empty() {
        use ::std::io::{Cursor, Read};

        let data = vec![];
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let outcome = TryIteratorExtVarint::read_varint_partial(iter).unwrap();
        assert_eq!(outcome, DecodeOutcome::Empty);
    }

    #[test]
    fn test_try_iterator_ext_varint_error() {
        use ::std::io::ErrorKind;

        let error = ::std::io::Error::new(ErrorKind::UnexpectedEof, "test error");
        let iter = ::std::iter::once(Err(error));
        let result = TryIteratorExtVarint::read_varint_partial(iter);

        assert!(result.is_err());
        if let Err(ProtobufError::IoError(io_err)) = result {
            assert_eq!(io_err.kind(), ErrorKind::UnexpectedEof);
        } else {
            panic!("Expected IoError");
        }
    }

    #[test]
    fn test_try_iterator_ext_varint_read_varints() {
        use ::std::io::{Cursor, Read};

        let data = vec![0x96, 0x01, 0x7F, 0x01];
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let varints: Vec<Varint> = iter.read_varints().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(varints.len(), 3);
        assert_eq!(varints[0].to_uint64(), 150);
        assert_eq!(varints[1].to_uint64(), 127);
        assert_eq!(varints[2].to_uint64(), 1);
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
    fn test_write_varint_roundtrip() {
        use crate::varint::WriteExtVarint;

        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            let varint = Varint::from_uint64(value);

            let mut buffer = Vec::new();
            buffer.write_varint(&varint).unwrap();

            let iter = buffer.iter().copied();
            let outcome = iter.read_varint_partial().unwrap();
            let decoded_varint = match outcome {
                DecodeOutcome::Complete(v) => v,
                _ => panic!("expected Complete"),
            };
            let decoded_value = decoded_varint.to_uint64();

            assert_eq!(decoded_value, value, "Roundtrip failed for value {}", value);
        }
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_complete() {
        let bytes: Vec<Result<u8, Infallible>> = vec![Ok(0x96), Ok(0x01)];
        let mut stream = stream::iter(bytes);

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Ok(DecodeOutcome::Complete(varint)) => {
            assert_eq!(varint.to_uint64(), 150);
        });
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_eof() {
        let mut stream = stream::empty::<Result<u8, Infallible>>();

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Ok(DecodeOutcome::Empty));
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_resume_from_pending() {
        let mut state = 0u32;
        let mut stream = poll_fn(move |cx| {
            let next: Option<Result<u8, Infallible>> = match state {
                0 => {
                    state = 1;
                    Some(Ok(0x96))
                }
                1 => {
                    state = 2;
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                2 => {
                    state = 3;
                    Some(Ok(0x01))
                }
                _ => None,
            };
            Poll::Ready(next)
        });

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Ok(DecodeOutcome::Complete(varint)) => {
            assert_eq!(varint.to_uint64(), 150);
        });
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_stream_error() {
        let bytes: Vec<Result<u8, std::io::Error>> = vec![
            Ok(0x80),
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "stream error",
            )),
        ];
        let mut stream = stream::iter(bytes);

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Err(_));
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_eof_mid_varint() {
        let bytes: Vec<Result<u8, Infallible>> = vec![Ok(0x80)];
        let mut stream = stream::iter(bytes);

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Ok(DecodeOutcome::Incomplete(_)));
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_too_long() {
        use crate::wire_format::MAX_VARINT_BYTES;
        let bytes: Vec<Result<u8, Infallible>> = (0..=MAX_VARINT_BYTES).map(|_| Ok(0x80)).collect();
        let mut stream = stream::iter(bytes);

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Err(ProtobufError::VarintTooLong));
    }
}
