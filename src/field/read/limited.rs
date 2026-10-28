//! Streaming (`Read`) field iteration with an internal message-byte budget.

use super::group::GroupBody;
use super::wire::WireCursor;
use crate::field::{Field, FieldValue};
use crate::field_number::FieldNumber;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::{MAX_VARINT_BYTES, WireType};
use crate::{ProtobufError, Result};
use ::std::io::{self, Read, Write};

// ============================================================================
// Internal message budget (not part of the public API)
// ============================================================================

/// Default Len handler for [`ReadExtProtobuf::read_protobuf_fields`]: copy each
/// body into a new `Vec<u8>`.
///
/// Custom handlers are ordinary
/// `FnMut(FieldNumber, &mut R, u32) -> Result<(L, u32)>` closures passed to
/// [`ReadExtProtobuf::read_protobuf_fields_with`] — there is no separate handler
/// trait on the public API. The `u32` arguments are the Len payload length and
/// bytes consumed (trusted); the library skips any remainder.
#[derive(Debug, Default, Clone, Copy)]
pub struct VecLenPayload;

/// Copy one Len body into a `Vec<u8>` (used by [`VecLenPayload`]).
#[inline]
fn copy_len_to_vec<R: Read + ?Sized>(
    _field_number: FieldNumber,
    reader: &mut R,
    len: u32,
) -> Result<(Vec<u8>, u32)> {
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    Ok((buf, len))
}

/// Private length budget over an underlying [`Read`].
///
/// Used only inside this module so tag/varint/fixed reads cannot run past the
/// message `len` passed to [`ReadExtProtobuf`]. Not exported: Len handlers get
/// `&mut R` plus the payload length instead.
struct LimitedRead<'a, R: ?Sized> {
    inner: &'a mut R,
    remaining: usize,
}

impl<'a, R: Read + ?Sized> LimitedRead<'a, R> {
    #[inline]
    fn new(inner: &'a mut R, remaining: usize) -> Self {
        Self { inner, remaining }
    }

    /// Read `n` bytes from the underlying reader without changing `remaining`.
    ///
    /// Used after a Len handler returns a consumed count: the message budget
    /// was already reduced by the full payload length.
    #[inline]
    fn drain_inner(&mut self, mut n: usize) -> Result<()> {
        let mut buf = [0u8; 256];
        while n > 0 {
            let chunk = n.min(buf.len());
            Read::read_exact(self.inner, &mut buf[..chunk])?;
            n -= chunk;
        }
        Ok(())
    }
}

/// Parse one protobuf field from `limited`, using `handler` for each Len body.
///
/// Len handlers receive `&mut R` and the payload `length` (`u32` from the length
/// prefix). They return `(payload, consumed)`; `consumed` is trusted and any
/// remainder is drained from the underlying reader (message budget already
/// committed for the full length).
fn parse_protobuf_field_with<R, L, F>(
    limited: &mut LimitedRead<'_, R>,
    handler: &mut F,
) -> Result<Field<L>>
where
    R: Read + ?Sized,
    F: FnMut(FieldNumber, &mut R, u32) -> Result<(L, u32)>,
{
    let tag = WireCursor::read_tag(limited)?;
    let value = match tag.wire_type {
        WireType::Varint => FieldValue::Varint(WireCursor::read_varint(limited)?),
        WireType::Int32 => FieldValue::I32(WireCursor::read_fixed(limited)?),
        WireType::Int64 => FieldValue::I64(WireCursor::read_fixed(limited)?),
        WireType::Len => {
            let length = WireCursor::read_varint(limited)?.try_to_uint32()?;
            let length_usize = length as usize;
            if length_usize > limited.remaining {
                return Err(ProtobufError::UnexpectedEof);
            }
            limited.remaining -= length_usize;
            let (payload, consumed) = handler(tag.field_number, limited.inner, length)?;
            if consumed > length {
                return Err(ProtobufError::UnexpectedEof);
            }
            let unread = (length - consumed) as usize;
            if unread > 0 {
                limited.drain_inner(unread)?;
            }
            FieldValue::Len(payload)
        }
        WireType::SGroup => FieldValue::SGroup,
        WireType::EGroup => FieldValue::EGroup,
    };
    Ok(Field {
        field_number: tag.field_number,
        value,
    })
}

impl<R: Read + ?Sized> Read for LimitedRead<'_, R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || buf.is_empty() {
            return Ok(0);
        }
        let want = buf.len().min(self.remaining);
        let n = self.inner.read(&mut buf[..want])?;
        self.remaining -= n;
        Ok(n)
    }
}

impl<R: Read + ?Sized> WireCursor for LimitedRead<'_, R> {
    #[inline]
    fn remaining(&self) -> usize {
        self.remaining
    }

    #[inline]
    fn read_varint_with_raw<'a>(
        &'a mut self,
        buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'a [u8])> {
        use crate::varint::ReadExtVarint;
        ReadExtVarint::read_varint_with_raw(self, buf)
    }

    #[inline]
    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        Read::read_exact(self, &mut bytes)?;
        Ok(bytes)
    }

    #[inline]
    fn skip_bytes(&mut self, mut length: usize) -> Result<()> {
        let mut buf = [0u8; 256];
        while length > 0 {
            let n = length.min(buf.len());
            Read::read_exact(self, &mut buf[..n])?;
            length -= n;
        }
        Ok(())
    }

    #[inline]
    fn copy_bytes<W: Write>(&mut self, length: usize, out: &mut W) -> Result<usize> {
        let mut left = length;
        let mut buf = [0u8; 256];
        while left > 0 {
            let n = left.min(buf.len());
            Read::read_exact(self, &mut buf[..n])?;
            out.write_all(&buf[..n])?;
            left -= n;
        }
        Ok(length)
    }
}

// ============================================================================
// ReadExtProtobuf
// ============================================================================

/// Iterator for reading raw protobuf fields sequentially from a reader.
///
/// Bounded by the `len` passed to [`ReadExtProtobuf::read_protobuf_fields`].
/// Uses an internal message-byte budget over `R` — not nested [`std::io::Take`].
/// `F` is the Len-body handler (default [`VecLenPayload`], or a
/// `FnMut(FieldNumber, &mut R, u32) -> Result<(L, u32)>`).
pub struct FieldIterFromRead<R, F = VecLenPayload> {
    inner: R,
    remaining: usize,
    handler: F,
}

impl<R: Read, F> FieldIterFromRead<R, F> {
    /// Skip fields until `pred` returns `Ok(true)`.
    ///
    /// Intended mainly for protobuf **group** handling: after an
    /// [`FieldValue::SGroup`] was consumed, pair with [`until_group_ends`] to
    /// discard the body through the matching end tag (see
    /// [`FieldIterFromSlice::skip_until`] for a runnable example).
    ///
    /// Length-delimited payloads are discarded without allocating. The byte
    /// budget is this iterator's remaining count.
    pub fn skip_until<P>(&mut self, pred: P) -> Result<()>
    where
        P: FnMut(Tag) -> Result<bool>,
    {
        let mut limited = LimitedRead::new(&mut self.inner, self.remaining);
        let result = WireCursor::skip_until(&mut limited, pred);
        self.remaining = limited.remaining;
        result
    }

    /// Iterate fields until the matching end-group for `field_number`.
    ///
    /// Intended for parsing a protobuf **group** body after the start-group
    /// tag was consumed. See [`GroupBody`] for nesting rules (depth only;
    /// nested field numbers are the caller's job).
    #[inline]
    pub fn group_body(&mut self, field_number: FieldNumber) -> GroupBody<&mut Self> {
        GroupBody::new(self, field_number)
    }

    /// Copy fields to `out` until `pred` returns `Ok(true)`.
    ///
    /// Intended mainly for protobuf **group** preserve on a stream: after an
    /// [`FieldValue::SGroup`] was consumed, pair with [`until_group_ends`] to
    /// tee the body and matching end tag (start tag already consumed — write
    /// it yourself first if the preserve blob needs it). For zero-copy
    /// preserve from a slice, use [`FieldIterFromSlice::take_until`] instead.
    ///
    /// Same stop semantics as [`skip_until`](Self::skip_until). Writes exact
    /// wire bytes (including non-canonical varints). Returns the number of
    /// bytes written. On error, `out` may already contain a partial copy.
    pub fn copy_until<P, W>(&mut self, pred: P, out: &mut W) -> Result<usize>
    where
        P: FnMut(Tag) -> Result<bool>,
        W: Write,
    {
        let mut limited = LimitedRead::new(&mut self.inner, self.remaining);
        let result = WireCursor::copy_until(&mut limited, pred, out);
        self.remaining = limited.remaining;
        result
    }

    /// Bytes still allowed in this iterator's budget.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.remaining
    }
}

impl<R: Read> Iterator for FieldIterFromRead<R, VecLenPayload> {
    type Item = Result<Field<Vec<u8>>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let mut limited = LimitedRead::new(&mut self.inner, self.remaining);
        let result = parse_protobuf_field_with(&mut limited, &mut copy_len_to_vec);
        self.remaining = limited.remaining;
        Some(result)
    }
}

impl<R, L, F> Iterator for FieldIterFromRead<R, F>
where
    R: Read,
    F: FnMut(FieldNumber, &mut R, u32) -> Result<(L, u32)>,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let mut limited = LimitedRead::new(&mut self.inner, self.remaining);
        let result = parse_protobuf_field_with(&mut limited, &mut self.handler);
        self.remaining = limited.remaining;
        Some(result)
    }
}

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// Each method takes an explicit message byte budget `len` (host `usize`). Length-delimited
/// payloads default to `Vec<u8>` via [`VecLenPayload`].
/// [`read_protobuf_fields_with`](ReadExtProtobuf::read_protobuf_fields_with)
/// supplies a custom
/// `FnMut(FieldNumber, &mut R, u32) -> Result<(L, u32)>`
/// (field number, reader, Len payload length → payload + bytes consumed).
/// It does not provide semantic interpretation - that is the caller's responsibility.
///
/// # Example
/// ```
/// use ::protobuf_core::{ReadExtProtobuf, Field, FieldValue};
///
/// fn main() -> Result<(), Box<dyn ::std::error::Error>> {
///     let data = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
///     let len = data.len();
///     let reader = data;
///
///     for field in reader.read_protobuf_fields(len) {
///         let field = field?;
///         match field.value {
///             FieldValue::Varint(varint) => {
///                 println!("Field {}: {}", field.field_number.as_u32(), varint.to_uint64());
///             },
///             FieldValue::Len(data) => {
///                 println!("Field {}: {:?}", field.field_number.as_u32(), data);
///             },
///             _ => {}
///         }
///     }
///     Ok(())
/// }
/// ```
pub trait ReadExtProtobuf: Read {
    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Consumes at most `len` bytes of message budget.
    /// This consumes the reader and returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Length-delimited payloads are `Vec<u8>`.
    fn read_protobuf_fields(self, len: usize) -> FieldIterFromRead<Self>
    where
        Self: Sized;

    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Consumes at most `len` bytes of message budget.
    ///
    /// `handler` is called once for each length-delimited field with the field
    /// number, a mutable borrow of this reader, and the Len payload length
    /// (`u32` from the length prefix). Return
    /// [`Ok`](Result::Ok)`((payload, consumed))` with any payload type `L` and
    /// the number of body bytes consumed (trusted). The library skips
    /// `length - consumed` from the stream afterward. Return
    /// [`Err`](Result::Err) to fail the field. Varint and fixed-width fields do
    /// not call `handler`.
    ///
    /// Nested messages can call [`read_protobuf_fields`](Self::read_protobuf_fields)
    /// / [`read_protobuf_fields_with`](Self::read_protobuf_fields_with) again on
    /// `&mut R` with `len as usize` as the nested budget.
    ///
    /// # Examples
    ///
    /// Copy each Len body into a `Box<[u8]>`:
    ///
    /// ```
    /// use ::protobuf_core::{FieldValue, ReadExtProtobuf};
    /// use ::std::io::Read;
    ///
    /// let data = &[0x0A, 0x03, b'H', b'e', b'l'][..];
    /// let len = data.len();
    /// let fields: Vec<_> = data
    ///     .read_protobuf_fields_with(len, |_n, r, body_len| {
    ///         let mut buf = vec![0u8; body_len as usize];
    ///         r.read_exact(&mut buf)?;
    ///         Ok((buf.into_boxed_slice(), body_len))
    ///     })
    ///     .collect::<Result<Vec<_>, _>>()
    ///     .unwrap();
    /// let FieldValue::Len(data) = &fields[0].value else {
    ///     panic!("expected a length-delimited field");
    /// };
    /// assert_eq!(&data[..], b"Hel");
    /// ```
    fn read_protobuf_fields_with<L, F>(self, len: usize, handler: F) -> FieldIterFromRead<Self, F>
    where
        Self: Sized,
        F: FnMut(FieldNumber, &mut Self, u32) -> Result<(L, u32)>;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_fields(self, len: usize) -> FieldIterFromRead<Self>
    where
        Self: Sized,
    {
        FieldIterFromRead {
            inner: self,
            remaining: len,
            handler: VecLenPayload,
        }
    }

    fn read_protobuf_fields_with<L, F>(self, len: usize, handler: F) -> FieldIterFromRead<Self, F>
    where
        Self: Sized,
        F: FnMut(FieldNumber, &mut Self, u32) -> Result<(L, u32)>,
    {
        FieldIterFromRead {
            inner: self,
            remaining: len,
            handler,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::field::FieldValue;

    #[test]
    fn test_read_ext_read_varint_field() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![0x08, 0x96, 0x01]; // field 1: 150
        let len = data.len();
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields(len)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 1);
        match &fields[0].value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }
    }

    #[test]
    fn test_read_ext_read_multiple_fields() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ];
        let len = data.len();
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields(len)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 2);

        // Check first field
        assert_eq!(fields[0].field_number.as_u32(), 1);
        match &fields[0].value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }

        // Check second field
        assert_eq!(fields[1].field_number.as_u32(), 2);
        match &fields[1].value {
            FieldValue::Len(data) => {
                assert_eq!(&data[..], b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_read_ext_read_i32_field() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![0x15, 0x78, 0x56, 0x34, 0x12]; // field 2: 0x12345678 (Fixed32)
        let len = data.len();
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields(len)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 2);
        match &fields[0].value {
            FieldValue::I32(bytes) => {
                assert_eq!(*bytes, [0x78, 0x56, 0x34, 0x12]);
            }
            _ => panic!("Expected I32 field"),
        }
    }

    #[test]
    fn test_read_ext_read_len_field() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![0x12, 0x03, 0x48, 0x65, 0x6c]; // field 2: "Hel"
        let len = data.len();
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields(len)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 2);
        match &fields[0].value {
            FieldValue::Len(data) => {
                assert_eq!(&data[..], b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_read_ext_read_i64_field() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![0x19, 0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]; // field 3: 0x1234567890ABCDEF (Fixed64)
        let len = data.len();
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields(len)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 3);
        match &fields[0].value {
            FieldValue::I64(bytes) => {
                assert_eq!(*bytes, [0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]);
            }
            _ => panic!("Expected I64 field"),
        }
    }

    #[test]
    fn test_read_ext_read_empty_stream() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![];
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields(0)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 0);
    }

    #[test]
    fn test_read_ext_respects_len_budget() {
        use super::ReadExtProtobuf;
        use ::std::io::{Cursor, Read};

        // Two fields; budget covers only the first.
        let data = vec![
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ];
        let mut reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .by_ref()
            .read_protobuf_fields(3)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 1);
        // Remaining bytes after the budget are still on the cursor.
        assert_eq!(reader.position(), 3);
        let rest = reader.into_inner();
        assert_eq!(&rest[3..], &[0x12, 0x03, 0x48, 0x65, 0x6c]);
    }

    #[test]
    fn test_read_ext_oversized_budget_is_unexpected_eof() {
        use super::ReadExtProtobuf;
        use crate::ProtobufError;
        use ::std::io::Cursor;

        // One complete field (3 bytes), but the budget claims more than the
        // underlying reader can supply. After the field is consumed, the local
        // budget still has remaining capacity while the cursor is at EOF, so
        // the next poll is UnexpectedEof (not a soft end).
        let data = vec![0x08, 0x96, 0x01]; // field 1: 150
        let result: ::std::result::Result<Vec<_>, _> =
            Cursor::new(data).read_protobuf_fields(100).collect();
        assert!(
            matches!(result, Err(ProtobufError::UnexpectedEof)),
            "got {result:?}"
        );
    }

    #[test]
    fn test_read_ext_truncated_fixed_and_len() {
        use super::ReadExtProtobuf;
        use crate::ProtobufError;
        use ::std::io::Cursor;

        // Mid-field truncation with budget == input.len(): fixed-width paths
        // surface as IoError via `Read::read_exact`; Len that claims more than
        // the remaining budget fails in `reborrow_len` as UnexpectedEof.
        use ::std::io::ErrorKind;

        let fixed_err = Cursor::new([0x0D, 0x01, 0x02, 0x03])
            .read_protobuf_fields(4)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(
            fixed_err,
            ProtobufError::IoError(ref err) if err.kind() == ErrorKind::UnexpectedEof
        ));

        let len_err = Cursor::new([0x0A, 0x05, b'a', b'b'])
            .read_protobuf_fields(4)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(len_err, ProtobufError::UnexpectedEof));
    }
}

#[cfg(test)]
mod len_payload {
    use super::ReadExtProtobuf;
    use crate::varint::Varint;
    use crate::{Field, FieldNumber, FieldValue, ProtobufError};
    use ::std::io::{Cursor, Read};

    const LARGE_LEN: usize = 8200;

    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    fn read_bytes_payload<R: Read>(
        _n: FieldNumber,
        reader: &mut R,
        len: u32,
    ) -> crate::Result<(Bytes, u32)> {
        let mut data = vec![0; len as usize];
        reader.read_exact(&mut data)?;
        Ok((Bytes(data), len))
    }

    fn len_message(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0x0A];
        let (encoded, count) = Varint::from_uint64(payload.len() as u64).encode();
        out.extend_from_slice(&encoded[..count]);
        out.extend_from_slice(payload);
        out
    }

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|index| index as u8).collect()
    }

    fn assert_bytes(field_bytes: &[u8], expected: &[u8]) {
        let owned = field_bytes.to_vec();
        let len = owned.len();
        let from_read = Cursor::new(owned)
            .read_protobuf_fields_with(len, read_bytes_payload)
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(Bytes(data)) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);
    }

    #[test]
    fn closure_fills_empty_small_and_large_payloads() {
        assert_bytes(&len_message(b""), b"");
        assert_bytes(&len_message(b"Hel"), b"Hel");
        let big = payload(LARGE_LEN);
        assert_bytes(&len_message(&big), &big);
    }

    #[test]
    fn short_payload_is_an_error_on_read() {
        let input = [0x0A, 0x05, b'a', b'b'];
        // Length prefix claims 5 bytes but the message budget only has 2 left.
        let error = Cursor::new(input)
            .read_protobuf_fields_with(input.len(), read_bytes_payload)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    fn assert_box(message: &[u8], expected: &[u8]) {
        let from_read = Cursor::new(message.to_vec())
            .read_protobuf_fields_with(message.len(), |_n, r, body_len| {
                let mut buf = vec![0u8; body_len as usize];
                r.read_exact(&mut buf)?;
                Ok((buf.into_boxed_slice(), body_len))
            })
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], expected);
    }

    #[test]
    fn box_closure_reads_payload_on_read() {
        assert_box(&len_message(b""), b"");
        assert_box(&len_message(b"Hel"), b"Hel");
        let big = payload(LARGE_LEN);
        assert_box(&len_message(&big), &big);
    }

    #[derive(Debug)]
    struct Tagged {
        id: u64,
        data: Vec<u8>,
    }

    fn two_len_fields() -> Vec<u8> {
        let mut message = len_message(b"Hel");
        message.extend_from_slice(&len_message(b"lo!"));
        message
    }

    fn assert_tagged(fields: &[Field<Tagged>], id: u64) {
        assert_eq!(fields.len(), 2);
        let FieldValue::Len(first) = &fields[0].value else {
            panic!("expected len");
        };
        let FieldValue::Len(second) = &fields[1].value else {
            panic!("expected len");
        };
        assert_eq!(first.data, b"Hel");
        assert_eq!(second.data, b"lo!");
        assert_eq!(first.id, id);
        assert_eq!(second.id, id);
    }

    #[test]
    fn closure_captures_state_for_each_length_field() {
        let message = two_len_fields();
        let id = 7u64;
        let len = message.len();

        let from_read = Cursor::new(message)
            .read_protobuf_fields_with(len, |_n, r, body_len| {
                let mut data = vec![0; body_len as usize];
                r.read_exact(&mut data)?;
                Ok((Tagged { id, data }, body_len))
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_tagged(&from_read, id);
    }

    #[test]
    fn closure_runs_only_for_length_fields() {
        let mixed = [0x08, 0x96, 0x01, 0x12, 0x03, b'H', b'e', b'l'];
        let mut calls = 0usize;
        let fields = Cursor::new(mixed)
            .read_protobuf_fields_with(mixed.len(), |_n, r, body_len| {
                calls += 1;
                read_bytes_payload(_n, r, body_len)
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(calls, 1);
        assert!(matches!(fields[0].value, FieldValue::Varint(_)));
        assert_eq!(fields[0].field_number.as_u32(), 1);
        let FieldValue::Len(Bytes(data)) = &fields[1].value else {
            panic!("expected len");
        };
        assert_eq!(data, b"Hel");
        assert_eq!(fields[1].field_number.as_u32(), 2);
    }

    #[test]
    fn closure_error_fails_the_field() {
        let input = [0x0A, 0x03, b'H', b'e', b'l'];
        let error = Cursor::new(input)
            .read_protobuf_fields_with(
                input.len(),
                |_n, _r, _len| -> crate::Result<(Vec<u8>, u32)> {
                    Err(ProtobufError::UnexpectedEof)
                },
            )
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    #[derive(Debug)]
    struct Spare {
        data: Vec<u8>,
    }

    fn assert_spare(fields: &[Field<Spare>]) {
        assert_eq!(fields.len(), 2);
        let FieldValue::Len(buf) = &fields[0].value else {
            panic!("expected len");
        };
        assert_eq!(&buf.data[..3], b"Hel");
        assert_eq!(&buf.data[3..], &[0xAB; 4]);
        let FieldValue::Varint(varint) = &fields[1].value else {
            panic!("expected varint");
        };
        assert_eq!(varint.to_uint64(), 1);
        assert_eq!(fields[1].field_number.as_u32(), 2);
    }

    #[test]
    fn longer_buffer_keeps_the_next_field_and_the_suffix() {
        let message = [0x0A, 0x03, b'H', b'e', b'l', 0x10, 0x01];

        let from_read = Cursor::new(message)
            .read_protobuf_fields_with(message.len(), |_n, r, body_len| {
                let n = body_len as usize;
                let mut data = vec![0xAB; n + 4];
                r.read_exact(&mut data[..n])?;
                Ok((Spare { data }, body_len))
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_spare(&from_read);
    }

    #[test]
    fn unread_body_bytes_are_skipped() {
        // Handler reads only the first byte; the rest of the Len body must be
        // drained so the following varint field is still visible.
        let message = [0x0A, 0x03, b'H', b'e', b'l', 0x10, 0x01];
        let fields = Cursor::new(message)
            .read_protobuf_fields_with(message.len(), |_n, r, _body_len| {
                let mut one = [0u8; 1];
                r.read_exact(&mut one)?;
                Ok((one[0], 1u32))
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(fields.len(), 2);
        assert!(matches!(fields[0].value, FieldValue::Len(b'H')));
        let FieldValue::Varint(v) = &fields[1].value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
    }

    #[derive(Debug, Default, PartialEq)]
    struct Child {
        id: u64,
        name: Vec<u8>,
    }

    #[derive(Debug, PartialEq)]
    enum LenValue {
        Bytes(Vec<u8>),
        Child(Child),
    }

    #[test]
    fn nested_message_via_on_len() {
        // field 1 string "Hi"; field 2 message { id=7, name="ok" }
        let wire: &[u8] = &[
            0x0A, 0x02, b'H', b'i', //
            0x12, 0x06, //
            0x08, 0x07, //
            0x1A, 0x02, b'o', b'k',
        ];

        fn decode_len<R: Read>(
            field_number: FieldNumber,
            reader: &mut R,
            len: u32,
        ) -> crate::Result<(LenValue, u32)> {
            match field_number.as_u32() {
                2 => {
                    let mut child = Child::default();
                    let nested = reader
                        .read_protobuf_fields_with(len as usize, |inner_n, inner, inner_len| {
                            let mut bytes = vec![0u8; inner_len as usize];
                            inner.read_exact(&mut bytes)?;
                            let _ = inner_n;
                            Ok((LenValue::Bytes(bytes), inner_len))
                        })
                        .collect::<crate::Result<Vec<_>>>()?;
                    for field in nested {
                        match (field.field_number.as_u32(), field.value) {
                            (1, FieldValue::Varint(v)) => child.id = v.to_uint64(),
                            (3, FieldValue::Len(LenValue::Bytes(name))) => child.name = name,
                            _ => {}
                        }
                    }
                    Ok((LenValue::Child(child), len))
                }
                _ => {
                    let mut bytes = vec![0u8; len as usize];
                    reader.read_exact(&mut bytes)?;
                    Ok((LenValue::Bytes(bytes), len))
                }
            }
        }

        let fields = Cursor::new(wire)
            .read_protobuf_fields_with(wire.len(), decode_len)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(fields.len(), 2);
        assert!(matches!(
            &fields[0].value,
            FieldValue::Len(LenValue::Bytes(b)) if b == b"Hi"
        ));
        assert!(matches!(
            &fields[1].value,
            FieldValue::Len(LenValue::Child(Child { id: 7, name })) if name == b"ok"
        ));
    }
}
