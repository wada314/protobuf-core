//! Streaming (`Read`) field iteration with a local [`LimitedRead`] budget.

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
// LimitedRead budget (no nested std::io::Take)
// ============================================================================

/// Default Len handler for [`ReadExtProtobuf::read_protobuf_fields`]: copy each
/// body into a new `Vec<u8>`.
///
/// Custom handlers are ordinary
/// `FnMut(FieldNumber, LimitedRead<'_, R>) -> Result<(L, usize)>` closures
/// passed to [`ReadExtProtobuf::read_protobuf_fields_with`] — there is no
/// separate handler trait on the public API. The `usize` is bytes consumed from
/// the body (trusted); the library skips any remainder.
#[derive(Debug, Default, Clone, Copy)]
pub struct VecLenPayload;

/// Copy one Len body into a `Vec<u8>` (used by [`VecLenPayload`]).
#[inline]
fn copy_len_to_vec<R: Read + ?Sized>(
    _field_number: FieldNumber,
    mut body: LimitedRead<'_, R>,
) -> Result<(Vec<u8>, usize)> {
    let len = body.remaining();
    let mut buf = vec![0u8; len];
    body.read_exact(&mut buf)?;
    Ok((buf, len))
}

/// A length budget over an underlying [`Read`].
///
/// Nesting does **not** wrap another adapter around `R`. [`reborrow_len`](Self::reborrow_len)
/// commits `len` bytes from this budget and returns a child over the same `&mut R`.
/// Boundary checks stay O(1) per read regardless of nesting depth.
pub struct LimitedRead<'a, R: ?Sized> {
    inner: &'a mut R,
    remaining: usize,
}

impl<'a, R: Read + ?Sized> LimitedRead<'a, R> {
    /// Bound `inner` to at most `remaining` bytes.
    #[inline]
    pub fn new(inner: &'a mut R, remaining: usize) -> Self {
        Self { inner, remaining }
    }

    /// Bytes still allowed in this budget.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Commit `len` bytes from this budget and return a child over the same `R`.
    ///
    /// The parent's [`remaining`](Self::remaining) decreases by `len` immediately.
    /// Nested message parsing uses the child. After the Len handler returns its
    /// consumed byte count, [`LimitedReadExtProtobuf::parse_protobuf_field`]
    /// drains any remainder from the underlying reader without touching this
    /// budget again.
    #[inline]
    pub fn reborrow_len(&mut self, len: usize) -> Result<LimitedRead<'_, R>> {
        if len > self.remaining {
            return Err(ProtobufError::UnexpectedEof);
        }
        self.remaining -= len;
        Ok(LimitedRead {
            inner: self.inner,
            remaining: len,
        })
    }

    /// Read `n` bytes from the underlying reader without changing [`remaining`](Self::remaining).
    ///
    /// Used after a Len handler returns a consumed count: the parent budget was
    /// already committed by [`reborrow_len`](Self::reborrow_len).
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
/// Len handlers take the body by value and return `(payload, consumed)`.
/// `consumed` is trusted; any `length - consumed` remainder is drained from the
/// underlying reader (parent budget already committed).
fn parse_protobuf_field_with<R, L, F>(
    limited: &mut LimitedRead<'_, R>,
    handler: &mut F,
) -> Result<Field<L>>
where
    R: Read + ?Sized,
    F: FnMut(FieldNumber, LimitedRead<'_, R>) -> Result<(L, usize)>,
{
    let tag = WireCursor::read_tag(limited)?;
    let value = match tag.wire_type {
        WireType::Varint => FieldValue::Varint(WireCursor::read_varint(limited)?),
        WireType::Int32 => FieldValue::I32(WireCursor::read_fixed(limited)?),
        WireType::Int64 => FieldValue::I64(WireCursor::read_fixed(limited)?),
        WireType::Len => {
            let length = WireCursor::read_varint(limited)?.try_to_uint32()? as usize;
            let body = limited.reborrow_len(length)?;
            let (payload, consumed) = handler(tag.field_number, body)?;
            if consumed > length {
                return Err(ProtobufError::UnexpectedEof);
            }
            let unread = length - consumed;
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

/// Protobuf field parsing over a [`LimitedRead`] budget.
///
/// Kept separate from [`LimitedRead`] so the budget type stays a generic
/// length-limited [`Read`], not a protobuf message API.
///
/// Len handlers are
/// `FnMut(FieldNumber, LimitedRead<'_, Self::Reader>) -> Result<(L, usize)>`
/// (`usize` = bytes consumed from the body).
pub trait LimitedReadExtProtobuf {
    /// Underlying reader inside this [`LimitedRead`].
    type Reader: Read + ?Sized;

    /// Parse one field. Soft end is the caller's [`LimitedRead::remaining`]
    /// `== 0` check; mid-field emptiness is [`ProtobufError::UnexpectedEof`].
    fn parse_protobuf_field<L, F>(&mut self, handler: &mut F) -> Result<Field<L>>
    where
        F: FnMut(FieldNumber, LimitedRead<'_, Self::Reader>) -> Result<(L, usize)>;

    /// Collect every field in this budget.
    ///
    /// Useful inside a Len handler for nested messages. Prefer
    /// [`ReadExtProtobuf::read_protobuf_fields`] at the top level when an
    /// iterator is enough. Named differently from that method so both traits
    /// can be in scope on a [`LimitedRead`] (which also implements [`Read`]).
    fn collect_protobuf_fields<L, F>(&mut self, handler: &mut F) -> Result<Vec<Field<L>>>
    where
        F: FnMut(FieldNumber, LimitedRead<'_, Self::Reader>) -> Result<(L, usize)>;
}

impl<R: Read + ?Sized> LimitedReadExtProtobuf for LimitedRead<'_, R> {
    type Reader = R;

    #[inline]
    fn parse_protobuf_field<L, F>(&mut self, handler: &mut F) -> Result<Field<L>>
    where
        F: FnMut(FieldNumber, LimitedRead<'_, R>) -> Result<(L, usize)>,
    {
        parse_protobuf_field_with(self, handler)
    }

    fn collect_protobuf_fields<L, F>(&mut self, handler: &mut F) -> Result<Vec<Field<L>>>
    where
        F: FnMut(FieldNumber, LimitedRead<'_, R>) -> Result<(L, usize)>,
    {
        let mut fields = Vec::new();
        while self.remaining() > 0 {
            fields.push(self.parse_protobuf_field(handler)?);
        }
        Ok(fields)
    }
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
        // `ReadExtVarint` reads through `Read`, which updates `self.remaining`.
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
/// Uses a local budget ([`LimitedRead`]) over `R` — not nested [`std::io::Take`].
/// `F` is the Len-body handler (default [`VecLenPayload`], or a
/// `FnMut(FieldNumber, LimitedRead<'_, R>) -> Result<(L, usize)>`).
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
    F: FnMut(FieldNumber, LimitedRead<'_, R>) -> Result<(L, usize)>,
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
/// Each method takes an explicit byte budget `len` and tracks it with a local
/// [`LimitedRead`] over the reader (not [`std::io::Take`]). Length-delimited
/// payloads default to `Vec<u8>` via [`VecLenPayload`].
/// [`read_protobuf_fields_with`](ReadExtProtobuf::read_protobuf_fields_with)
/// supplies a custom
/// `FnMut(FieldNumber, LimitedRead<'_, R>) -> Result<(L, usize)>`
/// (nested parse, custom buffer, …). The `usize` is bytes consumed from the
/// Len body; unread remainder is skipped.
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
    /// Consumes at most `len` bytes (local budget; see [`LimitedRead`]).
    /// This consumes the reader and returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Length-delimited payloads are `Vec<u8>`.
    fn read_protobuf_fields(self, len: usize) -> FieldIterFromRead<Self>
    where
        Self: Sized;

    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Consumes at most `len` bytes (local budget; see [`LimitedRead`]).
    ///
    /// `handler` is called once for each length-delimited field with the field
    /// number and a [`LimitedRead`] body (by value) already bounded to the
    /// payload length. Return [`Ok`](Result::Ok)`((payload, consumed))` with any
    /// payload type `L` and the number of body bytes consumed (trusted). The
    /// library skips `length - consumed` from the stream afterward. Return
    /// [`Err`](Result::Err) to fail the field. Varint and fixed-width fields do
    /// not call `handler`.
    ///
    /// # Examples
    ///
    /// Copy each Len body into a `Box<[u8]>`:
    ///
    /// ```
    /// use ::protobuf_core::{FieldValue, LimitedRead, ReadExtProtobuf};
    /// use ::std::io::Read;
    ///
    /// let data = &[0x0A, 0x03, b'H', b'e', b'l'][..];
    /// let len = data.len();
    /// let fields: Vec<_> = data
    ///     .read_protobuf_fields_with(len, |_n, mut body: LimitedRead<'_, _>| {
    ///         let n = body.remaining();
    ///         let mut buf = vec![0u8; n];
    ///         body.read_exact(&mut buf)?;
    ///         Ok((buf.into_boxed_slice(), n))
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
        F: FnMut(FieldNumber, LimitedRead<'_, Self>) -> Result<(L, usize)>;
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
        F: FnMut(FieldNumber, LimitedRead<'_, Self>) -> Result<(L, usize)>,
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
    use super::{LimitedRead, LimitedReadExtProtobuf, ReadExtProtobuf};
    use crate::varint::Varint;
    use crate::{Field, FieldNumber, FieldValue, ProtobufError};
    use ::std::io::{Cursor, Read};

    const LARGE_LEN: usize = 8200;

    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    fn read_bytes_payload<R: Read>(
        _n: FieldNumber,
        mut body: LimitedRead<'_, R>,
    ) -> crate::Result<(Bytes, usize)> {
        let len = body.remaining();
        let mut data = vec![0; len];
        body.read_exact(&mut data)?;
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
            .read_protobuf_fields_with(message.len(), |_n, mut body: LimitedRead<'_, _>| {
                let n = body.remaining();
                let mut buf = vec![0u8; n];
                body.read_exact(&mut buf)?;
                Ok((buf.into_boxed_slice(), n))
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
            .read_protobuf_fields_with(len, |_n, mut body: LimitedRead<'_, _>| {
                let n = body.remaining();
                let mut data = vec![0; n];
                body.read_exact(&mut data)?;
                Ok((Tagged { id, data }, n))
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
            .read_protobuf_fields_with(mixed.len(), |_n, body: LimitedRead<'_, _>| {
                calls += 1;
                read_bytes_payload(_n, body)
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
                |_n, _body: LimitedRead<'_, _>| -> crate::Result<(Vec<u8>, usize)> {
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
            .read_protobuf_fields_with(message.len(), |_n, mut body: LimitedRead<'_, _>| {
                let len = body.remaining();
                let mut data = vec![0xAB; len + 4];
                body.read_exact(&mut data[..len])?;
                Ok((Spare { data }, len))
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
            .read_protobuf_fields_with(message.len(), |_n, mut body: LimitedRead<'_, _>| {
                let mut one = [0u8; 1];
                body.read_exact(&mut one)?;
                Ok((one[0], 1))
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
            mut body: LimitedRead<'_, R>,
        ) -> crate::Result<(LenValue, usize)> {
            let budget = body.remaining();
            match field_number.as_u32() {
                2 => {
                    let mut child = Child::default();
                    let mut raw = |inner_n: FieldNumber, mut inner: LimitedRead<'_, R>| {
                        let n = inner.remaining();
                        let mut bytes = vec![0u8; n];
                        inner.read_exact(&mut bytes)?;
                        let _ = inner_n;
                        Ok((LenValue::Bytes(bytes), n))
                    };
                    for field in body.collect_protobuf_fields(&mut raw)? {
                        match (field.field_number.as_u32(), field.value) {
                            (1, FieldValue::Varint(v)) => child.id = v.to_uint64(),
                            (3, FieldValue::Len(LenValue::Bytes(name))) => child.name = name,
                            _ => {}
                        }
                    }
                    Ok((LenValue::Child(child), budget - body.remaining()))
                }
                _ => {
                    let mut bytes = vec![0u8; budget];
                    body.read_exact(&mut bytes)?;
                    Ok((LenValue::Bytes(bytes), budget))
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
