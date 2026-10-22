//! Owned buffers for length-delimited payloads.
//!
//! Streaming readers ([`crate::ReadExtProtobuf`], [`crate::IteratorExtProtobuf`],
//! [`crate::TryIteratorExtProtobuf`]) build the `Len` payload with [`LenBuf`].
//! Slice readers borrow the input and do not use this trait.

use crate::{ProtobufError, Result};
use ::std::convert::AsMut;
use ::std::io::Read;

#[cfg(feature = "nightly")]
use ::std::alloc::Allocator;

/// Buffer that can hold one length-delimited protobuf payload.
///
/// The payload length is known before its bytes are read. Implement
/// [`with_len`](LenBuf::with_len) and [`AsMut<[u8]>`](::std::convert::AsMut).
/// `with_len` allocates exactly that many initialized bytes, and
/// [`as_mut`](::std::convert::AsMut::as_mut) returns all of them. The slice
/// length is the payload length for the whole read. The `from_*` methods have
/// defaults that allocate with `with_len` and write into that slice.
///
/// `Vec<u8>` and `Box<[u8]>` implement this trait. Both allocate `len` bytes
/// and `from_read` overwrites them with `read_exact`.
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
/// impl ::std::convert::AsMut<[u8]> for Bytes {
///     fn as_mut(&mut self) -> &mut [u8] {
///         &mut self.0
///     }
/// }
///
/// impl LenBuf for Bytes {
///     fn with_len(len: usize) -> Self {
///         Self(vec![0; len])
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
pub trait LenBuf: Sized + AsMut<[u8]> {
    /// Allocate a buffer of exactly `len` initialized bytes.
    ///
    /// [`AsMut::as_mut`](::std::convert::AsMut::as_mut) returns a slice of this
    /// length. The bytes are overwritten by [`from_read`](LenBuf::from_read),
    /// [`from_iter`](LenBuf::from_iter), and [`from_try_iter`](LenBuf::from_try_iter).
    /// Zero is a valid initial value.
    fn with_len(len: usize) -> Self;

    /// Read exactly `len` payload bytes from `reader`.
    ///
    /// The default allocates with [`with_len`](LenBuf::with_len) and fills that
    /// slice with `read_exact`.
    fn from_read<R>(reader: &mut R, len: usize) -> Result<Self>
    where
        R: Read,
    {
        let mut buf = Self::with_len(len);
        reader.read_exact(buf.as_mut())?;
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
    /// The default allocates with [`with_len`](LenBuf::with_len) and writes
    /// each byte into that slice.
    fn from_try_iter<I, E>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        let mut buf = Self::with_len(len);
        for slot in buf.as_mut() {
            *slot = iter
                .next()
                .ok_or(ProtobufError::UnexpectedEof)?
                .map_err(Into::into)?;
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
/// take a custom allocator. `with_len` allocates `len` zero bytes. `from_read`
/// overwrites them with `read_exact`.
#[cfg(not(feature = "nightly"))]
impl LenBuf for Vec<u8> {
    fn with_len(len: usize) -> Self {
        vec![0u8; len]
    }
}

/// Length-delimited payload stored in `Box<[u8]>`.
///
/// This implementation does not require the `nightly` feature and does not
/// take a custom allocator. `with_len` allocates one box of `len` zero bytes.
/// `from_read` overwrites them with `read_exact`.
#[cfg(not(feature = "nightly"))]
impl LenBuf for Box<[u8]> {
    fn with_len(len: usize) -> Self {
        vec![0u8; len].into_boxed_slice()
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
    fn with_len(len: usize) -> Self {
        // `read_exact` needs an initialized buffer. `A` is the vector's allocator,
        // so this cannot go through the `vec!` macro (that uses `Global`).
        let mut data = Vec::with_capacity_in(len, A::default());
        data.resize(len, 0);
        data
    }
}

/// `Box<[u8], A>`. Enable the `nightly` feature to use the allocator.
///
/// The feature is not required when you do not use an allocator. `Box<[u8]>`
/// implements this trait without it. The badge above marks this signature,
/// which is the one that takes an allocator.
#[cfg(feature = "nightly")]
#[doc(cfg(feature = "nightly"))]
#[allow(clippy::incompatible_msrv)]
impl<A> LenBuf for Box<[u8], A>
where
    A: Allocator + Default,
{
    fn with_len(len: usize) -> Self {
        // `read_exact` needs an initialized buffer. `A` is the box's allocator,
        // so this cannot go through the `vec!` macro (that uses `Global`).
        // `into_boxed_slice` keeps this allocation when the length equals the capacity.
        let mut data = Vec::with_capacity_in(len, A::default());
        data.resize(len, 0);
        data.into_boxed_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::LenBuf;
    use crate::field::read::{IteratorExtProtobuf, ReadExtProtobuf, TryIteratorExtProtobuf};
    use crate::varint::Varint;
    use crate::{FieldValue, ProtobufError};
    use ::std::convert::AsMut;
    use ::std::io::{Cursor, ErrorKind, Read};

    pub(super) const LARGE_LEN: usize = 8200;

    /// Custom buffer that uses only `with_len` and [`AsMut<[u8]>`](::std::convert::AsMut).
    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    impl AsMut<[u8]> for Bytes {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }
    }

    impl LenBuf for Bytes {
        fn with_len(len: usize) -> Self {
            Self(vec![0; len])
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

    fn assert_bytes(field_bytes: &[u8], expected: &[u8]) {
        let owned: Vec<u8> = field_bytes.to_vec();

        let from_read = Cursor::new(owned.clone())
            .read_protobuf_fields::<Bytes>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(Bytes(data)) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);

        let from_iter = owned
            .clone()
            .into_iter()
            .protobuf_fields::<Bytes>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(Bytes(data)) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);

        let from_try = Cursor::new(owned)
            .bytes()
            .protobuf_fields::<Bytes>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(Bytes(data)) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);
    }

    #[test]
    fn default_fill_covers_empty_small_and_large_payloads() {
        assert_bytes(&len_message(b""), b"");
        assert_bytes(&len_message(b"Hel"), b"Hel");
        let big = payload(LARGE_LEN);
        assert_bytes(&len_message(&big), &big);
    }

    #[test]
    fn short_payload_is_an_error_on_every_streaming_source() {
        let input = [0x0A, 0x05, b'a', b'b'];
        // `Read::read_exact` reports a short buffer as an I/O error.
        let error = Cursor::new(input)
            .read_protobuf_fields::<Bytes>()
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(
            error,
            ProtobufError::IoError(ref err) if err.kind() == ErrorKind::UnexpectedEof
        ));

        let error = input
            .into_iter()
            .protobuf_fields::<Bytes>()
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));

        let error = Cursor::new(input)
            .bytes()
            .protobuf_fields::<Bytes>()
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    fn assert_vec(message: &[u8], expected: &[u8]) {
        let from_read = Cursor::new(message.to_vec())
            .read_protobuf_fields::<Vec<u8>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);

        let from_iter = message
            .iter()
            .copied()
            .protobuf_fields::<Vec<u8>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);

        let from_try = Cursor::new(message.to_vec())
            .bytes()
            .protobuf_fields::<Vec<u8>>()
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);
    }

    #[test]
    fn vec_reads_payload_on_every_streaming_source() {
        assert_vec(&len_message(b""), b"");
        assert_vec(&len_message(b"Hel"), b"Hel");
        let big = payload(LARGE_LEN);
        assert_vec(&len_message(&big), &big);
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
        let big = payload(LARGE_LEN);
        assert_box(&len_message(&big), &big);
    }

    /// User buffer that replaces the default fill and records that it ran.
    #[derive(Debug, PartialEq, Eq)]
    struct OwnedBytes {
        data: Vec<u8>,
        direct: bool,
    }

    impl AsMut<[u8]> for OwnedBytes {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.data
        }
    }

    impl LenBuf for OwnedBytes {
        fn with_len(len: usize) -> Self {
            Self {
                data: vec![0; len],
                direct: false,
            }
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
            let mut data = vec![0u8; len];
            for slot in &mut data {
                *slot = iter.next().ok_or(ProtobufError::UnexpectedEof)?;
            }
            Ok(Self { data, direct: true })
        }

        fn from_try_iter<I, E>(iter: &mut I, len: usize) -> crate::Result<Self>
        where
            I: Iterator<Item = ::std::result::Result<u8, E>>,
            E: Into<ProtobufError>,
        {
            let mut data = vec![0u8; len];
            for slot in &mut data {
                *slot = iter
                    .next()
                    .ok_or(ProtobufError::UnexpectedEof)?
                    .map_err(Into::into)?;
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
        let big = payload(LARGE_LEN);
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
    use super::tests::{LARGE_LEN, len_message, payload};
    use crate::FieldValue;
    use crate::field::read::{IteratorExtProtobuf, ReadExtProtobuf, TryIteratorExtProtobuf};
    use ::std::alloc::{AllocError, Allocator, Global, Layout};
    use ::std::io::{Cursor, Read};
    use ::std::ptr::NonNull;
    use ::std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn vec_with_global_reads_payload_on_every_streaming_source() {
        let big = payload(LARGE_LEN);
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
    fn box_with_global_reads_payload_on_every_streaming_source() {
        let big = payload(LARGE_LEN);
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
    fn vec_with_custom_allocator_is_one_allocation() {
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
    fn box_with_custom_allocator_is_one_allocation() {
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
