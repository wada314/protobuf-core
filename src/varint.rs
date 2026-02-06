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

use crate::wire_format::{MAX_VARINT_BYTES, VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK};
use crate::{ProtobufError, Result};
use ::std::convert::{Infallible, TryFrom};
use ::std::io::{Read, Write};
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
    type Item = ::std::result::Result<u8, Infallible>;

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
///
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
        DecodeState::new().feed(self.map(Ok::<u8, Infallible>))
    }

    fn read_varint_resume(self, state: DecodeState) -> Result<DecodeOutcome> {
        state.feed(self.map(Ok::<u8, Infallible>))
    }

    fn read_varints(self) -> VarintIterator<ToResultIterator<Self>>
    where
        Self: Sized,
    {
        VarintIterator::new(ToResultIterator { inner: self })
    }
}

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

#[cfg(test)]
mod tests {
    use super::{MAX_VARINT_BYTES, Varint};

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

    // ============================================================================
    // Basic Varint tests (no traits)
    // ============================================================================

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

        let varint = Varint::new(bytes);
        // 406 in ZigZag encoding represents 203 in signed value
        assert_eq!(varint.to_sint64(), 203);

        let varint = Varint::new(bytes);
        match varint.try_to_sint32() {
            Ok(value) => assert_eq!(value, 203),
            Err(e) => panic!("Expected Ok(203), got error: {:?}", e),
        }

        let varint = Varint::new(bytes);
        assert_eq!(varint.to_bool(), true);
    }

    #[test]
    fn test_signed_integer_conversions() {
        // -1 in ZigZag encoding: 1
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

    // ============================================================================
    // Encoding tests
    // ============================================================================

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
    fn test_write_varint() {
        use super::WriteExtVarint;

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
    fn test_all_encoding_methods_consistency() {
        use super::WriteExtVarint;

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

    // ============================================================================
    // IteratorExtVarint tests
    // ============================================================================

    #[test]
    fn test_read_varint_from_iterator() {
        use super::{DecodeOutcome, IteratorExtVarint};

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
        use super::{DecodeOutcome, IteratorExtVarint};

        let bytes = vec![0x96, 0x01]; // 150 in varint encoding
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
        use super::{DecodeOutcome, IteratorExtVarint};

        let outcome = IteratorExtVarint::read_varint_partial(::std::iter::empty()).unwrap();
        assert_eq!(outcome, DecodeOutcome::Empty);
    }

    #[test]
    fn test_iterator_ext_varint_read_varints() {
        use super::IteratorExtVarint;

        let bytes = vec![0x96, 0x01, 0x7F, 0x01]; // 150, 127, 1 in varint encoding
        let iter = bytes.into_iter();
        let varints: Vec<Varint> = iter.read_varints().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(varints.len(), 3);
        assert_eq!(varints[0].to_uint64(), 150);
        assert_eq!(varints[1].to_uint64(), 127);
        assert_eq!(varints[2].to_uint64(), 1);
    }

    #[test]
    fn test_iterator_ext_varint_incomplete() {
        use super::{DecodeOutcome, IteratorExtVarint};

        // Single byte with continuation bit - incomplete varint
        let bytes = vec![0x80u8];
        let result = IteratorExtVarint::read_varint_partial(bytes.into_iter());
        assert!(result.is_ok());
        assert!(matches!(
            result,
            Ok(DecodeOutcome::Incomplete(state)) if state.bytes_consumed() == 1
        ));
    }

    // ============================================================================
    // ReadExtVarint from slice (&[u8] implements Read) and DecodeState::feed tests
    // ============================================================================

    #[test]
    fn test_read_ext_varint_from_slice() {
        use super::{DecodeOutcome, ReadExtVarint};

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
        use super::{DecodeOutcome, ReadExtVarint};

        let mut slice: &[u8] = &[];
        let outcome = slice.read_varint_partial().unwrap();
        assert_eq!(outcome, DecodeOutcome::Empty);
    }

    #[test]
    fn test_read_ext_varint_from_slice_incomplete() {
        use super::{DecodeOutcome, ReadExtVarint};

        let mut slice = &[0x80u8][..]; // Incomplete varint
        let result = slice.read_varint_partial();
        assert!(result.is_ok());
        assert!(matches!(result, Ok(DecodeOutcome::Incomplete(_))));
    }

    #[test]
    fn test_decode_state_feed_incomplete() {
        use super::DecodeState;
        use ::std::convert::Infallible;

        let slice = &[0x80u8][..]; // Incomplete: continuation bit set, payload 0
        let result = DecodeState::new().feed(slice.iter().copied().map(Ok::<u8, Infallible>));
        assert!(result.is_ok());
        assert!(matches!(result, Ok(super::DecodeOutcome::Incomplete(_))));
    }

    #[test]
    fn test_decode_state_feed_resume_complete() {
        use super::{DecodeOutcome, DecodeState};
        use ::std::convert::Infallible;

        let buf1 = &[0x80u8][..]; // Incomplete
        let Ok(DecodeOutcome::Incomplete(state)) =
            DecodeState::new().feed(buf1.iter().copied().map(Ok::<u8, Infallible>))
        else {
            panic!("Expected Incomplete");
        };

        let buf2 = &[0x01u8][..]; // Completes to value 128
        let Ok(DecodeOutcome::Complete(varint)) =
            state.feed(buf2.iter().copied().map(Ok::<u8, Infallible>))
        else {
            panic!("Expected Complete");
        };
        assert_eq!(varint.to_uint64(), 128);
    }

    #[test]
    fn test_decode_state_feed_resume_incomplete() {
        use super::{DecodeOutcome, DecodeState};
        use ::std::convert::Infallible;

        let buf1 = &[0x80u8][..]; // Incomplete
        let Ok(DecodeOutcome::Incomplete(state)) =
            DecodeState::new().feed(buf1.iter().copied().map(Ok::<u8, Infallible>))
        else {
            panic!("Expected Incomplete");
        };

        let buf2 = &[0x80u8][..]; // Still incomplete
        let result = state.feed(buf2.iter().copied().map(Ok::<u8, Infallible>));
        assert!(result.is_ok());
        assert!(matches!(result, Ok(super::DecodeOutcome::Incomplete(_))));
    }

    // ============================================================================
    // TryIteratorExtVarint tests
    // ============================================================================

    #[test]
    fn test_try_iterator_ext_varint() {
        use super::{DecodeOutcome, TryIteratorExtVarint};
        use ::std::io::{Cursor, Read};

        let data = vec![0x96, 0x01]; // 150 in varint encoding
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
        use super::{DecodeOutcome, TryIteratorExtVarint};
        use ::std::io::{Cursor, Read};

        let data = vec![];
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let outcome = TryIteratorExtVarint::read_varint_partial(iter).unwrap();
        assert_eq!(outcome, DecodeOutcome::Empty);
    }

    #[test]
    fn test_try_iterator_ext_varint_error() {
        use super::TryIteratorExtVarint;
        use crate::ProtobufError;
        use ::std::io::ErrorKind;

        // Create an iterator that returns an error
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
        use super::TryIteratorExtVarint;
        use ::std::io::{Cursor, Read};

        let data = vec![0x96, 0x01, 0x7F, 0x01]; // 150, 127, 1 in varint encoding
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let varints: Vec<Varint> = iter.read_varints().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(varints.len(), 3);
        assert_eq!(varints[0].to_uint64(), 150);
        assert_eq!(varints[1].to_uint64(), 127);
        assert_eq!(varints[2].to_uint64(), 1);
    }

    // ============================================================================
    // ReadExtVarint tests
    // ============================================================================

    #[test]
    fn test_read_ext_varint_trait() {
        use super::{DecodeOutcome, ReadExtVarint};
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
        use super::ReadExtVarint;
        use ::std::io::Cursor;

        let data = vec![0x96, 0x01, 0x7F, 0x01]; // 150, 127, 1 in varint encoding
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

    // ============================================================================
    // Roundtrip tests
    // ============================================================================

    #[test]
    fn test_write_varint_roundtrip() {
        use super::{IteratorExtVarint, WriteExtVarint};

        let test_values = vec![0, 1, 127, 128, 150, 255, 256, 65535, 0x7FFFFFFF];

        for &value in &test_values {
            // Create Varint from the test value
            let varint = Varint::from_uint64(value);

            let mut buffer = Vec::new();
            buffer.write_varint(&varint).unwrap();

            let iter = buffer.iter().copied();
            let outcome = iter.read_varint_partial().unwrap();
            let decoded_varint = match outcome {
                super::DecodeOutcome::Complete(v) => v,
                _ => panic!("expected Complete"),
            };
            let decoded_value = decoded_varint.to_uint64();

            assert_eq!(decoded_value, value, "Roundtrip failed for value {}", value);
        }
    }

    // ============================================================================
    // StreamExtVarint tests (requires futures feature)
    // ============================================================================

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_complete() {
        use super::DecodeOutcome;

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
        use super::DecodeOutcome;

        let mut stream = stream::empty::<Result<u8, Infallible>>();

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Ok(DecodeOutcome::Empty));
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_resume_from_pending() {
        use super::DecodeOutcome;

        // State: 0 = yield 0x96, 1 = return Pending (wake), 2 = yield 0x01, 3 = EOF
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
                    return Poll::Pending; // Yield once; executor will poll again
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
        use super::DecodeOutcome;

        // One byte with continuation bit set, then EOF (malformed)
        let bytes: Vec<Result<u8, Infallible>> = vec![Ok(0x80)];
        let mut stream = stream::iter(bytes);

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Ok(DecodeOutcome::Incomplete(_)));
    }

    #[cfg(feature = "futures")]
    #[test]
    fn test_stream_ext_varint_too_long() {
        // 11 bytes, all with continuation bit set - exceeds MAX_VARINT_BYTES (10)
        let bytes: Vec<Result<u8, Infallible>> = (0..=MAX_VARINT_BYTES).map(|_| Ok(0x80)).collect();
        let mut stream = stream::iter(bytes);

        let result = block_on(async { Pin::new(&mut stream).read_varint_partial().await });
        assert_matches!(result, Err(crate::ProtobufError::VarintTooLong));
    }
}
