//! Streaming (`Read`) field iteration with a local [`Limited`] budget.

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
// Limited Read budget (no nested std::io::Take)
// ============================================================================

/// How to consume one length-delimited field body on a streaming [`Read`].
///
/// The library does not prescribe the payload type: `Output` is whatever this
/// handler returns (`Vec<u8>`, a folded message struct, nested `Field`s, …).
///
/// Closures `FnMut(FieldNumber, &mut Limited<'_, R>) -> Result<L>` implement
/// this trait.
pub trait OnLen<R: Read + ?Sized> {
    /// Type stored in [`FieldValue::Len`].
    type Output;

    /// Consume the Len body in `body` (already bounded to the payload length).
    ///
    /// Unread bytes left in `body` when this returns are skipped by the caller
    /// so the parent cursor stays aligned.
    fn on_len(
        &mut self,
        field_number: FieldNumber,
        body: &mut Limited<'_, R>,
    ) -> Result<Self::Output>;
}

impl<R, L, F> OnLen<R> for F
where
    R: Read + ?Sized,
    F: FnMut(FieldNumber, &mut Limited<'_, R>) -> Result<L>,
{
    type Output = L;

    #[inline]
    fn on_len(&mut self, field_number: FieldNumber, body: &mut Limited<'_, R>) -> Result<L> {
        self(field_number, body)
    }
}

/// Default [`OnLen`] for [`ReadExtProtobuf::read_protobuf_fields`]: copy the
/// body into a new `Vec<u8>`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VecLenPayload;

impl<R: Read + ?Sized> OnLen<R> for VecLenPayload {
    type Output = Vec<u8>;

    #[inline]
    fn on_len(&mut self, _field_number: FieldNumber, body: &mut Limited<'_, R>) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; body.remaining()];
        body.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// A length budget over an underlying [`Read`].
///
/// Nesting does **not** wrap another adapter around `R`. [`reborrow_len`](Self::reborrow_len)
/// commits `len` bytes from this budget and returns a child over the same `&mut R`.
/// Boundary checks stay O(1) per read regardless of nesting depth.
pub struct Limited<'a, R: ?Sized> {
    inner: &'a mut R,
    remaining: usize,
}

impl<'a, R: Read + ?Sized> Limited<'a, R> {
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
    /// Nested message parsing uses the child; unread child bytes should be
    /// skipped (as [`parse_field`](Self::parse_field) does after [`OnLen`]).
    #[inline]
    pub fn reborrow_len(&mut self, len: usize) -> Result<Limited<'_, R>> {
        if len > self.remaining {
            return Err(ProtobufError::UnexpectedEof);
        }
        self.remaining -= len;
        Ok(Limited {
            inner: self.inner,
            remaining: len,
        })
    }

    /// Read exactly `buf.len()` bytes from this budget into `buf`.
    #[inline]
    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        if buf.len() > self.remaining {
            return Err(ProtobufError::UnexpectedEof);
        }
        self.inner.read_exact(buf)?;
        self.remaining -= buf.len();
        Ok(())
    }

    /// Parse one field. Soft end is the caller's [`remaining`](Self::remaining)
    /// `== 0` check; mid-field emptiness is [`ProtobufError::UnexpectedEof`].
    pub fn parse_field<F>(&mut self, on_len: &mut F) -> Result<Field<F::Output>>
    where
        F: OnLen<R>,
    {
        let tag = WireCursor::read_tag(self)?;
        let value = match tag.wire_type {
            WireType::Varint => FieldValue::Varint(WireCursor::read_varint(self)?),
            WireType::Int32 => FieldValue::I32(WireCursor::read_fixed(self)?),
            WireType::Int64 => FieldValue::I64(WireCursor::read_fixed(self)?),
            WireType::Len => {
                let length = WireCursor::read_varint(self)?.try_to_uint32()? as usize;
                let mut body = self.reborrow_len(length)?;
                let payload = on_len.on_len(tag.field_number, &mut body)?;
                let unread = body.remaining();
                if unread > 0 {
                    WireCursor::skip_bytes(&mut body, unread)?;
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

    /// Collect every field in this budget.
    ///
    /// Useful inside an [`OnLen`] handler for nested messages. Prefer
    /// [`ReadExtProtobuf::read_protobuf_fields`] at the top level when an
    /// iterator is enough.
    pub fn read_protobuf_fields<F>(&mut self, on_len: &mut F) -> Result<Vec<Field<F::Output>>>
    where
        F: OnLen<R>,
    {
        let mut fields = Vec::new();
        while self.remaining() > 0 {
            fields.push(self.parse_field(on_len)?);
        }
        Ok(fields)
    }
}

impl<R: Read + ?Sized> Read for Limited<'_, R> {
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

impl<R: Read + ?Sized> WireCursor for Limited<'_, R> {
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
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    #[inline]
    fn skip_bytes(&mut self, mut length: usize) -> Result<()> {
        let mut buf = [0u8; 256];
        while length > 0 {
            let n = length.min(buf.len());
            self.read_exact(&mut buf[..n])?;
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
            self.read_exact(&mut buf[..n])?;
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
/// Uses a local budget ([`Limited`]) over `R` — not nested [`std::io::Take`].
/// `F` handles each length-delimited body ([`OnLen`]); the default is
/// [`VecLenPayload`].
pub struct FieldIterFromRead<R, F = VecLenPayload> {
    inner: R,
    remaining: usize,
    on_len: F,
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
        let mut limited = Limited::new(&mut self.inner, self.remaining);
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
        let mut limited = Limited::new(&mut self.inner, self.remaining);
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

impl<R, F> Iterator for FieldIterFromRead<R, F>
where
    R: Read,
    F: OnLen<R>,
{
    type Item = Result<Field<F::Output>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let mut limited = Limited::new(&mut self.inner, self.remaining);
        let result = limited.parse_field(&mut self.on_len);
        self.remaining = limited.remaining;
        Some(result)
    }
}

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// Each method takes an explicit byte budget `len` and tracks it with a local
/// [`Limited`] over the reader (not [`std::io::Take`]). Length-delimited
/// payloads default to `Vec<u8>` via [`VecLenPayload`].
/// [`read_protobuf_fields_with`](ReadExtProtobuf::read_protobuf_fields_with)
/// supplies a custom [`OnLen`] handler (nested parse, custom buffer, …).
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
    /// Consumes at most `len` bytes (local budget; see [`Limited`]).
    /// This consumes the reader and returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Length-delimited payloads are `Vec<u8>`.
    fn read_protobuf_fields(self, len: usize) -> FieldIterFromRead<Self>
    where
        Self: Sized,
    {
        self.read_protobuf_fields_with(len, VecLenPayload)
    }

    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Consumes at most `len` bytes (local budget; see [`Limited`]).
    ///
    /// `on_len` is called once for each length-delimited field with the field
    /// number and a [`Limited`] body already bounded to the payload length.
    /// Return [`Ok`](Result::Ok) with any payload type `L` ([`OnLen::Output`]).
    /// Unread body bytes are skipped after the handler returns. Return
    /// [`Err`](Result::Err) to fail the field. Varint and fixed-width fields
    /// do not call `on_len`.
    ///
    /// # Examples
    ///
    /// Copy each Len body into a `Box<[u8]>`:
    ///
    /// ```
    /// use ::protobuf_core::{FieldValue, Limited, ReadExtProtobuf};
    ///
    /// let data = &[0x0A, 0x03, b'H', b'e', b'l'][..];
    /// let len = data.len();
    /// let fields: Vec<_> = data
    ///     .read_protobuf_fields_with(len, |_n, body: &mut Limited<'_, _>| {
    ///         let mut buf = vec![0u8; body.remaining()];
    ///         body.read_exact(&mut buf)?;
    ///         Ok(buf.into_boxed_slice())
    ///     })
    ///     .collect::<Result<Vec<_>, _>>()
    ///     .unwrap();
    /// let FieldValue::Len(data) = &fields[0].value else {
    ///     panic!("expected a length-delimited field");
    /// };
    /// assert_eq!(&data[..], b"Hel");
    /// ```
    fn read_protobuf_fields_with<F>(self, len: usize, on_len: F) -> FieldIterFromRead<Self, F>
    where
        Self: Sized,
        F: OnLen<Self>;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_fields_with<F>(self, len: usize, on_len: F) -> FieldIterFromRead<Self, F>
    where
        Self: Sized,
        F: OnLen<Self>,
    {
        FieldIterFromRead {
            inner: self,
            remaining: len,
            on_len,
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

        // Mid-field truncation with budget == input.len(): the local remaining
        // count catches the shortfall before the underlying Read EOFs.
        for input in [
            &[0x0D, 0x01, 0x02, 0x03][..],    // truncated i32
            &[0x09, 0, 0, 0, 0, 0, 0, 0][..], // truncated i64
            &[0x0A, 0x05, b'a', b'b'][..],    // truncated len
        ] {
            let error = Cursor::new(input)
                .read_protobuf_fields(input.len())
                .next()
                .unwrap()
                .unwrap_err();
            assert!(
                matches!(error, ProtobufError::UnexpectedEof),
                "input={input:?} got {error:?}"
            );
        }
    }
}

#[cfg(test)]
mod len_payload {
    use super::{Limited, ReadExtProtobuf};
    use crate::varint::Varint;
    use crate::{Field, FieldNumber, FieldValue, ProtobufError};
    use ::std::io::{Cursor, Read};

    const LARGE_LEN: usize = 8200;

    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    fn read_bytes_payload<R: Read>(
        _n: FieldNumber,
        body: &mut Limited<'_, R>,
    ) -> crate::Result<Bytes> {
        let mut data = vec![0; body.remaining()];
        body.read_exact(&mut data)?;
        Ok(Bytes(data))
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
            .read_protobuf_fields_with(message.len(), |_n, body: &mut Limited<'_, _>| {
                let mut buf = vec![0u8; body.remaining()];
                body.read_exact(&mut buf)?;
                Ok(buf.into_boxed_slice())
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
            .read_protobuf_fields_with(len, |_n, body: &mut Limited<'_, _>| {
                let mut data = vec![0; body.remaining()];
                body.read_exact(&mut data)?;
                Ok(Tagged { id, data })
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
            .read_protobuf_fields_with(mixed.len(), |_n, body: &mut Limited<'_, _>| {
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
                |_n, _body: &mut Limited<'_, _>| -> crate::Result<Vec<u8>> {
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
            .read_protobuf_fields_with(message.len(), |_n, body: &mut Limited<'_, _>| {
                let len = body.remaining();
                let mut data = vec![0xAB; len + 4];
                body.read_exact(&mut data[..len])?;
                Ok(Spare { data })
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
            .read_protobuf_fields_with(message.len(), |_n, body: &mut Limited<'_, _>| {
                let mut one = [0u8; 1];
                body.read_exact(&mut one)?;
                Ok(one[0])
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
            body: &mut Limited<'_, R>,
        ) -> crate::Result<LenValue> {
            match field_number.as_u32() {
                2 => {
                    let mut child = Child::default();
                    let mut raw = |inner_n: FieldNumber, inner: &mut Limited<'_, R>| {
                        let mut bytes = vec![0u8; inner.remaining()];
                        inner.read_exact(&mut bytes)?;
                        let _ = inner_n;
                        Ok(LenValue::Bytes(bytes))
                    };
                    for field in body.read_protobuf_fields(&mut raw)? {
                        match (field.field_number.as_u32(), field.value) {
                            (1, FieldValue::Varint(v)) => child.id = v.to_uint64(),
                            (3, FieldValue::Len(LenValue::Bytes(name))) => child.name = name,
                            _ => {}
                        }
                    }
                    Ok(LenValue::Child(child))
                }
                _ => {
                    let mut bytes = vec![0u8; body.remaining()];
                    body.read_exact(&mut bytes)?;
                    Ok(LenValue::Bytes(bytes))
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
