//! Field reading utilities for Protocol Buffers
//!
//! This module provides low-level utilities for reading raw protobuf fields from
//! `std::io::Read` (with an explicit byte budget) and from byte slices (`AsRef<[u8]>`).
//! Streaming sources build each length-delimited payload with a closure
//! `FnMut(usize) -> Result<L>`. Slice sources borrow them as `&[u8]`.
//!
//! Byte iterators are not supported; wrap bytes as a slice ([`AsRefExtProtobuf`])
//! or as a [`Read`] with a `len` budget ([`ReadExtProtobuf`]).

use crate::field::{Field, FieldValue};
use crate::field_number::FieldNumber;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::{MAX_VARINT_BYTES, WireType};
use crate::{ProtobufError, Result};
use ::std::convert::{AsMut, AsRef};
use ::std::io::{Read, Take, Write};

/// `Vec<u8>` payload of `len` initialized bytes.
///
/// [`ReadExtProtobuf::read_protobuf_fields`] passes this function. Its type is
/// `fn(usize) -> Result<Vec<u8>>`.
fn vec_payload(len: usize) -> Result<Vec<u8>> {
    Ok(vec![0u8; len])
}

/// Call `make` for one length-delimited payload.
///
/// `make` returns at least `length` initialized bytes. The reader overwrites the
/// first `length` bytes and leaves any suffix as the closure initialized it.
/// A shorter buffer panics.
fn make_payload<L, F>(make: &mut F, length: usize) -> Result<L>
where
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
{
    let mut payload = make(length)?;
    let got = payload.as_mut().len();
    assert!(
        got >= length,
        "length-delimited buffer length {got} is shorter than payload length {length}",
    );
    Ok(payload)
}

// ============================================================================
// Common parsing infrastructure
// ============================================================================

/// Predicate for [`skip_until`](WireCursor::skip_until) /
/// [`copy_until`](WireCursor::copy_until) / [`take_until`](FieldIterFromSlice::take_until):
/// stop after the matching end-group, including nested groups.
///
/// Call after the start-group tag for `field_number` has already been consumed.
/// Open groups are tracked with an explicit stack (LIFO field numbers) captured
/// inside the returned closure.
///
/// Discard the body:
///
/// ```
/// use ::protobuf_core::{AsRefExtProtobuf, FieldValue, until_group_ends};
///
/// let bytes = [0x0B, 0x10, 0x01, 0x0C, 0x18, 0x07];
/// let mut fields = bytes.as_slice().read_protobuf_fields();
/// let start = fields.next().unwrap().unwrap();
/// assert!(matches!(start.value, FieldValue::SGroup));
/// fields.skip_until(until_group_ends(start.field_number)).unwrap();
/// ```
///
/// Preserve the body (and matching end tag) for an unknown-field store — the
/// start tag is already consumed, so write it yourself first if needed:
///
/// ```
/// use ::protobuf_core::{AsRefExtProtobuf, FieldValue, WriteExtProtobuf, until_group_ends};
///
/// let bytes = [0x0B, 0x10, 0x01, 0x0C, 0x18, 0x07];
/// let mut fields = bytes.as_slice().read_protobuf_fields();
/// let start = fields.next().unwrap().unwrap();
/// assert!(matches!(start.value, FieldValue::SGroup));
///
/// let mut preserved = Vec::new();
/// preserved.write_protobuf_field(&start).unwrap();
/// let body = fields.take_until(until_group_ends(start.field_number)).unwrap();
/// preserved.extend_from_slice(body);
/// assert_eq!(preserved, &bytes[..bytes.len() - 2]); // without trailing field 3
/// ```
pub fn until_group_ends(field_number: FieldNumber) -> impl FnMut(Tag) -> Result<bool> {
    let mut stack = vec![field_number];
    move |tag| match tag.wire_type {
        WireType::SGroup => {
            stack.push(tag.field_number);
            Ok(false)
        }
        WireType::EGroup => {
            let Some(&expected) = stack.last() else {
                // Pred misuse: called after the tracked group already closed.
                debug_assert!(
                    false,
                    "until_group_ends predicate called after the tracked group closed"
                );
                return Err(ProtobufError::UnexpectedEof);
            };
            if tag.field_number != expected {
                return Err(ProtobufError::MismatchedEndGroup {
                    expected,
                    got: tag.field_number,
                });
            }
            stack.pop();
            Ok(stack.is_empty())
        }
        _ => Ok(false),
    }
}

/// Byte-level cursor over protobuf wire data.
///
/// Required methods are atomic fragments (`remaining` / `read_varint_with_raw` /
/// `read_fixed` / `skip_bytes` / `copy_bytes`). Default methods compose those
/// into tags, one-field skip/copy, and conditional multi-field skip/copy.
///
/// Soft end-of-input is signaled by [`remaining`](Self::remaining) `== 0`
/// (field iterators stop there). Mid-field emptiness is
/// [`ProtobufError::UnexpectedEof`].
pub(crate) trait WireCursor {
    /// Bytes still allowed in this cursor's bound (`Take::limit` / slice length).
    fn remaining(&self) -> usize;

    /// Read a varint and return its exact wire bytes.
    ///
    /// The returned slice borrows for the joint lifetime of `self` and `buf`:
    /// it may point into the cursor's input (slice path, no copy) or into
    /// `buf` (Read path fills `buf` while decoding). Length is in
    /// `1..=MAX_VARINT_BYTES`. Empty input is [`ProtobufError::UnexpectedEof`]
    /// (caller must check [`remaining`](Self::remaining) first for soft end).
    fn read_varint_with_raw<'a>(
        &'a mut self,
        buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'a [u8])>;

    /// Read a varint (discards the raw wire bytes).
    ///
    /// Inlined so decode-only call sites do not keep the raw borrow / buffer
    /// work alive longer than needed.
    ///
    /// Empty input is [`ProtobufError::UnexpectedEof`] (caller must check
    /// [`remaining`](Self::remaining) first for soft end).
    #[inline(always)]
    fn read_varint(&mut self) -> Result<Varint> {
        let mut raw = [0u8; MAX_VARINT_BYTES];
        Ok(self.read_varint_with_raw(&mut raw)?.0)
    }

    /// Read exactly `N` bytes (fixed-width wire types).
    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]>;

    /// Discard exactly `length` bytes without allocating a payload buffer.
    fn skip_bytes(&mut self, length: usize) -> Result<()>;

    /// Copy exactly `length` bytes to `out` (dual of [`skip_bytes`](Self::skip_bytes)).
    fn copy_bytes<W: Write>(&mut self, length: usize, out: &mut W) -> Result<()>;

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
    /// the tag is consumed; the start-group body is left for the caller.
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

    /// Copy one complete field's wire bytes to `out` and return its tag.
    ///
    /// Dual of [`skip_field`](Self::skip_field). Tag, length-prefix, and value
    /// varints are teed from the input (exact wire bytes, including
    /// non-canonical encodings); fixed-width and Len payloads are copied
    /// verbatim. For groups, only the tag is copied (same tag-only contract as
    /// [`skip_field`](Self::skip_field)).
    ///
    /// On error after some bytes were written, `out` may hold a partial field.
    /// Empty input is [`ProtobufError::UnexpectedEof`].
    #[inline]
    fn copy_field<W: Write>(&mut self, out: &mut W) -> Result<Tag> {
        let mut raw = [0u8; MAX_VARINT_BYTES];
        let (encoded, bytes) = self.read_varint_with_raw(&mut raw)?;
        out.write_all(bytes)?;
        let tag = Tag::from_encoded(encoded)?;
        match tag.wire_type {
            WireType::Varint => {
                let (_, bytes) = self.read_varint_with_raw(&mut raw)?;
                out.write_all(bytes)?;
            }
            WireType::Int32 => self.copy_bytes(4, out)?,
            WireType::Int64 => self.copy_bytes(8, out)?,
            WireType::Len => {
                let (length, bytes) = self.read_varint_with_raw(&mut raw)?;
                out.write_all(bytes)?;
                self.copy_bytes(length.try_to_uint32()? as usize, out)?;
            }
            WireType::SGroup | WireType::EGroup => {}
        }
        Ok(tag)
    }

    /// Skip fields until `pred` returns `Ok(true)`.
    ///
    /// Each iteration runs [`skip_field`](Self::skip_field) (so length-delimited
    /// payloads are discarded without allocating) and passes the field's [`Tag`]
    /// to `pred`. EOF before `pred` succeeds is [`ProtobufError::UnexpectedEof`].
    ///
    /// For a group body after its start tag was consumed, use
    /// [`until_group_ends`] (see [`FieldIterFromSlice::skip_until`] for a
    /// runnable public example).
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

    /// Copy fields to `out` until `pred` returns `Ok(true)`.
    ///
    /// Same stop / nesting semantics as [`skip_until`](Self::skip_until), but
    /// each field is written via [`copy_field`](Self::copy_field). Returns the
    /// number of bytes written. EOF before `pred` succeeds is
    /// [`ProtobufError::UnexpectedEof`].
    ///
    /// On error, `out` may already contain a partial copy (including the field
    /// that failed the predicate, when the failure is from `pred` after a
    /// successful [`copy_field`](Self::copy_field)).
    ///
    /// For a group body after its start tag was consumed, use
    /// [`until_group_ends`]. Prefer [`FieldIterFromSlice::take_until`] when the
    /// input is a slice (exact wire bytes, zero-copy).
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
            // Track bytes via a counting wrapper so we do not require `Seek`.
            let mut counter = WriteCounter {
                inner: out,
                written: 0,
            };
            let tag = self.copy_field(&mut counter)?;
            written += counter.written;
            if pred(tag)? {
                return Ok(written);
            }
        }
    }
}

/// Counts bytes written while forwarding to an inner [`Write`].
struct WriteCounter<'a, W> {
    inner: &'a mut W,
    written: usize,
}

impl<W: Write> Write for WriteCounter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> ::std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written += n;
        Ok(n)
    }

    fn flush(&mut self) -> ::std::io::Result<()> {
        self.inner.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> ::std::io::Result<()> {
        self.inner.write_all(buf)?;
        self.written += buf.len();
        Ok(())
    }
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
    /// read or [`skip_until`](WireCursor::skip_until) with [`until_group_ends`].
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

/// [`WireCursor`] over a length-bounded [`Take`].
///
/// Nested varint reads use the take's EOF; [`remaining`](WireCursor::remaining)
/// is [`Take::limit`].
struct ReadCursor<'a, R>(&'a mut Take<R>);

impl<R> WireCursor for ReadCursor<'_, R>
where
    R: Read,
{
    #[inline]
    fn remaining(&self) -> usize {
        usize::try_from(self.0.limit()).unwrap_or(usize::MAX)
    }

    #[inline]
    fn read_varint_with_raw<'a>(
        &'a mut self,
        buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'a [u8])> {
        read_varint_with_raw_from_take(self.0, buf)
    }

    #[inline]
    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        self.0.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    #[inline]
    fn skip_bytes(&mut self, mut length: usize) -> Result<()> {
        let mut buf = [0u8; 256];
        while length > 0 {
            let n = length.min(buf.len());
            self.0.read_exact(&mut buf[..n])?;
            length -= n;
        }
        Ok(())
    }

    #[inline]
    fn copy_bytes<W: Write>(&mut self, mut length: usize, out: &mut W) -> Result<()> {
        let mut buf = [0u8; 256];
        while length > 0 {
            let n = length.min(buf.len());
            self.0.read_exact(&mut buf[..n])?;
            out.write_all(&buf[..n])?;
            length -= n;
        }
        Ok(())
    }
}

/// [`Read::read_exact`] for varint bytes: EOF → [`ProtobufError::UnexpectedEof`]
/// (matches the former `read_varint` empty/`None` path). Fixed / Len truncation
/// still uses plain `read_exact` → [`ProtobufError::IoError`].
#[inline]
fn read_exact_eof(reader: &mut impl Read, buf: &mut [u8]) -> Result<()> {
    match reader.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ::std::io::ErrorKind::UnexpectedEof => {
            Err(ProtobufError::UnexpectedEof)
        }
        Err(e) => Err(ProtobufError::IoError(e)),
    }
}

/// Fill `buf` from a [`Take`] while decoding one varint; return a subslice of `buf`.
#[inline]
fn read_varint_with_raw_from_take<'a, R: Read>(
    take: &'a mut Take<R>,
    buf: &'a mut [u8; MAX_VARINT_BYTES],
) -> Result<(Varint, &'a [u8])> {
    use crate::varint::{DecodeOutcome, DecodeState};
    let mut state = DecodeState::new();
    let mut n = 0usize;
    loop {
        read_exact_eof(take, &mut buf[n..=n])?;
        n += 1;
        match state.feed(::std::iter::once(Ok::<u8, ProtobufError>(buf[n - 1])))? {
            DecodeOutcome::Complete(v) => return Ok((v, &buf[..n])),
            DecodeOutcome::Incomplete(s) => state = s,
            DecodeOutcome::Empty => return Err(ProtobufError::UnexpectedEof),
        }
    }
}

// ============================================================================
// ReadExtProtobuf
// ============================================================================

/// Helper struct implementing BufWireCursor for a length-bounded [`Take`].
struct ReadReader<'a, R, F>(&'a mut Take<R>, &'a mut F);

impl<R, F> WireCursor for ReadReader<'_, R, F>
where
    R: Read,
{
    #[inline]
    fn remaining(&self) -> usize {
        usize::try_from(self.0.limit()).unwrap_or(usize::MAX)
    }

    #[inline]
    fn read_varint_with_raw<'a>(
        &'a mut self,
        buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'a [u8])> {
        read_varint_with_raw_from_take(self.0, buf)
    }

    #[inline]
    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        ReadCursor(self.0).read_fixed()
    }

    #[inline]
    fn skip_bytes(&mut self, length: usize) -> Result<()> {
        ReadCursor(self.0).skip_bytes(length)
    }

    #[inline]
    fn copy_bytes<W: Write>(&mut self, length: usize, out: &mut W) -> Result<()> {
        ReadCursor(self.0).copy_bytes(length, out)
    }
}

impl<R, F, L> BufWireCursor<L> for ReadReader<'_, R, F>
where
    R: Read,
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
{
    fn read_bytes(&mut self, length: usize) -> Result<L> {
        let mut payload = make_payload(self.1, length)?;
        self.0.read_exact(&mut payload.as_mut()[..length])?;
        Ok(payload)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a reader.
///
/// Bounded by the `len` passed to [`ReadExtProtobuf::read_protobuf_fields`]
/// (internally a [`Take`]). `F` builds each length-delimited payload. It
/// defaults to `fn(usize) -> Result<Vec<u8>>`. `F` is called once per
/// length-delimited field.
pub struct FieldIterFromRead<R, F = fn(usize) -> Result<Vec<u8>>> {
    reader: Take<R>,
    make: F,
}

impl<R, F> FieldIterFromRead<R, F> {
    /// Skip fields until `pred` returns `Ok(true)`.
    ///
    /// Length-delimited payloads are discarded without allocating. The byte
    /// budget is the remaining capacity of the inner [`Take`]. For a group
    /// body after [`FieldValue::SGroup`], use [`until_group_ends`] (see
    /// [`FieldIterFromSlice::skip_until`] for a runnable example).
    pub fn skip_until<P>(&mut self, pred: P) -> Result<()>
    where
        R: Read,
        P: FnMut(Tag) -> Result<bool>,
    {
        ReadCursor(&mut self.reader).skip_until(pred)
    }

    /// Copy fields to `out` until `pred` returns `Ok(true)`.
    ///
    /// Same stop semantics as [`skip_until`](Self::skip_until). Writes exact
    /// wire bytes (including non-canonical varints). Returns the number of
    /// bytes written.
    ///
    /// After [`FieldValue::SGroup`], pair with [`until_group_ends`] to preserve
    /// the group body and matching end tag (start tag already consumed — write
    /// it yourself first if the preserve blob needs it). On error, `out` may
    /// already contain a partial copy.
    ///
    /// For zero-copy preserve from a slice, use
    /// [`FieldIterFromSlice::take_until`] instead.
    pub fn copy_until<P, W>(&mut self, pred: P, out: &mut W) -> Result<usize>
    where
        R: Read,
        P: FnMut(Tag) -> Result<bool>,
        W: Write,
    {
        ReadCursor(&mut self.reader).copy_until(pred, out)
    }
}

impl<R, F, L> Iterator for FieldIterFromRead<R, F>
where
    R: Read,
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = ReadReader(&mut self.reader, &mut self.make);
        if reader.remaining() == 0 {
            return None;
        }
        Some(reader.parse_field())
    }
}

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// Each method takes an explicit byte budget `len` and wraps the reader in
/// [`std::io::Take`]. Length-delimited payloads default to `Vec<u8>`.
/// [`read_protobuf_fields_with`](ReadExtProtobuf::read_protobuf_fields_with) supplies a different buffer.
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
pub trait ReadExtProtobuf {
    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Consumes at most `len` bytes via [`std::io::Take`].
    /// This consumes the reader and returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Length-delimited payloads are `Vec<u8>`.
    fn read_protobuf_fields(self, len: usize) -> FieldIterFromRead<Self>
    where
        Self: Sized,
    {
        self.read_protobuf_fields_with(len, vec_payload)
    }

    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Consumes at most `len` bytes via [`std::io::Take`].
    ///
    /// `make` is called once for each length-delimited field. `len` (the closure
    /// argument) is the payload length from the wire. Return [`Ok`](Result::Ok)
    /// with at least that many initialized bytes. The reader overwrites the first
    /// `len` bytes and leaves any suffix as `make` initialized it. A buffer
    /// shorter than `len` panics. Return [`Err`](Result::Err) to fail the field.
    /// Varint and fixed-width fields do not call `make`.
    ///
    /// If the payload bytes run out, the buffer is dropped and the reader returns
    /// [`UnexpectedEof`](ProtobufError::UnexpectedEof). Capture an allocator in
    /// `make` when the buffer needs one.
    ///
    /// # Examples
    ///
    /// ```
    /// use ::protobuf_core::{FieldValue, ReadExtProtobuf};
    ///
    /// let data = &[0x0A, 0x03, b'H', b'e', b'l'][..];
    /// let len = data.len();
    /// let fields: Vec<_> = data
    ///     .read_protobuf_fields_with(len, |payload_len| Ok(vec![0u8; payload_len].into_boxed_slice()))
    ///     .collect::<Result<Vec<_>, _>>()
    ///     .unwrap();
    /// let FieldValue::Len(data) = &fields[0].value else {
    ///     panic!("expected a length-delimited field");
    /// };
    /// assert_eq!(&data[..], b"Hel");
    /// ```
    fn read_protobuf_fields_with<L, F>(self, len: usize, make: F) -> FieldIterFromRead<Self, F>
    where
        Self: Sized,
        F: FnMut(usize) -> Result<L>,
        L: AsMut<[u8]>;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_fields_with<L, F>(self, len: usize, make: F) -> FieldIterFromRead<Self, F>
    where
        Self: Sized,
        F: FnMut(usize) -> Result<L>,
        L: AsMut<[u8]>,
    {
        FieldIterFromRead {
            reader: self.take(len as u64),
            make,
        }
    }
}

// ============================================================================
// AsRefExtProtobuf
// ============================================================================

/// Helper struct implementing BufWireCursor for slice-based readers
pub(crate) struct SliceReader<'a, 'b> {
    pub(crate) slice: &'b mut &'a [u8],
}

impl WireCursor for SliceReader<'_, '_> {
    #[inline]
    fn remaining(&self) -> usize {
        self.slice.len()
    }

    /// Returns a borrow into the input buffer (ignores `buf`; no copy).
    /// The joint lifetime with `buf` still applies so this matches the trait.
    #[inline]
    fn read_varint_with_raw<'a>(
        &'a mut self,
        _buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'a [u8])> {
        use crate::varint::ReadExtVarint;
        let before = *self.slice;
        let value = self
            .slice
            .read_varint()?
            .ok_or(ProtobufError::UnexpectedEof)?;
        let n = before.len() - self.slice.len();
        debug_assert!(n <= MAX_VARINT_BYTES);
        Ok((value, &before[..n]))
    }

    #[inline]
    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let (bytes, remaining) = self
            .slice
            .split_first_chunk::<N>()
            .ok_or(ProtobufError::UnexpectedEof)?;
        *self.slice = remaining;
        Ok(*bytes)
    }

    #[inline]
    fn skip_bytes(&mut self, length: usize) -> Result<()> {
        let (_, remaining) = self
            .slice
            .split_at_checked(length)
            .ok_or(ProtobufError::UnexpectedEof)?;
        *self.slice = remaining;
        Ok(())
    }

    #[inline]
    fn copy_bytes<W: Write>(&mut self, length: usize, out: &mut W) -> Result<()> {
        let (payload, remaining) = self
            .slice
            .split_at_checked(length)
            .ok_or(ProtobufError::UnexpectedEof)?;
        out.write_all(payload)?;
        *self.slice = remaining;
        Ok(())
    }
}

impl<'a> BufWireCursor<&'a [u8]> for SliceReader<'a, '_> {
    fn read_bytes(&mut self, length: usize) -> Result<&'a [u8]> {
        let (value_slice, remaining) = self
            .slice
            .split_at_checked(length)
            .ok_or(ProtobufError::UnexpectedEof)?;
        *self.slice = remaining;
        Ok(value_slice)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a byte slice.
pub struct FieldIterFromSlice<'a> {
    slice: &'a [u8],
}

impl<'a> FieldIterFromSlice<'a> {
    /// Skip fields until `pred` returns `Ok(true)`.
    ///
    /// Length-delimited payloads are discarded without allocating. After an
    /// [`FieldValue::SGroup`], pair this with [`until_group_ends`]:
    ///
    /// ```
    /// use ::protobuf_core::{AsRefExtProtobuf, FieldValue, until_group_ends};
    ///
    /// // field 1 SGroup, field 2 varint 1, field 1 EGroup, field 3 varint 7
    /// let bytes = [0x0B, 0x10, 0x01, 0x0C, 0x18, 0x07];
    /// let mut fields = bytes.as_slice().read_protobuf_fields();
    ///
    /// let start = fields.next().unwrap().unwrap();
    /// assert!(matches!(start.value, FieldValue::SGroup));
    ///
    /// fields.skip_until(until_group_ends(start.field_number)).unwrap();
    ///
    /// let after = fields.next().unwrap().unwrap();
    /// assert_eq!(after.field_number.as_u32(), 3);
    /// ```
    pub fn skip_until<P>(&mut self, pred: P) -> Result<()>
    where
        P: FnMut(Tag) -> Result<bool>,
    {
        SliceReader {
            slice: &mut self.slice,
        }
        .skip_until(pred)
    }

    /// Advance past fields until `pred` returns `Ok(true)`, returning the exact
    /// wire-byte span that was consumed (zero-copy).
    ///
    /// Same stop / nesting semantics as [`skip_until`](Self::skip_until). After
    /// [`FieldValue::SGroup`], pair with [`until_group_ends`] to borrow the
    /// group body through the matching end tag (start tag already consumed).
    ///
    /// On error the cursor may already have advanced through some fields; there
    /// is no partial span return.
    ///
    /// ```
    /// use ::protobuf_core::{AsRefExtProtobuf, FieldValue, until_group_ends};
    ///
    /// let bytes = [0x0B, 0x10, 0x01, 0x0C, 0x18, 0x07];
    /// let mut fields = bytes.as_slice().read_protobuf_fields();
    /// let start = fields.next().unwrap().unwrap();
    /// assert!(matches!(start.value, FieldValue::SGroup));
    ///
    /// let body = fields.take_until(until_group_ends(start.field_number)).unwrap();
    /// assert_eq!(body, &[0x10, 0x01, 0x0C]);
    /// ```
    pub fn take_until<P>(&mut self, pred: P) -> Result<&'a [u8]>
    where
        P: FnMut(Tag) -> Result<bool>,
    {
        let start = self.slice;
        self.skip_until(pred)?;
        let taken = start.len() - self.slice.len();
        Ok(&start[..taken])
    }

    /// Copy fields to `out` until `pred` returns `Ok(true)`.
    ///
    /// Implemented via [`take_until`](Self::take_until) so the written bytes are
    /// the exact wire encoding (including non-canonical varints). Returns the
    /// number of bytes written. On error before a successful take, `out` is
    /// unchanged; see [`take_until`](Self::take_until) for cursor advancement.
    pub fn copy_until<P, W>(&mut self, pred: P, out: &mut W) -> Result<usize>
    where
        P: FnMut(Tag) -> Result<bool>,
        W: Write,
    {
        let span = self.take_until(pred)?;
        out.write_all(span)?;
        Ok(span.len())
    }
}

impl<'a> Iterator for FieldIterFromSlice<'a> {
    type Item = Result<Field<&'a [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = SliceReader {
            slice: &mut self.slice,
        };
        if reader.remaining() == 0 {
            return None;
        }
        Some(reader.parse_field())
    }
}

/// Extension trait for reading raw Protocol Buffer fields from slice types
///
/// This trait provides low-level utilities for reading field-by-field from a byte slice.
/// It returns borrowed references (`Field<&'a [u8]>`) suitable for slice sources.
/// The slice length is the bound; there is no extra `len` argument.
/// It does not provide semantic interpretation - that is the caller's responsibility.
///
/// # Example
/// ```
/// use ::protobuf_core::AsRefExtProtobuf;
///
/// let slice = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
///
/// for field_result in slice.read_protobuf_fields() {
///     let field = field_result?;
///     // Process field...
/// }
/// # Ok::<(), protobuf_core::ProtobufError>(())
/// ```
pub trait AsRefExtProtobuf: AsRef<[u8]> {
    /// Read raw protobuf fields from the slice, returning an iterator
    ///
    /// This returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Returns borrowed references (`Field<&'a [u8]>`).
    ///
    /// This method is available for all types implementing `AsRef<[u8]>`.
    fn read_protobuf_fields(&self) -> FieldIterFromSlice<'_> {
        FieldIterFromSlice {
            slice: self.as_ref(),
        }
    }
}

impl<T> AsRefExtProtobuf for T where T: AsRef<[u8]> + ?Sized {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::field::FieldValue;

    // ============================================================================
    // AsRefExtProtobuf tests (borrowed references)
    // ============================================================================

    #[test]
    fn test_as_ref_ext_read_varint_field() {
        use super::AsRefExtProtobuf;

        let reader = &[0x08, 0x96, 0x01][..]; // field 1: 150

        let fields: Vec<_> = reader
            .read_protobuf_fields()
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
    fn test_as_ref_ext_read_len_field() {
        use super::AsRefExtProtobuf;

        let reader = &[0x12, 0x03, 0x48, 0x65, 0x6c][..]; // field 2: "Hel"

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 2);
        match &field.value {
            FieldValue::Len(data) => {
                assert_eq!(&data[..], b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_as_ref_ext_read_i32_field() {
        use super::AsRefExtProtobuf;

        let reader = &[0x15, 0x78, 0x56, 0x34, 0x12][..]; // field 2: 0x12345678 (Fixed32)

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 2);
        match &field.value {
            FieldValue::I32(bytes) => {
                assert_eq!(*bytes, [0x78, 0x56, 0x34, 0x12]);
            }
            _ => panic!("Expected I32 field"),
        }
    }

    #[test]
    fn test_as_ref_ext_read_i64_field() {
        use super::AsRefExtProtobuf;

        let reader = &[0x19, 0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12][..]; // field 3: 0x1234567890ABCDEF (Fixed64)

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 3);
        match &field.value {
            FieldValue::I64(bytes) => {
                assert_eq!(*bytes, [0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]);
            }
            _ => panic!("Expected I64 field"),
        }
    }

    #[test]
    fn test_as_ref_ext_read_multiple_fields() {
        use super::AsRefExtProtobuf;

        let reader = &[
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ][..];

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 2);

        // Check first field
        let field = &fields[0];
        assert_eq!(field.field_number.as_u32(), 1);
        match &field.value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }

        // Check second field
        let field = &fields[1];
        assert_eq!(field.field_number.as_u32(), 2);
        match &field.value {
            FieldValue::Len(data) => {
                assert_eq!(&data[..], b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_as_ref_ext_read_empty_stream() {
        use super::AsRefExtProtobuf;

        let reader = &[][..];

        let fields: Vec<_> = reader
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 0);
    }

    #[test]
    fn test_as_ref_ext_works_with_unsized_type() {
        use super::AsRefExtProtobuf;

        // This test verifies that ?Sized bound allows the trait to work with unsized types
        // Without ?Sized, this would fail to compile because [u8] is unsized
        fn accepts_unsized<T: AsRefExtProtobuf + ?Sized>(x: &T) {
            let _ = x.read_protobuf_fields();
        }

        let data = [0x08, 0x96, 0x01]; // field 1: 150
        accepts_unsized(&data[..]); // &[u8] - unsized type slice

        // Also test with direct unsized slice
        let slice: &[u8] = &[0x08, 0x96, 0x01];
        let fields: Vec<_> = slice
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 1);
    }

    // ============================================================================
    // ReadExtProtobuf tests (owned data from Read)
    // ============================================================================

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
        // Remaining bytes after the Take are still on the cursor.
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
        // underlying reader can supply. After the field is consumed, Take still
        // has remaining capacity while the cursor is at EOF, so the next poll is
        // UnexpectedEof (not a soft end).
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
        use ::std::io::{Cursor, ErrorKind};

        // Mid-field truncation on the Read path: `read_exact` surfaces as IoError.
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
                matches!(
                    error,
                    ProtobufError::IoError(ref err) if err.kind() == ErrorKind::UnexpectedEof
                ),
                "input={input:?} got {error:?}"
            );
        }
    }

    #[test]
    fn test_as_ref_ext_algorithm_edges() {
        use super::AsRefExtProtobuf;
        use crate::ProtobufError;

        let multi_byte_tag = &[0x80, 0x01, 0x01][..];
        let fields: Vec<_> = multi_byte_tag
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields[0].field_number.as_u32(), 16);
        match &fields[0].value {
            FieldValue::Varint(varint) => assert_eq!(varint.to_uint64(), 1),
            other => panic!("expected varint, got {other:?}"),
        }

        let empty_len = &[0x0A, 0x00][..];
        let fields: Vec<_> = empty_len
            .read_protobuf_fields()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        match &fields[0].value {
            FieldValue::Len(data) => assert!(data.is_empty()),
            other => panic!("expected empty len, got {other:?}"),
        }

        let truncated_i32 = (&[0x0D, 0x01, 0x02, 0x03][..])
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_i32, Err(ProtobufError::UnexpectedEof)));

        let truncated_i64 = (&[0x09, 0, 0, 0, 0, 0, 0, 0][..])
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_i64, Err(ProtobufError::UnexpectedEof)));

        let truncated_len = (&[0x0A, 0x05, b'a', b'b'][..])
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_len, Err(ProtobufError::UnexpectedEof)));

        let length_past_u32 = (&[0x0A, 0x80, 0x80, 0x80, 0x80, 0x10][..])
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(
            length_past_u32,
            Err(ProtobufError::VarintDowncastOutOfRange {
                value: 0x1_0000_0000,
                target_type: "u32",
            })
        ));

        // Wire types 3 and 4 are group tags; field readers return them as values.
        for (byte, expected) in [(0x0Bu8, FieldValue::SGroup), (0x0C, FieldValue::EGroup)] {
            let bytes = [byte];
            let field = bytes[..].read_protobuf_fields().next().unwrap().unwrap();
            assert_eq!(field.field_number.as_u32(), 1);
            assert_eq!(field.value, expected);
        }
    }
}

#[cfg(test)]
mod group_preserve {
    use super::{AsRefExtProtobuf, ReadExtProtobuf, until_group_ends};
    use crate::field_number::FieldNumber;
    use crate::{FieldValue, ProtobufError, WriteExtProtobuf};
    use ::std::io::Cursor;

    fn slice_fields(bytes: &[u8]) -> super::FieldIterFromSlice<'_> {
        AsRefExtProtobuf::read_protobuf_fields(bytes)
    }

    fn expect_sgroup(fields: &mut super::FieldIterFromSlice<'_>) -> FieldNumber {
        let f = fields.next().unwrap().unwrap();
        assert!(matches!(f.value, FieldValue::SGroup));
        f.field_number
    }

    fn message_with_group_then_varint() -> Vec<u8> {
        vec![
            0x0B, // field 1 SGroup
            0x10, 0x96, 0x01, // field 2 varint 150
            0x0C, // field 1 EGroup
            0x18, 0x07, // field 3 varint 7
        ]
    }

    #[test]
    fn take_empty_group_body() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let body = fields.take_until(until_group_ends(field_number)).unwrap();
        assert_eq!(body, &[0x0C]);
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 1);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
    }

    #[test]
    fn take_group_with_nested_scalars() {
        let bytes = message_with_group_then_varint();
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let body = fields.take_until(until_group_ends(field_number)).unwrap();
        assert_eq!(body, &[0x10, 0x96, 0x01, 0x0C]);
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 3);
    }

    #[test]
    fn take_nested_groups() {
        let bytes = [
            0x0B, // 1 SGroup
            0x13, // 2 SGroup
            0x18, 0x01, // 3 varint 1
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
            0x20, 0x02, // 4 varint 2
        ];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let body = fields.take_until(until_group_ends(field_number)).unwrap();
        assert_eq!(body, &[0x13, 0x18, 0x01, 0x14, 0x0C]);
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 4);
    }

    #[test]
    fn take_group_containing_len_and_fixed() {
        let bytes = [
            0x0B, // 1 SGroup
            0x12, 0x03, b'a', b'b', b'c', // field 2 len "abc"
            0x15, 0x01, 0x02, 0x03, 0x04, // field 2 fixed32
            0x09, 0, 0, 0, 0, 0, 0, 0, 0,    // field 1 fixed64
            0x0C, // 1 EGroup
            0x10, 0x05, // field 2 varint 5
        ];
        let expected_body = &bytes[1..bytes.len() - 2];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let body = fields.take_until(until_group_ends(field_number)).unwrap();
        assert_eq!(body, expected_body);
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 2);
    }

    #[test]
    fn take_mismatched_end_group_is_an_error() {
        let bytes = [0x0B, 0x14];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let err = fields
            .take_until(until_group_ends(field_number))
            .unwrap_err();
        assert!(
            matches!(
                err,
                ProtobufError::MismatchedEndGroup { expected, got }
                if expected.as_u32() == 1 && got.as_u32() == 2
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn take_truncated_group_is_unexpected_eof() {
        let bytes = [0x0B, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        assert!(matches!(
            fields.take_until(until_group_ends(field_number)),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn slice_copy_until_matches_take_until() {
        let bytes = message_with_group_then_varint();
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let mut out = Vec::new();
        let n = fields
            .copy_until(until_group_ends(field_number), &mut out)
            .unwrap();
        assert_eq!(n, out.len());
        assert_eq!(out, vec![0x10, 0x96, 0x01, 0x0C]);
    }

    #[test]
    fn read_copy_until_preserves_group_body() {
        let bytes = message_with_group_then_varint();
        let len = bytes.len();
        let mut fields = Cursor::new(bytes.clone()).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut out = Vec::new();
        fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x10, 0x96, 0x01, 0x0C]);

        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn read_copy_until_nested_groups_and_len() {
        let bytes = [
            0x0B, // 1 SGroup
            0x13, // 2 SGroup
            0x12, 0x02, b'x', b'y', // field 2 len "xy" inside inner group
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
        ];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        let mut out = Vec::new();
        fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x13, 0x12, 0x02, b'x', b'y', 0x14, 0x0C]);
        assert!(fields.next().is_none());
    }

    #[test]
    fn read_copy_until_mismatched_end_group() {
        let bytes = [0x0B, 0x14];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        let mut out = Vec::new();
        let err = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap_err();
        assert!(matches!(
            err,
            ProtobufError::MismatchedEndGroup { expected, got }
            if expected.as_u32() == 1 && got.as_u32() == 2
        ));
        // Partial write includes the mismatched end-group tag that failed pred.
        assert_eq!(out, vec![0x14]);
    }

    #[test]
    fn round_trip_preserved_group_reparsed() {
        let bytes = [
            0x0B, // 1 SGroup
            0x10, 0x01, // 2 varint 1
            0x12, 0x03, b'a', b'b', b'c', // 2 len
            0x15, 0x11, 0x22, 0x33, 0x44, // 2 fixed32
            0x13, // 2 SGroup
            0x18, 0x02, // 3 varint 2
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
        ];
        let mut fields = slice_fields(&bytes);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut preserved = Vec::new();
        preserved.write_protobuf_field(&start).unwrap();
        let body = fields
            .take_until(until_group_ends(start.field_number))
            .unwrap();
        preserved.extend_from_slice(body);
        assert_eq!(preserved.as_slice(), bytes.as_slice());

        let reparsed: Vec<_> = AsRefExtProtobuf::read_protobuf_fields(preserved.as_slice())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(reparsed.len(), 8);
        assert!(matches!(reparsed[0].value, FieldValue::SGroup));
        assert_eq!(reparsed[0].field_number.as_u32(), 1);
        assert!(matches!(&reparsed[1].value, FieldValue::Varint(v) if v.to_uint64() == 1));
        assert!(matches!(reparsed[2].value, FieldValue::Len(b"abc")));
        assert!(matches!(
            &reparsed[3].value,
            FieldValue::I32([0x11, 0x22, 0x33, 0x44])
        ));
        assert!(matches!(reparsed[4].value, FieldValue::SGroup));
        assert_eq!(reparsed[4].field_number.as_u32(), 2);
        assert!(matches!(&reparsed[5].value, FieldValue::Varint(v) if v.to_uint64() == 2));
        assert!(matches!(reparsed[6].value, FieldValue::EGroup));
        assert_eq!(reparsed[6].field_number.as_u32(), 2);
        assert!(matches!(reparsed[7].value, FieldValue::EGroup));
        assert_eq!(reparsed[7].field_number.as_u32(), 1);
    }

    #[test]
    fn read_copy_until_round_trip_matches_slice_take() {
        let bytes = message_with_group_then_varint();
        let group_only = &bytes[..bytes.len() - 2]; // drop trailing field 3

        let mut slice_fields = slice_fields(group_only);
        let start = slice_fields.next().unwrap().unwrap();
        let mut from_slice = Vec::new();
        from_slice.write_protobuf_field(&start).unwrap();
        from_slice.extend_from_slice(
            slice_fields
                .take_until(until_group_ends(start.field_number))
                .unwrap(),
        );

        let len = group_only.len();
        let mut read_fields = Cursor::new(group_only.to_vec()).read_protobuf_fields(len);
        let start = read_fields.next().unwrap().unwrap();
        let mut from_read = Vec::new();
        from_read.write_protobuf_field(&start).unwrap();
        read_fields
            .copy_until(until_group_ends(start.field_number), &mut from_read)
            .unwrap();

        assert_eq!(from_slice, group_only);
        assert_eq!(from_read, from_slice);
    }

    #[test]
    fn read_copy_until_preserves_noncanonical_varints() {
        // Overlong length prefix (1 encoded as 0x81 0x00) must not be
        // re-canonicalized to 0x01 when copying through Read.
        let bytes = [
            0x0B, // 1 SGroup
            0x12, 0x81, 0x00, b'x', // 2 Len, non-canonical length=1, payload 'x'
            0x0C, // 1 EGroup
        ];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        let mut out = Vec::new();
        fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x12, 0x81, 0x00, b'x', 0x0C]);
    }
}

#[cfg(test)]
mod group_skip {
    use super::{AsRefExtProtobuf, ReadExtProtobuf, SliceReader, WireCursor, until_group_ends};
    use crate::field_number::FieldNumber;
    use crate::wire_format::WireType;
    use crate::{FieldValue, ProtobufError};

    fn field(n: u32) -> FieldNumber {
        FieldNumber::try_new(n).unwrap()
    }

    fn slice_fields(bytes: &[u8]) -> super::FieldIterFromSlice<'_> {
        AsRefExtProtobuf::read_protobuf_fields(bytes)
    }

    /// Take the next field and assert it is an `SGroup`; return its field number.
    fn expect_sgroup(fields: &mut super::FieldIterFromSlice<'_>) -> FieldNumber {
        let field = fields.next().unwrap().unwrap();
        assert!(
            matches!(field.value, FieldValue::SGroup),
            "expected SGroup, got {:?}",
            field.value
        );
        field.field_number
    }

    /// field 1 start-group, nested field 2 varint 150, field 1 end-group, then
    /// field 3 varint 7.
    fn message_with_group_then_varint() -> Vec<u8> {
        vec![
            0x0B, // field 1 SGroup
            0x10, 0x96, 0x01, // field 2 varint 150
            0x0C, // field 1 EGroup
            0x18, 0x07, // field 3 varint 7
        ]
    }

    #[test]
    fn skip_empty_group_then_read_next_field() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01]; // empty group 1, then field 1 = 1
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        fields.skip_until(until_group_ends(field_number)).unwrap();
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 1);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
        assert!(fields.next().is_none());
    }

    #[test]
    fn skip_group_with_nested_fields_on_slice_iter() {
        let bytes = message_with_group_then_varint();
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        assert_eq!(field_number.as_u32(), 1);
        fields.skip_until(until_group_ends(field_number)).unwrap();
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn skip_nested_groups() {
        // outer 1 { inner 2 { field 3 = 1 } } then field 4 = 2
        let bytes = [
            0x0B, // 1 SGroup
            0x13, // 2 SGroup
            0x18, 0x01, // 3 varint 1
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
            0x20, 0x02, // 4 varint 2
        ];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        fields.skip_until(until_group_ends(field_number)).unwrap();
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 4);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 2);
    }

    #[test]
    fn skip_group_containing_len_and_fixed() {
        let bytes = [
            0x0B, // 1 SGroup
            0x12, 0x03, b'a', b'b', b'c', // field 2 len "abc"
            0x15, 0x01, 0x02, 0x03, 0x04, // field 2 fixed32
            0x09, 0, 0, 0, 0, 0, 0, 0, 0,    // field 1 fixed64
            0x0C, // 1 EGroup
            0x10, 0x05, // field 2 varint 5
        ];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        fields.skip_until(until_group_ends(field_number)).unwrap();
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 2);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 5);
    }

    #[test]
    fn read_group_body_field_by_field_until_egroup() {
        let bytes = message_with_group_then_varint();
        let mut fields = slice_fields(&bytes);
        assert_eq!(expect_sgroup(&mut fields).as_u32(), 1);

        let inner = fields.next().unwrap().unwrap();
        assert_eq!(inner.field_number.as_u32(), 2);
        let FieldValue::Varint(v) = inner.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 150);

        let end = fields.next().unwrap().unwrap();
        assert_eq!(end.field_number.as_u32(), 1);
        assert!(matches!(end.value, FieldValue::EGroup));

        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn mismatched_end_group_is_an_error() {
        // start field 1, end field 2
        let bytes = [0x0B, 0x14];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let skip_err = fields
            .skip_until(until_group_ends(field_number))
            .unwrap_err();
        assert!(
            matches!(
                skip_err,
                ProtobufError::MismatchedEndGroup { expected, got }
                if expected.as_u32() == 1 && got.as_u32() == 2
            ),
            "got {skip_err:?}"
        );
    }

    #[test]
    fn truncated_group_is_unexpected_eof() {
        let bytes = [0x0B, 0x08, 0x01]; // start group, varint field, no end
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        assert!(matches!(
            fields.skip_until(until_group_ends(field_number)),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn skip_via_read_ext_after_start_tag() {
        use ::std::io::{Cursor, Read};

        let bytes = message_with_group_then_varint();
        let mut reader = Cursor::new(bytes);
        assert_eq!(
            reader
                .by_ref()
                .read_protobuf_fields(1)
                .next()
                .unwrap()
                .unwrap()
                .value,
            FieldValue::SGroup
        );
        let remaining = reader.get_ref().len() - reader.position() as usize;
        let mut fields = reader.read_protobuf_fields(remaining);
        fields.skip_until(until_group_ends(field(1))).unwrap();
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
    }

    #[test]
    fn skip_field_returns_tag_including_groups() {
        let bytes = message_with_group_then_varint();
        let mut slice = bytes.as_slice();
        let mut cursor = SliceReader { slice: &mut slice };

        let start = cursor.skip_field().unwrap();
        assert_eq!(start.field_number.as_u32(), 1);
        assert_eq!(start.wire_type, WireType::SGroup);

        cursor
            .skip_until(until_group_ends(start.field_number))
            .unwrap();

        let after = cursor.skip_field().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        assert_eq!(after.wire_type, WireType::Varint);
        assert_eq!(cursor.remaining(), 0);
    }

    #[test]
    fn skip_field_on_slice_reader_skips_len_payload() {
        // field 1 SGroup, field 2 len "ab", field 1 EGroup
        let bytes = [0x0B, 0x12, 0x02, b'a', b'b', 0x0C];
        let mut slice = bytes.as_slice();
        let mut cursor = SliceReader { slice: &mut slice };

        let start = cursor.skip_field().unwrap();
        assert_eq!(start.wire_type, WireType::SGroup);
        cursor
            .skip_until(until_group_ends(start.field_number))
            .unwrap();
        assert_eq!(cursor.remaining(), 0);
        assert!(slice.is_empty());
    }

    #[test]
    fn skip_via_field_iter_from_read() {
        use ::std::io::Cursor;

        let bytes = message_with_group_then_varint();
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        fields
            .skip_until(until_group_ends(start.field_number))
            .unwrap();
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn skip_group_with_len_payload_on_read_iter() {
        use ::std::io::Cursor;

        // Exercises ReadCursor::skip_bytes via Len inside skip_until.
        // field 1 SGroup, field 2 len "ab", field 1 EGroup, field 3 varint 7
        let bytes = [0x0B, 0x12, 0x02, b'a', b'b', 0x0C, 0x18, 0x07];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        fields
            .skip_until(until_group_ends(start.field_number))
            .unwrap();
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn skip_large_len_payload_on_read_iter() {
        use crate::varint::Varint;
        use ::std::io::Cursor;

        // Len payload longer than ReadCursor's 256-byte skip buffer so the
        // multi-chunk skip_bytes loop runs.
        const PAYLOAD_LEN: usize = 300;
        let mut bytes = vec![0x0B]; // field 1 SGroup
        bytes.push(0x12); // field 2 Len
        let (encoded, count) = Varint::from_uint64(PAYLOAD_LEN as u64).encode();
        bytes.extend_from_slice(&encoded[..count]);
        bytes.extend(::std::iter::repeat_n(0u8, PAYLOAD_LEN));
        bytes.push(0x0C); // field 1 EGroup
        bytes.extend_from_slice(&[0x18, 0x07]); // field 3 varint 7

        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        fields
            .skip_until(until_group_ends(start.field_number))
            .unwrap();
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }
}

#[cfg(test)]
mod len_payload {
    use super::ReadExtProtobuf;
    use crate::varint::Varint;
    use crate::{Field, FieldValue, ProtobufError};
    use ::std::convert::AsMut;
    use ::std::io::{Cursor, ErrorKind};

    const LARGE_LEN: usize = 8200;

    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    impl AsMut<[u8]> for Bytes {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }
    }

    fn bytes_buf(len: usize) -> crate::Result<Bytes> {
        Ok(Bytes(vec![0; len]))
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
            .read_protobuf_fields_with(len, bytes_buf)
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
        // `Read::read_exact` reports a short buffer as an I/O error.
        let error = Cursor::new(input)
            .read_protobuf_fields_with(input.len(), bytes_buf)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(
            error,
            ProtobufError::IoError(ref err) if err.kind() == ErrorKind::UnexpectedEof
        ));
    }

    fn assert_box(message: &[u8], expected: &[u8]) {
        let from_read = Cursor::new(message.to_vec())
            .read_protobuf_fields_with(message.len(), |len| Ok(vec![0u8; len].into_boxed_slice()))
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

    impl AsMut<[u8]> for Tagged {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.data
        }
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
            .read_protobuf_fields_with(len, |payload_len| {
                Ok(Tagged {
                    id,
                    data: vec![0; payload_len],
                })
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
            .read_protobuf_fields_with(mixed.len(), |len| {
                calls += 1;
                Ok(Bytes(vec![0; len]))
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
            .read_protobuf_fields_with(input.len(), |_len| -> crate::Result<Vec<u8>> {
                Err(ProtobufError::UnexpectedEof)
            })
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    #[derive(Debug)]
    struct Spare {
        data: Vec<u8>,
    }

    impl AsMut<[u8]> for Spare {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.data
        }
    }

    fn spare(len: usize) -> crate::Result<Spare> {
        Ok(Spare {
            data: vec![0xAB; len + 4],
        })
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
            .read_protobuf_fields_with(message.len(), spare)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_spare(&from_read);
    }

    #[test]
    #[should_panic(expected = "shorter than payload length")]
    fn short_buffer_panics_on_read() {
        let input = [0x0A, 0x03, b'H', b'e', b'l'];
        let _ = Cursor::new(input)
            .read_protobuf_fields_with(input.len(), |len| Ok(Vec::<u8>::with_capacity(len)))
            .next();
    }
}
