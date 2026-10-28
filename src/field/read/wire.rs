//! Byte-level wire cursor shared by streaming and slice field readers.

use crate::field::{Field, FieldValue};
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::{MAX_VARINT_BYTES, WireType};
use crate::{ProtobufError, Result};
use ::std::io::Write;

/// Byte-level cursor over protobuf wire data.
///
/// Like [`Iterator`]: a small set of required fragments, plus default methods
/// that compose them (`skip_field`, `skip_until`, `copy_field`, `copy_until`,
/// …). Implementors only override the I/O primitives.
///
/// Soft end-of-input is signaled by [`remaining`](Self::remaining) `== 0`
/// (field iterators stop there). Mid-field emptiness is
/// [`ProtobufError::UnexpectedEof`].
pub(crate) trait WireCursor {
    /// Bytes still allowed in this cursor's bound (message budget / slice length).
    fn remaining(&self) -> usize;

    /// Read a varint and return its exact wire bytes.
    ///
    /// The returned slice borrows for the joint lifetime of `self` and `buf`.
    /// Length is in `1..=MAX_VARINT_BYTES`. [`SliceReader`] returns a subslice of
    /// the input (no copy); [`Read`] paths fill `buf` and return `&buf[..n]`.
    /// Empty input is [`ProtobufError::UnexpectedEof`] (caller must check
    /// [`remaining`](Self::remaining) first for soft end).
    fn read_varint_with_raw<'a>(
        &'a mut self,
        buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'a [u8])>;

    /// Read a varint (discards the raw wire bytes).
    ///
    /// Empty input is [`ProtobufError::UnexpectedEof`] (caller must check
    /// [`remaining`](Self::remaining) first for soft end).
    #[inline(always)]
    fn read_varint(&mut self) -> Result<Varint> {
        Ok(self.read_varint_with_raw(&mut [0u8; MAX_VARINT_BYTES])?.0)
    }

    /// Read exactly `N` bytes (fixed-width wire types).
    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]>;

    /// Discard exactly `length` bytes without allocating a payload buffer.
    fn skip_bytes(&mut self, length: usize) -> Result<()>;

    /// Copy exactly `length` bytes to `out` (dual of [`skip_bytes`](Self::skip_bytes)).
    ///
    /// Returns `length` on success.
    fn copy_bytes<W: Write>(&mut self, length: usize, out: &mut W) -> Result<usize>;

    /// Read a tag (field number + wire type).
    ///
    /// Empty input is [`ProtobufError::UnexpectedEof`]. Invalid field numbers
    /// and wire types surface as [`ProtobufError`].
    #[inline(always)]
    fn read_tag(&mut self) -> Result<Tag> {
        Tag::from_encoded(self.read_varint()?)
    }

    /// Skip one complete field and return its tag.
    ///
    /// Dual of [`BufWireCursor::parse_field`]: the payload is discarded instead
    /// of materialized. For [`WireType::SGroup`] / [`WireType::EGroup`], only
    /// the tag is consumed; a start-group's following body fields are left for
    /// the caller.
    ///
    /// Empty input is [`ProtobufError::UnexpectedEof`].
    #[inline]
    fn skip_field(&mut self) -> Result<Tag> {
        let tag = self.read_tag()?;
        match tag.wire_type {
            WireType::Varint => {
                self.read_varint()?;
            }
            WireType::Int32 => self.skip_bytes(4)?,
            WireType::Int64 => self.skip_bytes(8)?,
            WireType::Len => {
                let length = self.read_varint()?.try_to_uint32()? as usize;
                self.skip_bytes(length)?;
            }
            WireType::SGroup | WireType::EGroup => {}
        }
        Ok(tag)
    }

    /// Skip fields until `pred` returns `Ok(true)`.
    ///
    /// Primary caller is group handling via [`until_group_ends`]. Each
    /// iteration runs [`skip_field`](Self::skip_field) (Len payloads discarded
    /// without allocating) and passes the field's [`Tag`] to `pred`. EOF
    /// before `pred` succeeds is [`ProtobufError::UnexpectedEof`].
    #[inline]
    fn skip_until<F>(&mut self, mut pred: F) -> Result<()>
    where
        F: FnMut(Tag) -> Result<bool>,
    {
        loop {
            if self.remaining() == 0 {
                return Err(ProtobufError::UnexpectedEof);
            }
            let tag = self.skip_field()?;
            if pred(tag)? {
                return Ok(());
            }
        }
    }

    /// Copy one complete field's wire bytes to `out`.
    ///
    /// Returns `(tag, bytes_written)`. Tag / length / value varints are teed
    /// exactly (including non-canonical encodings). Fixed-width and Len
    /// payloads use [`copy_bytes`](Self::copy_bytes). Groups copy the tag
    /// only. On error, `out` may hold a partial field.
    #[inline]
    fn copy_field<W: Write>(&mut self, out: &mut W) -> Result<(Tag, usize)> {
        let mut written = 0usize;
        let mut buf = [0u8; MAX_VARINT_BYTES];
        let (tag_varint, tag_raw) = self.read_varint_with_raw(&mut buf)?;
        written += write_all_len(out, tag_raw)?;
        let tag = Tag::from_encoded(tag_varint)?;
        match tag.wire_type {
            WireType::Varint => {
                let (_, value_raw) = self.read_varint_with_raw(&mut buf)?;
                written += write_all_len(out, value_raw)?;
            }
            WireType::Int32 => written += self.copy_bytes(4, out)?,
            WireType::Int64 => written += self.copy_bytes(8, out)?,
            WireType::Len => {
                let (len_varint, len_raw) = self.read_varint_with_raw(&mut buf)?;
                written += write_all_len(out, len_raw)?;
                written += self.copy_bytes(len_varint.try_to_uint32()? as usize, out)?;
            }
            WireType::SGroup | WireType::EGroup => {}
        }
        Ok((tag, written))
    }

    /// Copy fields to `out` until `pred` returns `Ok(true)`.
    ///
    /// Primary caller is group preserve via [`until_group_ends`]. Same stop
    /// semantics as [`skip_until`](Self::skip_until). Returns bytes written.
    /// On error, `out` may already contain a partial copy.
    #[inline]
    fn copy_until<F, W>(&mut self, mut pred: F, out: &mut W) -> Result<usize>
    where
        F: FnMut(Tag) -> Result<bool>,
        W: Write,
    {
        let mut written = 0usize;
        loop {
            if self.remaining() == 0 {
                return Err(ProtobufError::UnexpectedEof);
            }
            let (tag, n) = self.copy_field(out)?;
            written += n;
            if pred(tag)? {
                return Ok(written);
            }
        }
    }
}

/// `write_all` then return how many bytes were written (`buf.len()` on success).
#[inline]
fn write_all_len<W: Write>(out: &mut W, buf: &[u8]) -> Result<usize> {
    out.write_all(buf)?;
    Ok(buf.len())
}

/// [`WireCursor`] that can materialize a byte buffer into `L`.
///
/// Streaming sources build `L` with the caller's closure; the slice source
/// borrows `&[u8]`.
pub(crate) trait BufWireCursor<L>: WireCursor {
    /// Read exactly `length` bytes into an `L` buffer.
    ///
    /// This is the materializing counterpart of [`WireCursor::skip_bytes`].
    /// For a Len field, the length varint is parsed first; this method only
    /// consumes the following payload bytes.
    fn read_bytes(&mut self, length: usize) -> Result<L>;

    /// Parse a complete field (tag + value).
    ///
    /// `L` is the length-delimited payload. Streaming readers build it with the
    /// caller's closure. The slice reader uses `&'a [u8]`.
    ///
    /// Empty input is [`ProtobufError::UnexpectedEof`] (field iterators check
    /// [`remaining`](WireCursor::remaining) first for soft end).
    ///
    /// For [`FieldValue::SGroup`] / [`FieldValue::EGroup`], only the tag is
    /// consumed; a start-group body remains on the cursor for the caller to
    /// read or skip with [`until_group_ends`].
    fn parse_field(&mut self) -> Result<Field<L>> {
        let tag = self.read_tag()?;

        let value = match tag.wire_type {
            WireType::Varint => FieldValue::Varint(self.read_varint()?),
            WireType::Int32 => FieldValue::I32(self.read_fixed::<4>()?),
            WireType::Int64 => FieldValue::I64(self.read_fixed::<8>()?),
            WireType::Len => {
                let length = self.read_varint()?.try_to_uint32()? as usize;
                FieldValue::Len(self.read_bytes(length)?)
            }
            WireType::SGroup => FieldValue::SGroup,
            WireType::EGroup => FieldValue::EGroup,
        };

        Ok(Field {
            field_number: tag.field_number,
            value,
        })
    }
}
