//! Streaming field iteration over a [`LenBody`].
//!
//! [`LenBody`] is what Len handlers receive **by value**: the delimited payload
//! (or top-level message) over `&mut R` with a `u32`
//! [`remaining`](LenBody::remaining). The handler signature is HRTB so the body
//! cannot escape the closure; on [`Drop`] any leftover count is written back to
//! a caller slot (no I/O in [`Drop`]). For Len fields the library then discards
//! that many bytes on the fallible parse path; top-level unused budget stays on
//! the underlying reader. [`LenBody`] itself does **not** implement [`Read`];
//! bytes/string copy uses consuming [`into_read`](LenBody::into_read), and
//! nested decode uses [`fields`](LenBody::fields) /
//! [`fields_with`](LenBody::fields_with). Top-level message `len` is expected
//! to be `<= MAX_MESSAGE_SIZE` ([`crate::MAX_MESSAGE_SIZE`], 2 GiB); that is
//! `debug_assert`ed at the entrance (not enforced in release).

use super::group::GroupBody;
use super::wire::WireCursor;
use crate::field::{Field, FieldValue};
use crate::field_number::FieldNumber;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::{
    MAX_MESSAGE_SIZE, MAX_VARINT_BYTES, VARINT_CONTINUATION_BIT, VARINT_PAYLOAD_MASK, WireType,
};
use crate::{ProtobufError, Result};
use ::std::convert::{AsMut, AsRef};
use ::std::io::{Read, Write};

// ============================================================================
// Len / message body view
// ============================================================================

/// Body of a length-delimited field (or the top-level message), over `&'a mut R`.
///
/// Len handlers take this **by value**:
/// `for<'t> FnMut(FieldNumber, LenBody<'t, R>) -> Result<L>` (HRTB so it cannot
/// escape the closure). [`remaining`](Self::remaining) at entry is the payload
/// length. On [`Drop`], leftover `remaining` is written to an optional caller
/// slot when nested under a parent body (no I/O). For Len fields the library
/// then discards that many bytes from the reader on the fallible path. Nested
/// messages call [`fields`](Self::fields) / [`fields_with`](Self::fields_with)
/// (consuming `self`).
///
/// Does not implement [`Read`] itself (so field/varint extension traits do not
/// attach directly). Modes that consume `self`:
/// [`into_read`](Self::into_read) (bytes/string via [`LenBodyRead`]),
/// [`varints`](Self::varints) / [`fixed32s`](Self::fixed32s) /
/// [`fixed64s`](Self::fixed64s), and [`fields`](Self::fields)*.
///
/// # Contract
///
/// `remaining()` always fits in [`u32`] by construction. Top-level message
/// lengths also stay `<= MAX_MESSAGE_SIZE` at [`ReadExtProtobuf`] entrances
/// (`debug_assert` only). Do not `mem::forget` a scoped [`LenBody`] /
/// [`LenBodyRead`]: [`Drop`] must run so leftover writeback stays accurate
/// (forgetting leaves leftover at `0` and underskips; prefer that to overskip).
///
/// [`ReadExtVarint`]: crate::ReadExtVarint
/// [`ReadExtTag`]: crate::ReadExtTag
pub struct LenBody<'a, R: ?Sized> {
    reader: &'a mut R,
    remaining: u32,
    /// When set, [`Drop`] does `*remaining_out = remaining` (Len [`scope`] slot).
    remaining_out: Option<&'a mut u32>,
}

impl<'a, R: ?Sized> Drop for LenBody<'a, R> {
    #[inline]
    fn drop(&mut self) {
        if let Some(ref mut out) = self.remaining_out {
            **out = self.remaining;
        }
    }
}

impl<'a, R: ?Sized> AsRef<LenBody<'a, R>> for LenBody<'a, R> {
    #[inline]
    fn as_ref(&self) -> &LenBody<'a, R> {
        self
    }
}

impl<'a, R: ?Sized> AsMut<LenBody<'a, R>> for LenBody<'a, R> {
    #[inline]
    fn as_mut(&mut self) -> &mut LenBody<'a, R> {
        self
    }
}

impl<'a, R: Read + ?Sized> LenBody<'a, R> {
    /// Crate-internal: top-level message body (unused bytes stay on the
    /// underlying reader when this value is dropped; no `remaining_out`).
    #[inline]
    pub(crate) fn new(reader: &'a mut R, len: u32) -> Self {
        Self {
            reader,
            remaining: len,
            remaining_out: None,
        }
    }

    /// Bytes still allowed in this body.
    #[inline]
    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    /// Decode one varint from this body (empty body is
    /// [`ProtobufError::UnexpectedEof`]).
    #[inline]
    fn read_varint_with_raw<'b>(
        &'b mut self,
        buf: &'b mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'b [u8])> {
        let mut decoded_value = 0u64;
        let mut shift = 0u32;
        let mut n = 0usize;
        while n < MAX_VARINT_BYTES {
            if self.remaining == 0 {
                return Err(ProtobufError::UnexpectedEof);
            }
            let mut byte = [0u8; 1];
            read_body_exact(self, &mut byte)?;
            buf[n] = byte[0];
            n += 1;
            let value = (byte[0] & VARINT_PAYLOAD_MASK) as u64;
            if n == MAX_VARINT_BYTES && value > 1 {
                return Err(ProtobufError::VarintTooLong);
            }
            decoded_value |= value << shift;
            if byte[0] & VARINT_CONTINUATION_BIT == 0 {
                return Ok((Varint::new(decoded_value.to_le_bytes()), &buf[..n]));
            }
            shift += 7;
        }
        Err(ProtobufError::VarintTooLong)
    }

    /// Treat this body as a byte stream (consumes `self`).
    ///
    /// For `string` / `bytes` (or any opaque Len payload) copy via
    /// [`Read`](std::io::Read) helpers (`read_exact`, `read_to_end`, …). Soft
    /// EOF when [`remaining`](Self::remaining) is exhausted. Prefer
    /// [`fields`](Self::fields)* / packed iterators for structured decode.
    #[inline]
    pub fn into_read(self) -> LenBodyRead<'a, R> {
        LenBodyRead { body: self }
    }

    /// Iterate packed varints from this body (consumes `self`).
    ///
    /// Soft-stops when [`remaining`](Self::remaining) is exhausted. A truncated
    /// mid-varint is [`ProtobufError::UnexpectedEof`]. While this iterator is
    /// live, the body cannot be reinterpreted as fields or raw bytes.
    #[inline]
    pub fn varints(self) -> VarintIterFromLenBody<'a, R> {
        VarintIterFromLenBody { body: self }
    }

    /// Iterate packed `I32` values from this body (consumes `self`).
    ///
    /// Each item is four little-endian wire bytes (`fixed32` / `sfixed32` /
    /// `float`). Soft-stops when remaining is exhausted; a partial trailing
    /// element is [`ProtobufError::UnexpectedEof`].
    #[inline]
    pub fn fixed32s(self) -> FixedIterFromLenBody<'a, R, 4> {
        FixedIterFromLenBody { body: self }
    }

    /// Iterate packed `I64` values from this body (consumes `self`).
    ///
    /// Each item is eight little-endian wire bytes (`fixed64` / `sfixed64` /
    /// `double`). Soft-stops when remaining is exhausted; a partial trailing
    /// element is [`ProtobufError::UnexpectedEof`].
    #[inline]
    pub fn fixed64s(self) -> FixedIterFromLenBody<'a, R, 8> {
        FixedIterFromLenBody { body: self }
    }

    /// Child body of exactly `n` bytes (prepaid).
    ///
    /// Charges `n` against this body immediately. `*leftover` starts at `0`;
    /// on child [`Drop`], unread count is written to `leftover` (no I/O). The
    /// caller must then discard that many bytes from the underlying reader
    /// (Len parse does this). Panics if `n > remaining()`.
    #[inline]
    pub(crate) fn scope<'b>(&'b mut self, n: u32, leftover: &'b mut u32) -> LenBody<'b, R> {
        assert!(
            n <= self.remaining,
            "LenBody::scope length {n} exceeds remaining {}",
            self.remaining
        );
        self.remaining -= n;
        *leftover = 0;
        LenBody {
            reader: &mut *self.reader,
            remaining: n,
            remaining_out: Some(leftover),
        }
    }

    /// Iterate fields from this body (consumes `self`).
    ///
    /// Consumes the wire bytes of each field the iterator has advanced over
    /// (including unread Len body bytes discarded after each Len handler).
    /// Fields never yielded leave their bytes in this body until [`Drop`]
    /// writeback / the parent's discard.
    #[inline]
    pub fn fields(self) -> FieldIter<'a, R, Self, VecLenPayload> {
        FieldIter {
            body: self,
            handler: VecLenPayload,
            failed: false,
            _marker: ::std::marker::PhantomData,
        }
    }

    /// Like [`fields`](Self::fields), but each length-delimited field is built
    /// by `handler` instead of [`VecLenPayload`].
    ///
    /// The body is passed **by value** (`remaining()` at entry is the payload
    /// length). After return, leftover body bytes are discarded. Do not move
    /// the `LenBody` out (HRTB). Non-Len wire types never call `handler`.
    /// Custom stateful handlers can wrap as
    /// `|n, body| my_handler.handle(n, body)`.
    #[inline]
    pub fn fields_with<L, F>(self, handler: F) -> FieldIter<'a, R, Self, F>
    where
        F: for<'t> FnMut(FieldNumber, LenBody<'t, R>) -> Result<L>,
    {
        FieldIter {
            body: self,
            handler,
            failed: false,
            _marker: ::std::marker::PhantomData,
        }
    }

    /// Parse one protobuf field from this body.
    ///
    /// When the field is length-delimited, `handler` runs once with a child
    /// [`LenBody`] by value for the payload length. After it returns (and
    /// [`Drop`] writes leftover into a local slot), unused body bytes are
    /// discarded. Handler errors are preferred over discard I/O errors. Other
    /// wire types do not call `handler`.
    #[inline]
    fn parse_field_with<F>(&mut self, handler: &mut F) -> Result<Field<F::Payload>>
    where
        F: LenHandler<R>,
    {
        let tag = WireCursor::read_tag(self)?;
        let value = match tag.wire_type {
            WireType::Varint => FieldValue::Varint(WireCursor::read_varint(self)?),
            WireType::Int32 => FieldValue::I32(WireCursor::read_fixed(self)?),
            WireType::Int64 => FieldValue::I64(WireCursor::read_fixed(self)?),
            WireType::Len => {
                let length = WireCursor::read_varint(self)?.try_to_uint32()?;
                if length > self.remaining() {
                    return Err(ProtobufError::UnexpectedEof);
                }
                let mut left = 0;
                let result = handler.handle(tag.field_number, self.scope(length, &mut left));
                // `LenBody` Drop wrote unread count into `left`.
                let discard = discard_reader_bytes(self.reader, left);
                match (result, discard) {
                    (Err(handler_err), _) => return Err(handler_err),
                    (Ok(_), Err(discard_err)) => return Err(discard_err),
                    (Ok(payload), Ok(())) => FieldValue::Len(payload),
                }
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

/// [`Read`] view of a consumed [`LenBody`].
///
/// Returned by [`LenBody::into_read`]. Caps reads by the body's remaining
/// budget (soft `Ok(0)` when exhausted), like a `u32`-bounded
/// [`std::io::Take`]. Must be dropped (do not `mem::forget`) so the inner
/// [`LenBody`] can write leftover into its scope slot.
pub struct LenBodyRead<'a, R: ?Sized> {
    body: LenBody<'a, R>,
}

impl<'a, R: Read + ?Sized> Read for LenBodyRead<'a, R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        let max = buf.len().min(self.body.remaining as usize);
        if max == 0 {
            return Ok(0);
        }
        loop {
            match self.body.reader.read(&mut buf[..max]) {
                Ok(n) => {
                    self.body.remaining -= n as u32;
                    return Ok(n);
                }
                Err(err) if err.kind() == ::std::io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            }
        }
    }
}

/// Iterator of packed varints from a consumed [`LenBody`].
///
/// Returned by [`LenBody::varints`]. Soft-stops at body end; truncated
/// mid-varint is [`ProtobufError::UnexpectedEof`].
pub struct VarintIterFromLenBody<'a, R: ?Sized> {
    body: LenBody<'a, R>,
}

impl<'a, R: Read + ?Sized> Iterator for VarintIterFromLenBody<'a, R> {
    type Item = Result<Varint>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.body.remaining() == 0 {
            return None;
        }
        Some(
            self.body
                .read_varint_with_raw(&mut [0u8; MAX_VARINT_BYTES])
                .map(|(v, _)| v),
        )
    }
}

/// Iterator of packed fixed-width values from a consumed [`LenBody`].
///
/// Returned by [`LenBody::fixed32s`] (`N = 4`) and [`LenBody::fixed64s`]
/// (`N = 8`). Soft-stops at body end; a partial trailing element is
/// [`ProtobufError::UnexpectedEof`].
pub struct FixedIterFromLenBody<'a, R: ?Sized, const N: usize> {
    body: LenBody<'a, R>,
}

impl<'a, R: Read + ?Sized, const N: usize> Iterator for FixedIterFromLenBody<'a, R, N> {
    type Item = Result<[u8; N]>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.body.remaining() == 0 {
            return None;
        }
        let mut bytes = [0u8; N];
        Some(read_body_exact(&mut self.body, &mut bytes).map(|()| bytes))
    }
}

/// Read up to `buf.len()` bytes from `body`, capped by remaining (soft `Ok(0)`).
#[inline]
fn read_body<R: Read + ?Sized>(body: &mut LenBody<'_, R>, buf: &mut [u8]) -> Result<usize> {
    let max = buf.len().min(body.remaining as usize);
    if max == 0 {
        return Ok(0);
    }
    loop {
        match body.reader.read(&mut buf[..max]) {
            Ok(n) => {
                body.remaining -= n as u32;
                return Ok(n);
            }
            Err(err) if err.kind() == ::std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.into()),
        }
    }
}

/// Read exactly `buf.len()` bytes from `body`.
#[inline]
fn read_body_exact<R: Read + ?Sized>(
    body: &mut LenBody<'_, R>,
    mut buf: &mut [u8],
) -> Result<()> {
    while !buf.is_empty() {
        match read_body(body, buf)? {
            0 => return Err(ProtobufError::UnexpectedEof),
            n => buf = &mut buf[n..],
        }
    }
    Ok(())
}

/// Discard exactly `left` bytes from `reader`.
#[inline]
fn discard_reader_bytes<R: Read + ?Sized>(reader: &mut R, mut left: u32) -> Result<()> {
    let mut buf = [0u8; 256];
    while left > 0 {
        let n = left.min(buf.len() as u32) as usize;
        Read::read_exact(reader, &mut buf[..n])?;
        left -= n as u32;
    }
    Ok(())
}

/// Builds a Len field payload from a scoped [`LenBody`].
///
/// Used by [`FieldIter`]. Closures
/// `FnMut(FieldNumber, LenBody<'_, R>) -> Result<L>` implement this via a
/// blanket impl. [`VecLenPayload`] is the default for
/// [`ReadExtProtobuf::read_protobuf_fields`] / [`LenBody::fields`].
pub trait LenHandler<R: ?Sized> {
    /// Payload stored in [`FieldValue::Len`].
    type Payload;

    /// Consume the Len body (by value) into a payload.
    ///
    /// `body.remaining()` at entry is the payload length. After return, the
    /// library discards any unread body bytes.
    fn handle(
        &mut self,
        field_number: FieldNumber,
        body: LenBody<'_, R>,
    ) -> Result<Self::Payload>;
}

impl<R, L, F> LenHandler<R> for F
where
    R: Read + ?Sized,
    F: for<'t> FnMut(FieldNumber, LenBody<'t, R>) -> Result<L>,
{
    type Payload = L;

    #[inline]
    fn handle(
        &mut self,
        field_number: FieldNumber,
        body: LenBody<'_, R>,
    ) -> Result<Self::Payload> {
        self(field_number, body)
    }
}

/// Default [`LenHandler`]: copy each Len body into a new `Vec<u8>`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VecLenPayload;

impl<R: Read + ?Sized> LenHandler<R> for VecLenPayload {
    type Payload = Vec<u8>;

    #[inline]
    fn handle(
        &mut self,
        _field_number: FieldNumber,
        mut body: LenBody<'_, R>,
    ) -> Result<Self::Payload> {
        let len = body.remaining();
        let mut buf = vec![0u8; len as usize];
        // Use `read_body_exact` so mid-budget truncation is
        // `UnexpectedEof` (same as fixed-width), not `IoError`.
        read_body_exact(&mut body, &mut buf)?;
        Ok(buf)
    }
}

impl<'a, R: Read + ?Sized> WireCursor for LenBody<'a, R> {
    #[inline]
    fn remaining(&self) -> u32 {
        self.remaining
    }

    #[inline]
    fn read_varint_with_raw<'b>(
        &'b mut self,
        buf: &'b mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'b [u8])> {
        LenBody::read_varint_with_raw(self, buf)
    }

    #[inline]
    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        read_body_exact(self, &mut bytes)?;
        Ok(bytes)
    }

    #[inline]
    fn skip_bytes(&mut self, mut length: u32) -> Result<()> {
        let mut buf = [0u8; 256];
        while length > 0 {
            let n = length.min(buf.len() as u32) as usize;
            read_body_exact(self, &mut buf[..n])?;
            length -= n as u32;
        }
        Ok(())
    }

    #[inline]
    fn copy_bytes<W: Write>(&mut self, length: u32, out: &mut W) -> Result<usize> {
        let mut left = length;
        let mut buf = [0u8; 256];
        while left > 0 {
            let n = left.min(buf.len() as u32) as usize;
            read_body_exact(self, &mut buf[..n])?;
            out.write_all(&buf[..n])?;
            left -= n as u32;
        }
        Ok(length as usize)
    }
}
// ============================================================================
// Field iterators
// ============================================================================

/// Iterator for reading raw protobuf fields from a [`LenBody`] holder.
///
/// `T` is typically an owned [`LenBody`] (from
/// [`ReadExtProtobuf::read_protobuf_fields`] or [`LenBody::fields`] /
/// [`fields_with`](LenBody::fields_with)). Bound with [`AsRef`] /
/// [`AsMut`]`<LenBody<'a, R>>`.
///
/// Consumes the wire bytes of each field it has advanced over (including unread
/// Len body bytes discarded after the handler). Fields never yielded leave
/// their bytes in the held [`LenBody`].
///
/// After a yielded [`Err`], further [`next`](Iterator::next) calls return
/// [`None`] (fused on error). Do not call [`skip_until`](Self::skip_until) /
/// [`copy_until`](Self::copy_until) / [`group_body`](Self::group_body) after
/// that either — they are not fused and may mis-parse a desynced cursor. The
/// reader / budget may already be wrong (mid-discard I/O failure, or a handler
/// panic that skipped discard); abandon the iterator.
pub struct FieldIter<'a, R: ?Sized, T, F = VecLenPayload> {
    body: T,
    handler: F,
    failed: bool,
    _marker: ::std::marker::PhantomData<&'a mut R>,
}

impl<'a, R, T, F> FieldIter<'a, R, T, F>
where
    R: Read + ?Sized,
    T: AsRef<LenBody<'a, R>> + AsMut<LenBody<'a, R>>,
{
    /// Skip fields until `pred` returns `Ok(true)`.
    ///
    /// Intended mainly for protobuf **group** handling: after an
    /// [`FieldValue::SGroup`] was consumed, pair with [`crate::until_group_ends`]
    /// to discard the body through the matching end tag (see
    /// [`crate::FieldIterFromSlice::skip_until`] for a runnable example).
    ///
    /// Length-delimited payloads are discarded without allocating. The byte
    /// budget is this iterator's remaining count.
    pub fn skip_until<P>(&mut self, pred: P) -> Result<()>
    where
        P: FnMut(Tag) -> Result<bool>,
    {
        WireCursor::skip_until(self.body.as_mut(), pred)
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
    /// [`FieldValue::SGroup`] was consumed, pair with [`crate::until_group_ends`]
    /// to tee the body and matching end tag (start tag already consumed — write
    /// it yourself first if the preserve blob needs it). For zero-copy
    /// preserve from a slice, use [`crate::FieldIterFromSlice::take_until`]
    /// instead.
    ///
    /// Same stop semantics as [`skip_until`](Self::skip_until). Writes exact
    /// wire bytes (including non-canonical varints). Returns the number of
    /// bytes written. On error, `out` may already contain a partial copy.
    pub fn copy_until<P, W>(&mut self, pred: P, out: &mut W) -> Result<usize>
    where
        P: FnMut(Tag) -> Result<bool>,
        W: Write,
    {
        WireCursor::copy_until(self.body.as_mut(), pred, out)
    }

    /// Bytes still allowed in this iterator's budget.
    #[inline]
    pub fn remaining(&self) -> u32 {
        self.body.as_ref().remaining()
    }
}

impl<'a, R, T, F> Iterator for FieldIter<'a, R, T, F>
where
    R: Read + ?Sized,
    T: AsRef<LenBody<'a, R>> + AsMut<LenBody<'a, R>>,
    F: LenHandler<R>,
{
    type Item = Result<Field<F::Payload>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.body.as_ref().remaining() == 0 {
            return None;
        }
        match self.body.as_mut().parse_field_with(&mut self.handler) {
            Ok(field) => Some(Ok(field)),
            Err(err) => {
                self.failed = true;
                Some(Err(err))
            }
        }
    }
}

// ============================================================================
// ReadExtProtobuf
// ============================================================================

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// Each method takes the message length `len: u32` (how many bytes of `self`
/// make up this message). Length-delimited payloads default to `Vec<u8>`.
/// [`read_protobuf_fields_with`](ReadExtProtobuf::read_protobuf_fields_with)
/// takes a Len-building closure. This trait only splits the stream into raw
/// fields (numbers and wire bytes). Building a message — choosing which
/// fields mean what, decoding their values, and assembling your own struct —
/// is up to you.
///
/// # Example
/// ```
/// use ::protobuf_core::{FieldValue, ReadExtProtobuf};
///
/// // field 1 varint = 150, field 2 string = "Hel"
/// let data = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
/// let len = data.len() as u32;
/// let mut reader = data;
///
/// for field in reader.read_protobuf_fields(len) {
///     let field = field.unwrap();
///     match field.value {
///         FieldValue::Varint(varint) => {
///             println!("Field {}: {}", field.field_number.as_u32(), varint.to_uint64());
///         }
///         FieldValue::Len(data) => {
///             println!("Field {}: {:?}", field.field_number.as_u32(), data);
///         }
///         _ => {}
///     }
/// }
/// ```
pub trait ReadExtProtobuf: Read {
    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Borrows `self` and reads fields from the next `len` bytes (the message).
    ///
    /// # Contract
    ///
    /// `len` must be `<= MAX_MESSAGE_SIZE` (checked with `debug_assert` only).
    /// Returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Length-delimited payloads are `Vec<u8>`.
    fn read_protobuf_fields(&mut self, len: u32) -> FieldIter<'_, Self, LenBody<'_, Self>>
    where
        Self: Sized;

    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Borrows `self` and reads fields from the next `len` bytes (the message).
    ///
    /// # Contract
    ///
    /// `len` must be `<= MAX_MESSAGE_SIZE` (checked with `debug_assert` only).
    ///
    /// `handler` is called once for each length-delimited field. The Len body
    /// is passed **by value** (`remaining()` at entry is the payload length).
    /// After return, unused body bytes are discarded. Custom stateful handlers
    /// can wrap as `|n, body| my_handler.handle(n, body)`.
    ///
    /// Nested messages should call [`LenBody::fields`] /
    /// [`LenBody::fields_with`] on that body (consumes it; keeps `R` fixed).
    /// For `bytes` / `string`, use [`LenBody::into_read`] and [`Read`] helpers.
    ///
    /// # Examples
    ///
    /// Copy a Len body into a `Box<[u8]>`:
    ///
    /// ```
    /// use ::protobuf_core::{FieldValue, ReadExtProtobuf};
    /// use ::std::io::Read;
    ///
    /// let mut data = &[0x0A, 0x03, b'H', b'e', b'l'][..];
    /// let len = data.len() as u32;
    /// let fields: Vec<_> = data
    ///     .read_protobuf_fields_with(len, |_n, body| {
    ///         let body_len = body.remaining();
    ///         let mut buf = vec![0u8; body_len as usize];
    ///         body.into_read().read_exact(&mut buf)?;
    ///         Ok(buf.into_boxed_slice())
    ///     })
    ///     .collect::<Result<Vec<_>, _>>()
    ///     .unwrap();
    /// let FieldValue::Len(data) = &fields[0].value else {
    ///     panic!("expected a length-delimited field");
    /// };
    /// assert_eq!(&data[..], b"Hel");
    /// ```
    fn read_protobuf_fields_with<L, F>(
        &mut self,
        len: u32,
        handler: F,
    ) -> FieldIter<'_, Self, LenBody<'_, Self>, F>
    where
        Self: Sized,
        F: for<'t> FnMut(FieldNumber, LenBody<'t, Self>) -> Result<L>;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_fields(&mut self, len: u32) -> FieldIter<'_, Self, LenBody<'_, Self>>
    where
        Self: Sized,
    {
        debug_assert!(
            (len as usize) <= MAX_MESSAGE_SIZE,
            "message budget exceeds MAX_MESSAGE_SIZE"
        );
        FieldIter {
            body: LenBody::new(self, len),
            handler: VecLenPayload,
            failed: false,
            _marker: ::std::marker::PhantomData,
        }
    }

    fn read_protobuf_fields_with<L, F>(
        &mut self,
        len: u32,
        handler: F,
    ) -> FieldIter<'_, Self, LenBody<'_, Self>, F>
    where
        Self: Sized,
        F: for<'t> FnMut(FieldNumber, LenBody<'t, Self>) -> Result<L>,
    {
        debug_assert!(
            (len as usize) <= MAX_MESSAGE_SIZE,
            "message budget exceeds MAX_MESSAGE_SIZE"
        );
        FieldIter {
            body: LenBody::new(self, len),
            handler,
            failed: false,
            _marker: ::std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::field::FieldValue;
    use ::std::io::Read;

    #[test]
    fn unread_len_body_is_discarded_before_next_field() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let message = [0x0A, 0x03, b'H', b'e', b'l', 0x10, 0x01];
        let mut reader = Cursor::new(message);
        {
            let mut fields = reader.read_protobuf_fields_with(
                message.len() as u32,
                |_n, body| -> crate::Result<u8> {
                    let mut one = [0u8; 1];
                    body.into_read().read_exact(&mut one)?;
                    Ok(one[0])
                },
            );
            let first = fields.next().unwrap().unwrap();
            assert!(matches!(first.value, FieldValue::Len(b'H')));
            let second = fields.next().unwrap().unwrap();
            let FieldValue::Varint(v) = second.value else {
                panic!("expected varint");
            };
            assert_eq!(v.to_uint64(), 1);
        }
        assert_eq!(reader.position(), message.len() as u64);
    }

    #[test]
    fn abandon_after_partial_len_leaves_cursor_at_field_end() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let message = [0x0A, 0x03, b'H', b'e', b'l', 0x10, 0x01];
        let mut reader = Cursor::new(message);
        {
            let mut fields = reader.read_protobuf_fields_with(
                message.len() as u32,
                |_n, body| -> crate::Result<u8> {
                    let mut one = [0u8; 1];
                    body.into_read().read_exact(&mut one)?;
                    Ok(one[0])
                },
            );
            let first = fields.next().unwrap().unwrap();
            assert!(matches!(first.value, FieldValue::Len(b'H')));
        }
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, [0x10, 0x01]);
    }

    #[test]
    fn scope_writes_leftover_on_drop() {
        use super::{discard_reader_bytes, LenBody};
        use ::std::io::Cursor;

        let mut data = Cursor::new(vec![1u8, 2, 3, 4, 5]);
        let mut take = LenBody::new(&mut data, 5);
        assert_eq!(take.remaining(), 5);
        let mut left = 0;
        {
            let child = take.scope(3, &mut left);
            assert_eq!(child.remaining(), 3);
            let mut one = [0u8; 1];
            child.into_read().read_exact(&mut one).unwrap();
            assert_eq!(one, [1]);
        }
        // Parent charged 3; Drop reported 2 unread in the child.
        assert_eq!(take.remaining(), 2);
        assert_eq!(left, 2);
        discard_reader_bytes(take.reader, left).unwrap();
        assert_eq!(take.remaining(), 2);
        drop(take);
        assert_eq!(data.position(), 3);
    }

    #[test]
    fn packed_varints_via_varints_iterator() {
        use super::ReadExtProtobuf;
        use crate::FieldValue;
        use ::std::io::Cursor;

        // field 1, Len, packed varints 1, 150, 2
        let message = [0x0A, 0x04, 0x01, 0x96, 0x01, 0x02];
        let fields: Vec<_> = Cursor::new(message)
            .read_protobuf_fields_with(message.len() as u32, |_n, body| {
                body.varints()
                    .map(|v| v.map(|x| x.to_uint64()))
                    .collect::<crate::Result<Vec<_>>>()
            })
            .collect::<crate::Result<_>>()
            .unwrap();
        let FieldValue::Len(vals) = &fields[0].value else {
            panic!("expected len");
        };
        assert_eq!(vals, &[1, 150, 2]);
    }

    #[test]
    fn packed_fixed32s_via_fixed32s_iterator() {
        use super::ReadExtProtobuf;
        use crate::FieldValue;
        use ::std::io::Cursor;

        // field 1, Len, packed fixed32 1, 2
        let message = [
            0x0A, 0x08, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
        ];
        let fields: Vec<_> = Cursor::new(message)
            .read_protobuf_fields_with(message.len() as u32, |_n, body| {
                body.fixed32s()
                    .map(|v| v.map(u32::from_le_bytes))
                    .collect::<crate::Result<Vec<_>>>()
            })
            .collect::<crate::Result<_>>()
            .unwrap();
        let FieldValue::Len(vals) = &fields[0].value else {
            panic!("expected len");
        };
        assert_eq!(vals, &[1, 2]);
    }

    #[test]
    fn packed_fixed64s_via_fixed64s_iterator() {
        use super::ReadExtProtobuf;
        use crate::FieldValue;
        use ::std::io::Cursor;

        // field 1, Len, packed fixed64 1, 2
        let message = [
            0x0A, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        let fields: Vec<_> = Cursor::new(message)
            .read_protobuf_fields_with(message.len() as u32, |_n, body| {
                body.fixed64s()
                    .map(|v| v.map(u64::from_le_bytes))
                    .collect::<crate::Result<Vec<_>>>()
            })
            .collect::<crate::Result<_>>()
            .unwrap();
        let FieldValue::Len(vals) = &fields[0].value else {
            panic!("expected len");
        };
        assert_eq!(vals, &[1, 2]);
    }

    #[test]
    fn packed_fixed32s_truncated_mid_element_is_error() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        // Len claims 5 bytes for fixed32 elements → trailing partial
        let message = [0x0A, 0x05, 0x01, 0x00, 0x00, 0x00, 0x02];
        let err = Cursor::new(message)
            .read_protobuf_fields_with(message.len() as u32, |_n, body| {
                body.fixed32s().collect::<crate::Result<Vec<_>>>()
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap_err();
        assert!(matches!(err, crate::ProtobufError::UnexpectedEof));
    }

    #[test]
    fn test_read_ext_read_varint_field() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![0x08, 0x96, 0x01]; // field 1: 150
        let len = data.len() as u32;
        let mut reader = Cursor::new(data);
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
        let len = data.len() as u32;
        let mut reader = Cursor::new(data);
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
        let len = data.len() as u32;
        let mut reader = Cursor::new(data);
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
        let len = data.len() as u32;
        let mut reader = Cursor::new(data);
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
        let len = data.len() as u32;
        let mut reader = Cursor::new(data);
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
        let mut reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields(0)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 0);
    }

    #[test]
    fn test_read_ext_respects_len_budget() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

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

        // Mid-field truncation with budget == input.len(): body reads map
        // short fixed/Len payloads to UnexpectedEof.
        let fixed_err = Cursor::new([0x0D, 0x01, 0x02, 0x03])
            .read_protobuf_fields(4)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(fixed_err, ProtobufError::UnexpectedEof));

        let len_err = Cursor::new([0x0A, 0x05, b'a', b'b'])
            .read_protobuf_fields(4)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(len_err, ProtobufError::UnexpectedEof));
    }

    #[test]
    fn oversized_budget_truncated_len_is_unexpected_eof() {
        use super::ReadExtProtobuf;
        use crate::ProtobufError;
        use ::std::io::Cursor;

        // Length prefix claims 3; only 1 payload byte on the reader, but budget
        // is large enough to pass the prepaid length check — VecLenPayload must
        // still surface UnexpectedEof (not IoError).
        let err = Cursor::new([0x0A, 0x03, b'A'])
            .read_protobuf_fields(100)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(
            matches!(err, ProtobufError::UnexpectedEof),
            "got {err:?}"
        );
    }

    #[test]
    fn handler_err_preferred_over_discard_err() {
        use super::ReadExtProtobuf;
        use crate::ProtobufError;
        use ::std::io::Cursor;

        // Handler fails without reading; leftover discard then hits EOF. The
        // iterator must return the handler error, not the discard UnexpectedEof.
        let err = Cursor::new([0x0A, 0x03, b'A'])
            .read_protobuf_fields_with(100, |_n, _body| -> crate::Result<Vec<u8>> {
                Err(ProtobufError::InvalidWireType { value: 7 })
            })
            .next()
            .unwrap()
            .unwrap_err();
        assert!(
            matches!(err, ProtobufError::InvalidWireType { value: 7 }),
            "got {err:?}"
        );
    }

    #[test]
    fn field_iter_fuses_after_error() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let mut cursor = Cursor::new([0x0D, 0x01, 0x02, 0x03]);
        let mut fields = cursor.read_protobuf_fields(4);
        assert!(fields.next().unwrap().is_err());
        assert!(fields.next().is_none());
    }
}

#[cfg(test)]
mod len_payload {
    use super::{LenBody, ReadExtProtobuf};
    use crate::varint::Varint;
    use crate::{Field, FieldNumber, FieldValue, ProtobufError};
    use ::std::io::{Cursor, Read};

    const LARGE_LEN: usize = 8200;

    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    fn read_bytes_payload<R: Read + ?Sized>(
        _n: FieldNumber,
        reader: LenBody<'_, R>,
    ) -> crate::Result<Bytes> {
        let len = reader.remaining();
        let mut data = vec![0; len as usize];
        reader.into_read().read_exact(&mut data)?;
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
        let len = owned.len() as u32;
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
            .read_protobuf_fields_with(input.len() as u32, read_bytes_payload)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    fn assert_box(message: &[u8], expected: &[u8]) {
        let from_read = Cursor::new(message.to_vec())
            .read_protobuf_fields_with(message.len() as u32, |_n, r| {
                let body_len = r.remaining();
                let mut buf = vec![0u8; body_len as usize];
                r.into_read().read_exact(&mut buf)?;
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
        let len = message.len() as u32;

        let from_read = Cursor::new(message)
            .read_protobuf_fields_with(len, |_n, r| {
                let body_len = r.remaining();
                let mut data = vec![0; body_len as usize];
                r.into_read().read_exact(&mut data)?;
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
            .read_protobuf_fields_with(mixed.len() as u32, |_n, r| {
                calls += 1;
                read_bytes_payload(_n, r)
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
            .read_protobuf_fields_with(input.len() as u32, |_n, _r| -> crate::Result<Vec<u8>> {
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
            .read_protobuf_fields_with(message.len() as u32, |_n, r| {
                let body_len = r.remaining();
                let n = body_len as usize;
                let mut data = vec![0xAB; n + 4];
                r.into_read().read_exact(&mut data[..n])?;
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
            .read_protobuf_fields_with(message.len() as u32, |_n, r| {
                let mut one = [0u8; 1];
                r.into_read().read_exact(&mut one)?;
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
    fn nested_message_different_and_overlapping_field_numbers() {
        // Parent field 1 = string "Hi" (Len).
        // Parent field 2 = Child message { field 1 = id 7 (Varint), field 3 = name "ok" }.
        // Field number 1 appears on both parent and child with different meanings.
        let wire: &[u8] = &[
            0x0A, 0x02, b'H', b'i', //
            0x12, 0x06, //
            0x08, 0x07, //
            0x1A, 0x02, b'o', b'k',
        ];

        fn decode_len<R: Read + ?Sized>(
            field_number: FieldNumber,
            reader: LenBody<'_, R>,
        ) -> crate::Result<LenValue> {
            let len = reader.remaining();
            match field_number.as_u32() {
                2 => {
                    let mut child = Child::default();
                    let nested = reader
                        .fields_with(|_n, inner| {
                            let inner_len = inner.remaining();
                            let mut bytes = vec![0u8; inner_len as usize];
                            inner.into_read().read_exact(&mut bytes)?;
                            Ok(LenValue::Bytes(bytes))
                        })
                        .collect::<crate::Result<Vec<_>>>()?;
                    for field in nested {
                        match (field.field_number.as_u32(), field.value) {
                            (1, FieldValue::Varint(v)) => child.id = v.to_uint64(),
                            (3, FieldValue::Len(LenValue::Bytes(name))) => child.name = name,
                            _ => {}
                        }
                    }
                    Ok(LenValue::Child(child))
                }
                _ => {
                    let mut bytes = vec![0u8; len as usize];
                    reader.into_read().read_exact(&mut bytes)?;
                    Ok(LenValue::Bytes(bytes))
                }
            }
        }

        let fields = Cursor::new(wire)
            .read_protobuf_fields_with(wire.len() as u32, decode_len)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_number.as_u32(), 1);
        assert!(matches!(
            &fields[0].value,
            FieldValue::Len(LenValue::Bytes(b)) if b == b"Hi"
        ));
        assert_eq!(fields[1].field_number.as_u32(), 2);
        assert!(matches!(
            &fields[1].value,
            FieldValue::Len(LenValue::Child(Child { id: 7, name })) if name == b"ok"
        ));
    }

    #[test]
    fn nested_message_same_field_number_as_child() {
        // Parent field 1 = Child message { field 1 = 42 }. Same number, nested scope.
        let wire: &[u8] = &[0x0A, 0x02, 0x08, 0x2A];

        #[derive(Debug, Default, PartialEq)]
        struct OnlyChild {
            n: u64,
        }

        let fields = Cursor::new(wire)
            .read_protobuf_fields_with(wire.len() as u32, |_n, reader| {
                let mut child = OnlyChild::default();
                for field in reader.fields() {
                    let field = field?;
                    if field.field_number.as_u32() == 1 {
                        if let FieldValue::Varint(v) = field.value {
                            child.n = v.to_uint64();
                        }
                    }
                }
                Ok(child)
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_number.as_u32(), 1);
        assert_eq!(fields[0].value, FieldValue::Len(OnlyChild { n: 42 }));
    }

    /// Dummy self-recursive message: `Node { value, child: Option<Node> }`.
    ///
    /// Decode walks a fixed root `R` via [`LenBody::fields_with`]: each nested
    /// Len body is already a `LenBody<'_, R>` from the Len handler, so
    /// monomorphization does not stack `&mut` layers.
    #[derive(Debug, Default, PartialEq)]
    struct Node {
        value: u64,
        child: Option<Box<Node>>,
    }

    #[derive(Debug, PartialEq)]
    enum NodeLen {
        Child(Node),
        Skip(Vec<u8>),
    }

    fn parse_node<R: Read + ?Sized>(body: LenBody<'_, R>) -> crate::Result<Node> {
        let mut node = Node::default();
        for field in body.fields_with(|n, nested| {
            let body_len = nested.remaining();
            match n.as_u32() {
                2 => Ok(NodeLen::Child(parse_node(nested)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    nested.into_read().read_exact(&mut buf)?;
                    Ok(NodeLen::Skip(buf))
                }
            }
        }) {
            match field? {
                Field {
                    field_number,
                    value: FieldValue::Varint(v),
                } if field_number.as_u32() == 1 => {
                    node.value = v.to_uint64();
                }
                Field {
                    field_number,
                    value: FieldValue::Len(NodeLen::Child(child)),
                } if field_number.as_u32() == 2 => {
                    node.child = Some(Box::new(child));
                }
                _ => {}
            }
        }
        Ok(node)
    }

    /// Top-level entrance via [`ReadExtProtobuf`]; nested bodies stay on
    /// [`LenBody::fields_with`] / [`parse_node`].
    fn parse_node_from_read<R: Read>(reader: &mut R, len: u32) -> crate::Result<Node> {
        let mut node = Node::default();
        for field in reader.read_protobuf_fields_with(len, |n, body| {
            let body_len = body.remaining();
            match n.as_u32() {
                2 => Ok(NodeLen::Child(parse_node(body)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    body.into_read().read_exact(&mut buf)?;
                    Ok(NodeLen::Skip(buf))
                }
            }
        }) {
            match field? {
                Field {
                    field_number,
                    value: FieldValue::Varint(v),
                } if field_number.as_u32() == 1 => {
                    node.value = v.to_uint64();
                }
                Field {
                    field_number,
                    value: FieldValue::Len(NodeLen::Child(child)),
                } if field_number.as_u32() == 2 => {
                    node.child = Some(Box::new(child));
                }
                _ => {}
            }
        }
        Ok(node)
    }

    #[test]
    fn direct_recursive_node_via_len_body_scope() {
        // Node { value=1, child: Node { value=2, child: Node { value=3 } } }
        // Node3 = 08 03
        // Node2 = 08 02 | 12 02 | 08 03
        // Node1 = 08 01 | 12 06 | (Node2)
        let wire: &[u8] = &[
            0x08, 0x01, //
            0x12, 0x06, //
            0x08, 0x02, //
            0x12, 0x02, //
            0x08, 0x03,
        ];
        let mut input = Cursor::new(wire);
        let got = parse_node_from_read(&mut input, wire.len() as u32).unwrap();
        assert_eq!(
            got,
            Node {
                value: 1,
                child: Some(Box::new(Node {
                    value: 2,
                    child: Some(Box::new(Node {
                        value: 3,
                        child: None,
                    })),
                })),
            }
        );
    }

    /// Indirect recursion: `TypeA` contains `TypeB`, `TypeB` contains `TypeA`.
    /// Same fixed-`R` [`LenBody`] pattern as [`direct_recursive_node_via_len_body_scope`].
    #[derive(Debug, Default, PartialEq)]
    struct TypeA {
        mark: u64,
        b: Option<Box<TypeB>>,
    }

    #[derive(Debug, Default, PartialEq)]
    struct TypeB {
        mark: u64,
        a: Option<Box<TypeA>>,
    }

    #[derive(Debug, PartialEq)]
    enum ALen {
        B(TypeB),
        Skip(Vec<u8>),
    }

    #[derive(Debug, PartialEq)]
    enum BLen {
        A(TypeA),
        Skip(Vec<u8>),
    }

    fn parse_a<R: Read + ?Sized>(body: LenBody<'_, R>) -> crate::Result<TypeA> {
        let mut msg = TypeA::default();
        for field in body.fields_with(|n, nested| {
            let body_len = nested.remaining();
            match n.as_u32() {
                2 => Ok(ALen::B(parse_b(nested)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    nested.into_read().read_exact(&mut buf)?;
                    Ok(ALen::Skip(buf))
                }
            }
        }) {
            match field? {
                Field {
                    field_number,
                    value: FieldValue::Varint(v),
                } if field_number.as_u32() == 1 => {
                    msg.mark = v.to_uint64();
                }
                Field {
                    field_number,
                    value: FieldValue::Len(ALen::B(b)),
                } if field_number.as_u32() == 2 => {
                    msg.b = Some(Box::new(b));
                }
                _ => {}
            }
        }
        Ok(msg)
    }

    fn parse_b<R: Read + ?Sized>(body: LenBody<'_, R>) -> crate::Result<TypeB> {
        let mut msg = TypeB::default();
        for field in body.fields_with(|n, nested| {
            let body_len = nested.remaining();
            match n.as_u32() {
                2 => Ok(BLen::A(parse_a(nested)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    nested.into_read().read_exact(&mut buf)?;
                    Ok(BLen::Skip(buf))
                }
            }
        }) {
            match field? {
                Field {
                    field_number,
                    value: FieldValue::Varint(v),
                } if field_number.as_u32() == 1 => {
                    msg.mark = v.to_uint64();
                }
                Field {
                    field_number,
                    value: FieldValue::Len(BLen::A(a)),
                } if field_number.as_u32() == 2 => {
                    msg.a = Some(Box::new(a));
                }
                _ => {}
            }
        }
        Ok(msg)
    }

    fn parse_a_from_read<R: Read>(reader: &mut R, len: u32) -> crate::Result<TypeA> {
        let mut msg = TypeA::default();
        for field in reader.read_protobuf_fields_with(len, |n, body| {
            let body_len = body.remaining();
            match n.as_u32() {
                2 => Ok(ALen::B(parse_b(body)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    body.into_read().read_exact(&mut buf)?;
                    Ok(ALen::Skip(buf))
                }
            }
        }) {
            match field? {
                Field {
                    field_number,
                    value: FieldValue::Varint(v),
                } if field_number.as_u32() == 1 => {
                    msg.mark = v.to_uint64();
                }
                Field {
                    field_number,
                    value: FieldValue::Len(ALen::B(b)),
                } if field_number.as_u32() == 2 => {
                    msg.b = Some(Box::new(b));
                }
                _ => {}
            }
        }
        Ok(msg)
    }

    #[test]
    fn indirect_recursive_a_b_via_len_body_scope() {
        // A { mark=1, b: B { mark=2, a: A { mark=3 } } }
        // Inner A = 08 03
        // B      = 08 02 | 12 02 | 08 03
        // Outer A= 08 01 | 12 06 | (B)
        // Field 1 = mark on both A and B; field 2 = nested peer type on both.
        let wire: &[u8] = &[
            0x08, 0x01, //
            0x12, 0x06, //
            0x08, 0x02, //
            0x12, 0x02, //
            0x08, 0x03,
        ];
        let mut input = Cursor::new(wire);
        let got = parse_a_from_read(&mut input, wire.len() as u32).unwrap();
        assert_eq!(
            got,
            TypeA {
                mark: 1,
                b: Some(Box::new(TypeB {
                    mark: 2,
                    a: Some(Box::new(TypeA { mark: 3, b: None })),
                })),
            }
        );
    }
}
