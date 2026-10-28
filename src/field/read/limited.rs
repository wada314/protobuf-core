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
