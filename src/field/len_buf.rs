//! Owned buffers for length-delimited payloads.
//!
//! Streaming readers ([`crate::ReadExtProtobuf`], [`crate::IteratorExtProtobuf`],
//! [`crate::TryIteratorExtProtobuf`]) build the `Len` payload with [`LenBuf`].
//! Slice readers borrow the input and do not use this trait.

use ::std::convert::AsMut;

#[cfg(feature = "nightly")]
use ::std::alloc::Allocator;

/// Buffer that can hold one length-delimited protobuf payload.
///
/// The payload length is known before its bytes are read. Implement
/// [`with_len`](LenBuf::with_len) and [`AsMut<[u8]>`](::std::convert::AsMut).
/// `with_len` allocates exactly that many initialized bytes, and
/// [`as_mut`](::std::convert::AsMut::as_mut) returns all of them. The slice
/// length is the payload length for the whole read. The streaming readers
/// allocate with `with_len` and write the payload bytes into that slice.
///
/// `Vec<u8>` and `Box<[u8]>` implement this trait. Both allocate `len` bytes.
/// A `Read` source overwrites them with `read_exact`.
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
    /// length. The streaming readers overwrite those bytes. Zero is a valid
    /// initial value.
    fn with_len(len: usize) -> Self;
}

/// Length-delimited payload stored in `Vec<u8>`.
///
/// This implementation does not require the `nightly` feature and does not
/// take a custom allocator. `with_len` allocates `len` zero bytes. A `Read`
/// source overwrites them with `read_exact`.
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
/// A `Read` source overwrites them with `read_exact`.
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

    #[test]
    fn custom_len_buf_keeps_a_varint_beside_the_length_field() {
        let mixed = [0x08, 0x96, 0x01, 0x12, 0x03, b'H', b'e', b'l'];
        let fields = Cursor::new(mixed)
            .read_protobuf_fields::<Bytes>()
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert!(matches!(fields[0].value, FieldValue::Varint(_)));
        assert_eq!(fields[0].field_number.as_u32(), 1);
        let FieldValue::Len(Bytes(data)) = &fields[1].value else {
            panic!("expected len");
        };
        assert_eq!(data, b"Hel");
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
