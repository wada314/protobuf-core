//! Owned buffers for length-delimited payloads.
//!
//! Streaming readers ([`crate::ReadExtProtobuf`], [`crate::IteratorExtProtobuf`],
//! [`crate::TryIteratorExtProtobuf`]) build the `Len` payload with [`LenBuf`].
//! Slice readers borrow the input and do not use this trait.

use crate::{ProtobufError, Result};
use ::std::io::Read;

#[cfg(feature = "nightly")]
use ::std::alloc::Allocator;

/// Bytes per `append_slice` call in the default streaming readers.
const LEN_BUF_CHUNK: usize = 8192;

/// Buffer that can accept one length-delimited protobuf payload.
///
/// Implement [`with_capacity`](LenBuf::with_capacity) and
/// [`append_slice`](LenBuf::append_slice). The `from_*` methods have defaults
/// that create a buffer with [`with_capacity`](LenBuf::with_capacity) and append
/// fixed-size chunks. Override `from_read`, `from_iter`, or `from_try_iter` when
/// the buffer can take the bytes without that extra copy.
///
/// `Vec<u8>` and `Box<[u8]>` implement this trait. `Vec<u8>` writes the payload
/// with `read_exact`, and its iterator methods push each byte. `Box<[u8]>`
/// allocates one box of exactly `len` bytes and fills that box.
///
/// `Vec<u8, A>` and `Box<[u8], A>` are those payloads with an allocator. Enable
/// the `nightly` feature to use the allocator. docs.rs shows those signatures,
/// with a badge for the feature.
///
/// # Examples
///
/// ```
/// use ::protobuf_core::{FieldValue, LenBuf, ReadExtProtobuf};
/// use ::std::io::Cursor;
///
/// struct Bytes(Vec<u8>);
///
/// impl LenBuf for Bytes {
///     fn with_capacity(capacity: usize) -> Self {
///         Self(Vec::with_capacity(capacity))
///     }
///
///     fn append_slice(&mut self, data: &[u8]) {
///         self.0.extend_from_slice(data);
///     }
/// }
///
/// let input = [0x0A, 0x03, b'H', b'e', b'l'];
/// let field = Cursor::new(input)
///     .read_protobuf_fields::<Bytes>()
///     .next()
///     .unwrap()
///     .unwrap();
/// let FieldValue::Len(Bytes(data)) = field.value else {
///     panic!("expected a length-delimited field");
/// };
/// assert_eq!(data, b"Hel");
/// ```
pub trait LenBuf: Sized {
    /// Create a buffer that can accept at least `capacity` appended bytes.
    fn with_capacity(capacity: usize) -> Self;

    /// Append `data` to the buffer, in order.
    fn append_slice(&mut self, data: &[u8]);

    /// Read exactly `len` payload bytes from `reader`.
    ///
    /// The default reads into a stack chunk and [`append_slice`](LenBuf::append_slice)s
    /// each chunk. It does not allocate a second heap buffer for the payload.
    fn from_read<R>(reader: &mut R, len: usize) -> Result<Self>
    where
        R: Read,
    {
        let mut buf = Self::with_capacity(len);
        let mut remaining = len;
        let mut chunk = [0u8; LEN_BUF_CHUNK];
        while remaining > 0 {
            let n = remaining.min(chunk.len());
            reader.read_exact(&mut chunk[..n])?;
            buf.append_slice(&chunk[..n]);
            remaining -= n;
        }
        Ok(buf)
    }

    /// Read exactly `len` payload bytes from a byte iterator.
    fn from_iter<I>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = u8>,
    {
        Self::from_try_iter(&mut InfallibleBytes(iter), len)
    }

    /// Read exactly `len` payload bytes from a fallible byte iterator.
    ///
    /// The default stages bytes in a stack chunk, then appends that chunk.
    fn from_try_iter<I, E>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        let mut buf = Self::with_capacity(len);
        let mut remaining = len;
        let mut chunk = [0u8; LEN_BUF_CHUNK];
        while remaining > 0 {
            let n = remaining.min(chunk.len());
            for slot in &mut chunk[..n] {
                *slot = iter
                    .next()
                    .ok_or(ProtobufError::UnexpectedEof)?
                    .map_err(Into::into)?;
            }
            buf.append_slice(&chunk[..n]);
            remaining -= n;
        }
        Ok(buf)
    }
}

/// Adapts `Iterator<Item = u8>` to `Iterator<Item = Result<u8, Infallible>>`.
struct InfallibleBytes<'a, I>(&'a mut I);

impl<'a, I> Iterator for InfallibleBytes<'a, I>
where
    I: Iterator<Item = u8>,
{
    type Item = ::std::result::Result<u8, ::std::convert::Infallible>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Ok)
    }
}

/// Length-delimited payload stored in `Vec<u8>`.
///
/// This implementation does not require the `nightly` feature and does not
/// take a custom allocator. `from_read` fills the vector with `read_exact`.
/// The iterator methods push each byte.
#[cfg(not(feature = "nightly"))]
impl LenBuf for Vec<u8> {
    fn with_capacity(capacity: usize) -> Self {
        Vec::with_capacity(capacity)
    }

    fn append_slice(&mut self, data: &[u8]) {
        self.extend_from_slice(data);
    }

    fn from_read<R>(reader: &mut R, len: usize) -> Result<Self>
    where
        R: Read,
    {
        let mut data = vec![0u8; len];
        reader.read_exact(&mut data)?;
        Ok(data)
    }

    fn from_iter<I>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = u8>,
    {
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(iter.next().ok_or(ProtobufError::UnexpectedEof)?);
        }
        Ok(data)
    }

    fn from_try_iter<I, E>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(
                iter.next()
                    .ok_or(ProtobufError::UnexpectedEof)?
                    .map_err(Into::into)?,
            );
        }
        Ok(data)
    }
}

/// `Vec<u8, A>`. Enable the `nightly` feature to use the allocator.
///
/// The feature is not required when you do not use an allocator. `Vec<u8>`
/// implements this trait without it. The badge above marks this signature,
/// which is the one that takes an allocator.
#[cfg(feature = "nightly")]
#[doc(cfg(feature = "nightly"))]
#[allow(clippy::incompatible_msrv)]
impl<A> LenBuf for Vec<u8, A>
where
    A: Allocator + Default,
{
    fn with_capacity(capacity: usize) -> Self {
        Vec::with_capacity_in(capacity, A::default())
    }

    fn append_slice(&mut self, data: &[u8]) {
        self.extend_from_slice(data);
    }

    fn from_read<R>(reader: &mut R, len: usize) -> Result<Self>
    where
        R: Read,
    {
        // `read_exact` needs an initialized buffer. `A` is the vector's allocator,
        // so this cannot go through the `vec!` macro (that uses `Global`).
        let mut data = Vec::with_capacity_in(len, A::default());
        data.resize(len, 0);
        reader.read_exact(&mut data)?;
        Ok(data)
    }

    fn from_iter<I>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = u8>,
    {
        let mut data = Vec::with_capacity_in(len, A::default());
        for _ in 0..len {
            data.push(iter.next().ok_or(ProtobufError::UnexpectedEof)?);
        }
        Ok(data)
    }

    fn from_try_iter<I, E>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        let mut data = Vec::with_capacity_in(len, A::default());
        for _ in 0..len {
            data.push(
                iter.next()
                    .ok_or(ProtobufError::UnexpectedEof)?
                    .map_err(Into::into)?,
            );
        }
        Ok(data)
    }
}

/// Length-delimited payload stored in `Box<[u8]>`.
///
/// This implementation does not require the `nightly` feature and does not
/// take a custom allocator. `from_read`, `from_iter`, and `from_try_iter`
/// allocate one box of exactly `len` bytes and fill it. `from_read` uses
/// `read_exact`.
///
/// A boxed slice has no spare capacity. `with_capacity` returns an empty box.
/// `append_slice` allocates a new box that holds the bytes already stored
/// followed by `data`. Streaming readers call `from_*`.
#[cfg(not(feature = "nightly"))]
impl LenBuf for Box<[u8]> {
    fn with_capacity(_capacity: usize) -> Self {
        Box::default()
    }

    fn append_slice(&mut self, data: &[u8]) {
        let mut vec = ::std::mem::take(self).into_vec();
        vec.extend_from_slice(data);
        *self = vec.into_boxed_slice();
    }

    fn from_read<R>(reader: &mut R, len: usize) -> Result<Self>
    where
        R: Read,
    {
        let mut data = vec![0u8; len];
        reader.read_exact(&mut data)?;
        Ok(data.into_boxed_slice())
    }

    fn from_iter<I>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = u8>,
    {
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(iter.next().ok_or(ProtobufError::UnexpectedEof)?);
        }
        Ok(data.into_boxed_slice())
    }

    fn from_try_iter<I, E>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(
                iter.next()
                    .ok_or(ProtobufError::UnexpectedEof)?
                    .map_err(Into::into)?,
            );
        }
        Ok(data.into_boxed_slice())
    }
}

/// `Box<[u8], A>`. Enable the `nightly` feature to use the allocator.
///
/// `from_read`, `from_iter`, and `from_try_iter` allocate one box of exactly
/// `len` bytes and fill it. `from_read` uses `read_exact`.
///
/// The feature is not required when you do not use an allocator. `Box<[u8]>`
/// implements this trait without it. The badge above marks this signature,
/// which is the one that takes an allocator.
///
/// A boxed slice has no spare capacity. `with_capacity` returns an empty box.
/// `append_slice` allocates a new box that holds the bytes already stored
/// followed by `data`. Streaming readers call `from_*`.
#[cfg(feature = "nightly")]
#[doc(cfg(feature = "nightly"))]
#[allow(clippy::incompatible_msrv)]
impl<A> LenBuf for Box<[u8], A>
where
    A: Allocator + Default,
{
    fn with_capacity(_capacity: usize) -> Self {
        Vec::new_in(A::default()).into_boxed_slice()
    }

    fn append_slice(&mut self, data: &[u8]) {
        let empty = Vec::new_in(A::default()).into_boxed_slice();
        let mut vec = ::std::mem::replace(self, empty).into_vec();
        vec.extend_from_slice(data);
        *self = vec.into_boxed_slice();
    }

    fn from_read<R>(reader: &mut R, len: usize) -> Result<Self>
    where
        R: Read,
    {
        // `read_exact` needs an initialized buffer. `A` is the box's allocator,
        // so this cannot go through the `vec!` macro (that uses `Global`).
        // `into_boxed_slice` keeps this allocation when the length equals the capacity.
        let mut data = Vec::with_capacity_in(len, A::default());
        data.resize(len, 0);
        reader.read_exact(&mut data)?;
        Ok(data.into_boxed_slice())
    }

    fn from_iter<I>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = u8>,
    {
        let mut data = Vec::with_capacity_in(len, A::default());
        for _ in 0..len {
            data.push(iter.next().ok_or(ProtobufError::UnexpectedEof)?);
        }
        Ok(data.into_boxed_slice())
    }

    fn from_try_iter<I, E>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        let mut data = Vec::with_capacity_in(len, A::default());
        for _ in 0..len {
            data.push(
                iter.next()
                    .ok_or(ProtobufError::UnexpectedEof)?
                    .map_err(Into::into)?,
            );
        }
        Ok(data.into_boxed_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::{LEN_BUF_CHUNK, LenBuf};
    use crate::field::read::{IteratorExtProtobuf, ReadExtProtobuf, TryIteratorExtProtobuf};
    use crate::varint::Varint;
    use crate::{FieldValue, ProtobufError};
    use ::std::io::{Cursor, ErrorKind, Read};

    #[derive(Debug, PartialEq, Eq)]
    struct Chunks {
        parts: Vec<usize>,
        bytes: Vec<u8>,
    }

    impl LenBuf for Chunks {
        fn with_capacity(capacity: usize) -> Self {
            Self {
                parts: Vec::new(),
                bytes: Vec::with_capacity(capacity),
            }
        }

        fn append_slice(&mut self, data: &[u8]) {
            self.parts.push(data.len());
            self.bytes.extend_from_slice(data);
        }
    }

    pub(super) fn len_message(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0x0A];
        let (encoded, count) = Varint::from_uint64(payload.len() as u64).encode();
        out.extend_from_slice(&encoded[..count]);
        out.extend_from_slice(payload);
        out
    }

    pub(super) fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|index| index as u8).collect()
    }

    fn assert_chunks(field_bytes: &[u8], expected: &[u8]) {
        let owned: Vec<u8> = field_bytes.to_vec();

        let from_read = Cursor::new(owned.clone())
            .read_protobuf_fields::<Chunks>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(chunks) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(chunks.bytes, expected);
        if expected.is_empty() {
            assert!(chunks.parts.is_empty());
        } else {
            assert_eq!(chunks.parts.iter().sum::<usize>(), expected.len());
            assert!(chunks.parts.iter().all(|part| *part <= LEN_BUF_CHUNK));
            assert_eq!(chunks.parts.len(), expected.len().div_ceil(LEN_BUF_CHUNK));
        }

        let from_iter = owned
            .clone()
            .into_iter()
            .protobuf_fields::<Chunks>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(chunks) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(chunks.bytes, expected);

        let from_try = Cursor::new(owned)
            .bytes()
            .protobuf_fields::<Chunks>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(chunks) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(chunks.bytes, expected);
    }

    #[test]
    fn default_appends_cover_empty_small_and_multichunk_payloads() {
        assert_chunks(&len_message(b""), b"");
        assert_chunks(&len_message(b"Hel"), b"Hel");
        let big = payload(LEN_BUF_CHUNK + 3);
        assert_chunks(&len_message(&big), &big);
    }

    #[test]
    fn short_payload_is_an_error_on_every_streaming_source() {
        let input = [0x0A, 0x05, b'a', b'b'];
        // `Read::read_exact` reports a short buffer as an I/O error.
        let error = Cursor::new(input)
            .read_protobuf_fields::<Chunks>()
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(
            error,
            ProtobufError::IoError(ref err) if err.kind() == ErrorKind::UnexpectedEof
        ));

        let error = input
            .into_iter()
            .protobuf_fields::<Chunks>()
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));

        let error = Cursor::new(input)
            .bytes()
            .protobuf_fields::<Chunks>()
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    #[test]
    fn vec_reads_multichunk_payload_on_every_streaming_source() {
        let big = payload(LEN_BUF_CHUNK + 3);
        let message = len_message(&big);

        let from_read = Cursor::new(message.clone())
            .read_protobuf_fields::<Vec<u8>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(data, big);

        let from_iter = message
            .clone()
            .into_iter()
            .protobuf_fields::<Vec<u8>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(data, big);

        let from_try = Cursor::new(message)
            .bytes()
            .protobuf_fields::<Vec<u8>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(data, big);
    }

    fn assert_box(message: &[u8], expected: &[u8]) {
        let from_read = Cursor::new(message.to_vec())
            .read_protobuf_fields::<Box<[u8]>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], expected);

        let from_iter = message
            .iter()
            .copied()
            .protobuf_fields::<Box<[u8]>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], expected);

        let from_try = Cursor::new(message.to_vec())
            .bytes()
            .protobuf_fields::<Box<[u8]>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], expected);
    }

    #[test]
    fn box_reads_payload_on_every_streaming_source() {
        assert_box(&len_message(b""), b"");
        assert_box(&len_message(b"Hel"), b"Hel");
        let big = payload(LEN_BUF_CHUNK + 3);
        assert_box(&len_message(&big), &big);

        let mut data = <Box<[u8]> as LenBuf>::with_capacity(8);
        LenBuf::append_slice(&mut data, b"Hel");
        LenBuf::append_slice(&mut data, b"lo");
        assert_eq!(&data[..], b"Hello");
    }

    /// User buffer that takes the length-delimited payload in one shot.
    #[derive(Debug, PartialEq, Eq)]
    struct OwnedBytes {
        data: Vec<u8>,
        direct: bool,
    }

    impl LenBuf for OwnedBytes {
        fn with_capacity(capacity: usize) -> Self {
            Self {
                data: Vec::with_capacity(capacity),
                direct: false,
            }
        }

        fn append_slice(&mut self, data: &[u8]) {
            self.data.extend_from_slice(data);
        }

        fn from_read<R>(reader: &mut R, len: usize) -> crate::Result<Self>
        where
            R: Read,
        {
            let mut data = vec![0u8; len];
            reader.read_exact(&mut data)?;
            Ok(Self { data, direct: true })
        }

        fn from_iter<I>(iter: &mut I, len: usize) -> crate::Result<Self>
        where
            I: Iterator<Item = u8>,
        {
            let mut data = Vec::with_capacity(len);
            for _ in 0..len {
                data.push(iter.next().ok_or(ProtobufError::UnexpectedEof)?);
            }
            Ok(Self { data, direct: true })
        }

        fn from_try_iter<I, E>(iter: &mut I, len: usize) -> crate::Result<Self>
        where
            I: Iterator<Item = ::std::result::Result<u8, E>>,
            E: Into<ProtobufError>,
        {
            let mut data = Vec::with_capacity(len);
            for _ in 0..len {
                data.push(
                    iter.next()
                        .ok_or(ProtobufError::UnexpectedEof)?
                        .map_err(Into::into)?,
                );
            }
            Ok(Self { data, direct: true })
        }
    }

    fn assert_owned(message: &[u8], expected: &[u8]) {
        let from_read = Cursor::new(message.to_vec())
            .read_protobuf_fields::<OwnedBytes>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(owned) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(owned.data, expected);
        assert!(owned.direct);

        let from_iter = message
            .iter()
            .copied()
            .protobuf_fields::<OwnedBytes>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(owned) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(owned.data, expected);
        assert!(owned.direct);

        let from_try = Cursor::new(message.to_vec())
            .bytes()
            .protobuf_fields::<OwnedBytes>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(owned) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(owned.data, expected);
        assert!(owned.direct);
    }

    #[test]
    fn custom_len_buf_reads_payload_on_every_streaming_source() {
        assert_owned(&len_message(b""), b"");
        assert_owned(&len_message(b"Hel"), b"Hel");
        let big = payload(LEN_BUF_CHUNK + 3);
        assert_owned(&len_message(&big), &big);

        // A non-length field in the same stream stays a varint. Only the length
        // field uses `OwnedBytes`.
        let mixed = [0x08, 0x96, 0x01, 0x12, 0x03, b'H', b'e', b'l'];
        let fields = Cursor::new(mixed)
            .read_protobuf_fields::<OwnedBytes>()
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert!(matches!(fields[0].value, FieldValue::Varint(_)));
        assert_eq!(fields[0].field_number.as_u32(), 1);
        let FieldValue::Len(owned) = &fields[1].value else {
            panic!("expected len");
        };
        assert_eq!(owned.data, b"Hel");
        assert!(owned.direct);
        assert_eq!(fields[1].field_number.as_u32(), 2);
    }
}

#[cfg(all(test, feature = "nightly"))]
mod allocator_tests {
    use super::LEN_BUF_CHUNK;
    use super::tests::{len_message, payload};
    use crate::FieldValue;
    use crate::field::read::{IteratorExtProtobuf, ReadExtProtobuf, TryIteratorExtProtobuf};
    use ::std::alloc::{AllocError, Allocator, Global, Layout};
    use ::std::io::{Cursor, Read};
    use ::std::ptr::NonNull;
    use ::std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn vec_with_global_reads_multichunk_payload_on_every_streaming_source() {
        let big = payload(LEN_BUF_CHUNK + 3);
        let message = len_message(&big);

        let from_read = Cursor::new(message.clone())
            .read_protobuf_fields::<Vec<u8, Global>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(data, big);

        let from_iter = message
            .clone()
            .into_iter()
            .protobuf_fields::<Vec<u8, Global>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(data, big);

        let from_try = Cursor::new(message)
            .bytes()
            .protobuf_fields::<Vec<u8, Global>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(data, big);
    }

    #[test]
    fn box_with_global_reads_multichunk_payload_on_every_streaming_source() {
        let big = payload(LEN_BUF_CHUNK + 3);
        let message = len_message(&big);

        let from_read = Cursor::new(message.clone())
            .read_protobuf_fields::<Box<[u8], Global>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], big.as_slice());

        let from_iter = message
            .clone()
            .into_iter()
            .protobuf_fields::<Box<[u8], Global>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], big.as_slice());

        let from_try = Cursor::new(message)
            .bytes()
            .protobuf_fields::<Box<[u8], Global>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], big.as_slice());
    }

    static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
    static ALLOC_LOCK: ::std::sync::Mutex<()> = ::std::sync::Mutex::new(());

    struct Counting;

    impl Default for Counting {
        fn default() -> Self {
            Self
        }
    }

    unsafe impl Allocator for Counting {
        fn allocate(&self, layout: Layout) -> ::std::result::Result<NonNull<[u8]>, AllocError> {
            ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
            Global.allocate(layout)
        }

        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            unsafe { Global.deallocate(ptr, layout) }
        }
    }

    #[test]
    fn vec_with_custom_allocator_is_filled_by_read_exact() {
        let _guard = ALLOC_LOCK.lock().unwrap();
        ALLOCATIONS.store(0, Ordering::SeqCst);
        let message = len_message(b"Hel");
        let field = Cursor::new(message)
            .read_protobuf_fields::<Vec<u8, Counting>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = field.value else {
            panic!("expected len");
        };
        assert_eq!(data.as_slice(), b"Hel");
        assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 1);
        let _allocator: &Counting = data.allocator();
    }

    #[test]
    fn box_with_custom_allocator_is_filled_by_read_exact() {
        let _guard = ALLOC_LOCK.lock().unwrap();
        ALLOCATIONS.store(0, Ordering::SeqCst);
        let message = len_message(b"Hel");
        let field = Cursor::new(message)
            .read_protobuf_fields::<Box<[u8], Counting>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = field.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], b"Hel");
        assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 1);
        let _allocator: &Counting = ::std::boxed::Box::allocator(&data);
    }
}
