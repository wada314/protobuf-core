//! Streaming (`Read`) field iteration bounded by [`Take32`].
//!
//! [`Take32`] is a budgeted view of `&mut R` whose remaining count always fits
//! in a [`u32`]. Protobuf message budgets passed to [`ReadExtProtobuf`] are
//! additionally expected to be `<= MAX_MESSAGE_SIZE` ([`crate::MAX_MESSAGE_SIZE`],
//! 2 GiB); that is `debug_assert`ed at the entrance.
//!
//! Nested Len bodies use [`Take32::scope`] so the handler sees `&mut Take32`
//! for the same root `R` (shorter lifetime) instead of stacking `&mut` layers.

use super::group::GroupBody;
use super::wire::WireCursor;
use crate::field::{Field, FieldValue};
use crate::field_number::FieldNumber;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::{MAX_MESSAGE_SIZE, MAX_VARINT_BYTES, WireType};
use crate::{ProtobufError, Result};
use ::std::io::{Read, Write};

// ============================================================================
// Message budget via Take32
// ============================================================================

/// Budgeted view of `&'a mut R` whose remaining count always fits in a [`u32`].
///
/// Built only by [`ReadExtProtobuf::read_protobuf_fields`] (top-level) or
/// [`scope`](Self::scope) (nested Len / prepaid slice). Nested Len handlers
/// receive `&mut Take32`; after the closure returns, the library
/// [`discard_remaining`](Self::discard_remaining)s so the parent cursor sits at
/// the field boundary. [`fields`](Self::fields) / [`fields_with`](Self::fields_with)
/// borrow this take — they do not construct another `Take32`.
///
/// # Contract
///
/// `remaining()` is always `<= u32::MAX` by construction. Protobuf message
/// budgets also stay `<= MAX_MESSAGE_SIZE` at [`ReadExtProtobuf`] entrances
/// (`debug_assert`).
pub struct Take32<'a, R: ?Sized> {
    reader: &'a mut R,
    remaining: u32,
}

impl<'a, R: Read + ?Sized> Take32<'a, R> {
    /// Crate-internal: top-level take (unused budget stays on the underlying
    /// reader when this value is dropped — like [`std::io::Take`]).
    #[inline]
    pub(crate) fn new(reader: &'a mut R, len: u32) -> Self {
        Self {
            reader,
            remaining: len,
        }
    }

    /// Bytes still allowed under this take's budget.
    #[inline]
    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    /// Read and discard any unused [`remaining`](Self::remaining) bytes.
    #[inline]
    pub fn discard_remaining(&mut self) -> Result<()> {
        let mut buf = [0u8; 256];
        while self.remaining > 0 {
            let chunk = self.remaining.min(buf.len() as u32) as usize;
            Read::read_exact(self.reader, &mut buf[..chunk])?;
            self.remaining -= chunk as u32;
        }
        Ok(())
    }

    /// Scoped body view of exactly `n` bytes (prepaid).
    ///
    /// Charges `n` against this take immediately. The returned child is
    /// `Take32<'b, R>` for the **same** root `R`. Unused child bytes are left
    /// for the caller (or for Len parse's post-handler
    /// [`discard_remaining`](Self::discard_remaining)). Panics if
    /// `n > remaining()`.
    #[inline]
    pub fn scope<'b>(&'b mut self, n: u32) -> Take32<'b, R> {
        assert!(
            n <= self.remaining,
            "Take32::scope length {n} exceeds remaining {}",
            self.remaining
        );
        self.remaining -= n;
        Take32 {
            reader: &mut *self.reader,
            remaining: n,
        }
    }

    /// Iterate fields from this take (borrows `self`; does not create a new
    /// [`Take32`]). Consumes the wire bytes of each field the iterator has
    /// advanced over (including unread Len body bytes discarded after the
    /// handler). Fields never yielded leave their bytes in this take.
    #[inline]
    pub fn fields(&mut self) -> FieldIterMut<'_, 'a, R, VecLenPayload> {
        FieldIterMut {
            take: self,
            handler: VecLenPayload,
        }
    }

    /// Like [`fields`](Self::fields) with a custom Len handler.
    #[inline]
    pub fn fields_with<'s, L, F>(
        &'s mut self,
        handler: F,
    ) -> FieldIterMut<'s, 'a, R, F>
    where
        F: for<'t> FnMut(FieldNumber, &mut Take32<'t, R>) -> Result<L>,
    {
        FieldIterMut {
            take: self,
            handler,
        }
    }
}

impl<'a, R: Read + ?Sized> Read for Take32<'a, R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        let max = buf.len().min(self.remaining as usize);
        if max == 0 {
            return Ok(0);
        }
        let n = self.reader.read(&mut buf[..max])?;
        self.remaining -= n as u32;
        Ok(n)
    }
}

/// Default Len handler for [`ReadExtProtobuf::read_protobuf_fields`]: copy each
/// body into a new `Vec<u8>`.
///
/// Custom handlers are ordinary
/// `FnMut(FieldNumber, &mut Take32<'_, R>) -> Result<L>` closures passed to
/// [`ReadExtProtobuf::read_protobuf_fields_with`] — there is no separate handler
/// trait on the public API. The body take is already scoped to the Len length
/// (`remaining()` at entry). After the closure returns, unused body bytes are
/// discarded by the library before the next field.
#[derive(Debug, Default, Clone, Copy)]
pub struct VecLenPayload;

/// Copy one Len body into a `Vec<u8>` (used by [`VecLenPayload`]).
#[inline]
fn copy_len_to_vec<R: Read + ?Sized>(
    _field_number: FieldNumber,
    reader: &mut Take32<'_, R>,
) -> Result<Vec<u8>> {
    let len = reader.remaining();
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

/// Parse one protobuf field from `read`.
///
/// When the field is length-delimited, `handler` runs once on a
/// [`Take32::scope`] of the payload length (`&mut` body). After the handler
/// returns, unused body bytes are [`Take32::discard_remaining`]ed. Other wire
/// types do not call `handler`.
fn parse_protobuf_field_with<R, L, F>(read: &mut Take32<'_, R>, handler: &mut F) -> Result<Field<L>>
where
    R: Read + ?Sized,
    F: for<'t> FnMut(FieldNumber, &mut Take32<'t, R>) -> Result<L>,
{
    let tag = WireCursor::read_tag(read)?;
    let value = match tag.wire_type {
        WireType::Varint => FieldValue::Varint(WireCursor::read_varint(read)?),
        WireType::Int32 => FieldValue::I32(WireCursor::read_fixed(read)?),
        WireType::Int64 => FieldValue::I64(WireCursor::read_fixed(read)?),
        WireType::Len => {
            let length = WireCursor::read_varint(read)?.try_to_uint32()?;
            if length > read.remaining() {
                return Err(ProtobufError::UnexpectedEof);
            }
            let mut body = read.scope(length);
            let result = handler(tag.field_number, &mut body);
            // Drain unread body bytes so the parent sits at the field boundary.
            body.discard_remaining()?;
            match result {
                Ok(payload) => FieldValue::Len(payload),
                Err(err) => return Err(err),
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

impl<'a, R: Read + ?Sized> WireCursor for Take32<'a, R> {
    #[inline]
    fn remaining(&self) -> u32 {
        self.remaining
    }

    #[inline]
    fn read_varint_with_raw<'b>(
        &'b mut self,
        buf: &'b mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'b [u8])> {
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
    fn skip_bytes(&mut self, mut length: u32) -> Result<()> {
        let mut buf = [0u8; 256];
        while length > 0 {
            let n = length.min(buf.len() as u32) as usize;
            Read::read_exact(self, &mut buf[..n])?;
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
            Read::read_exact(self, &mut buf[..n])?;
            out.write_all(&buf[..n])?;
            left -= n as u32;
        }
        Ok(length as usize)
    }
}
// ============================================================================
// Field iterators
// ============================================================================

/// Iterator for reading raw protobuf fields from an owned [`Take32`].
///
/// Built only by [`ReadExtProtobuf::read_protobuf_fields`] /
/// [`read_protobuf_fields_with`](ReadExtProtobuf::read_protobuf_fields_with).
/// For an existing take, use [`Take32::fields`] / [`Take32::fields_with`]
/// ([`FieldIterMut`]).
pub struct FieldIterFromRead<'a, R: ?Sized, F = VecLenPayload> {
    read: Take32<'a, R>,
    handler: F,
}

impl<'a, R: Read + ?Sized, F> FieldIterFromRead<'a, R, F> {
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
        WireCursor::skip_until(&mut self.read, pred)
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
        WireCursor::copy_until(&mut self.read, pred, out)
    }

    /// Bytes still allowed in this iterator's budget.
    #[inline]
    pub fn remaining(&self) -> u32 {
        self.read.remaining()
    }
}

impl<'a, R: Read + ?Sized> Iterator for FieldIterFromRead<'a, R, VecLenPayload> {
    type Item = Result<Field<Vec<u8>>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.read.remaining() == 0 {
            return None;
        }
        Some(parse_protobuf_field_with(
            &mut self.read,
            &mut copy_len_to_vec,
        ))
    }
}

impl<'a, R, L, F> Iterator for FieldIterFromRead<'a, R, F>
where
    R: Read + ?Sized,
    F: for<'t> FnMut(FieldNumber, &mut Take32<'t, R>) -> Result<L>,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.read.remaining() == 0 {
            return None;
        }
        Some(parse_protobuf_field_with(&mut self.read, &mut self.handler))
    }
}

/// Iterator for reading raw protobuf fields from a borrowed [`Take32`].
///
/// Returned by [`Take32::fields`] / [`Take32::fields_with`]. Does not construct
/// a new `Take32`. Consumes the wire bytes of each field it has advanced over
/// (including unread Len body bytes discarded after the handler); fields never
/// yielded leave their bytes in the borrowed take.
pub struct FieldIterMut<'s, 't, R: ?Sized, F = VecLenPayload> {
    take: &'s mut Take32<'t, R>,
    handler: F,
}

impl<'s, 't, R: Read + ?Sized, F> FieldIterMut<'s, 't, R, F> {
    /// See [`FieldIterFromRead::skip_until`].
    pub fn skip_until<P>(&mut self, pred: P) -> Result<()>
    where
        P: FnMut(Tag) -> Result<bool>,
    {
        WireCursor::skip_until(self.take, pred)
    }

    /// See [`FieldIterFromRead::group_body`].
    #[inline]
    pub fn group_body(&mut self, field_number: FieldNumber) -> GroupBody<&mut Self> {
        GroupBody::new(self, field_number)
    }

    /// See [`FieldIterFromRead::copy_until`].
    pub fn copy_until<P, W>(&mut self, pred: P, out: &mut W) -> Result<usize>
    where
        P: FnMut(Tag) -> Result<bool>,
        W: Write,
    {
        WireCursor::copy_until(self.take, pred, out)
    }

    /// Bytes still allowed in this iterator's budget.
    #[inline]
    pub fn remaining(&self) -> u32 {
        self.take.remaining()
    }
}

impl<'s, 't, R: Read + ?Sized> Iterator for FieldIterMut<'s, 't, R, VecLenPayload> {
    type Item = Result<Field<Vec<u8>>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.take.remaining() == 0 {
            return None;
        }
        Some(parse_protobuf_field_with(self.take, &mut copy_len_to_vec))
    }
}

impl<'s, 't, R, L, F> Iterator for FieldIterMut<'s, 't, R, F>
where
    R: Read + ?Sized,
    F: for<'u> FnMut(FieldNumber, &mut Take32<'u, R>) -> Result<L>,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.take.remaining() == 0 {
            return None;
        }
        Some(parse_protobuf_field_with(self.take, &mut self.handler))
    }
}

// ============================================================================
// ReadExtProtobuf
// ============================================================================

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// Each method takes an explicit message byte budget `len` (`u32`). Length-delimited
/// payloads default to `Vec<u8>` via [`VecLenPayload`].
/// [`read_protobuf_fields_with`](ReadExtProtobuf::read_protobuf_fields_with)
/// supplies a custom
/// `FnMut(FieldNumber, &mut Take32<'_, Self>) -> Result<L>`
/// (field number, scoped body take → payload).
/// It does not provide semantic interpretation - that is the caller's responsibility.
///
/// # Example
/// ```
/// use ::protobuf_core::{ReadExtProtobuf, Field, FieldValue};
///
/// fn main() -> Result<(), Box<dyn ::std::error::Error>> {
///     let data = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
///     let len = data.len() as u32;
///     let mut reader = data;
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
    /// Borrows `self` and consumes at most `len` bytes of message budget.
    ///
    /// # Contract
    ///
    /// `len` must be `<= MAX_MESSAGE_SIZE` (checked with `debug_assert` only).
    /// Returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Length-delimited payloads are `Vec<u8>`.
    fn read_protobuf_fields(&mut self, len: u32) -> FieldIterFromRead<'_, Self>
    where
        Self: Sized;

    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// Borrows `self` and consumes at most `len` bytes of message budget.
    ///
    /// # Contract
    ///
    /// `len` must be `<= MAX_MESSAGE_SIZE` (checked with `debug_assert` only).
    ///
    /// `handler` is called once for each length-delimited field with the field
    /// number and a [`Take32::scope`] of the Len body (`remaining()` at entry is
    /// the payload length). Return [`Ok`](Result::Ok)`(payload)` with any payload
    /// type `L`. After the closure returns, unused body bytes are discarded so
    /// the parent cursor is at the field boundary. Return [`Err`](Result::Err)
    /// to fail the field (body leftover is still discarded first).
    ///
    /// Nested messages should call [`Take32::fields`] /
    /// [`Take32::fields_with`] on that body (keeps `R` fixed). Do
    /// **not** call [`read_protobuf_fields`](Self::read_protobuf_fields) on a
    /// `Take32` itself — that wraps another take and can restack
    /// monomorphizations.
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
    fn read_protobuf_fields_with<L, F>(
        &mut self,
        len: u32,
        handler: F,
    ) -> FieldIterFromRead<'_, Self, F>
    where
        Self: Sized,
        F: for<'t> FnMut(FieldNumber, &mut Take32<'t, Self>) -> Result<L>;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_fields(&mut self, len: u32) -> FieldIterFromRead<'_, Self>
    where
        Self: Sized,
    {
        debug_assert!(
            (len as usize) <= MAX_MESSAGE_SIZE,
            "message budget exceeds MAX_MESSAGE_SIZE"
        );
        FieldIterFromRead {
            read: Take32::new(self, len),
            handler: VecLenPayload,
        }
    }

    fn read_protobuf_fields_with<L, F>(
        &mut self,
        len: u32,
        handler: F,
    ) -> FieldIterFromRead<'_, Self, F>
    where
        Self: Sized,
        F: for<'t> FnMut(FieldNumber, &mut Take32<'t, Self>) -> Result<L>,
    {
        debug_assert!(
            (len as usize) <= MAX_MESSAGE_SIZE,
            "message budget exceeds MAX_MESSAGE_SIZE"
        );
        FieldIterFromRead {
            read: Take32::new(self, len),
            handler,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::field::FieldValue;

    #[test]
    fn unread_len_body_is_discarded_before_next_field() {
        use super::ReadExtProtobuf;
        use ::std::io::{Cursor, Read};

        let message = [0x0A, 0x03, b'H', b'e', b'l', 0x10, 0x01];
        let mut reader = Cursor::new(message);
        {
            let mut fields = reader.read_protobuf_fields_with(
                message.len() as u32,
                |_n, body| -> crate::Result<u8> {
                    let mut one = [0u8; 1];
                    body.read_exact(&mut one)?;
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
        use ::std::io::{Cursor, Read};

        let message = [0x0A, 0x03, b'H', b'e', b'l', 0x10, 0x01];
        let mut reader = Cursor::new(message);
        {
            let mut fields = reader.read_protobuf_fields_with(
                message.len() as u32,
                |_n, body| -> crate::Result<u8> {
                    let mut one = [0u8; 1];
                    body.read_exact(&mut one)?;
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
    fn scope_charges_parent_budget_without_restore() {
        use super::Take32;
        use ::std::io::Cursor;

        let mut data = Cursor::new(vec![1u8, 2, 3, 4, 5]);
        let mut take = Take32::new(&mut data, 5);
        assert_eq!(take.remaining(), 5);
        {
            let child = take.scope(3);
            assert_eq!(child.remaining(), 3);
        }
        // Prepaid length stays charged on the parent (Len path discards the
        // child before drop; abandoning a raw scope leaves the charge).
        assert_eq!(take.remaining(), 2);
        assert_eq!(data.position(), 0);
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
        // the remaining Take32 limit fails as UnexpectedEof.
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
    use super::{ReadExtProtobuf, Take32};
    use crate::varint::Varint;
    use crate::{Field, FieldNumber, FieldValue, ProtobufError};
    use ::std::io::{Cursor, Read};

    const LARGE_LEN: usize = 8200;

    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    fn read_bytes_payload<R: Read + ?Sized>(
        _n: FieldNumber,
        reader: &mut Take32<'_, R>,
    ) -> crate::Result<Bytes> {
        let len = reader.remaining();
        let mut data = vec![0; len as usize];
        reader.read_exact(&mut data)?;
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
                r.read_exact(&mut buf)?;
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
                r.read_exact(&mut data)?;
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
                r.read_exact(&mut data[..n])?;
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
                r.read_exact(&mut one)?;
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
            reader: &mut Take32<'_, R>,
        ) -> crate::Result<LenValue> {
            let len = reader.remaining();
            match field_number.as_u32() {
                2 => {
                    let mut child = Child::default();
                    // Nested fields move the body budget into a child iterator.
                    let nested = reader
                        .fields_with(|_n, inner| {
                            let inner_len = inner.remaining();
                            let mut bytes = vec![0u8; inner_len as usize];
                            inner.read_exact(&mut bytes)?;
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
                    reader.read_exact(&mut bytes)?;
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
    /// Decode walks a fixed root `R` via [`Take32::fields_with`]: each nested
    /// Len body is already a `Take32<'_, R>` from [`Take32::scope`], so
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

    fn parse_node<R: Read + ?Sized>(take: &mut Take32<'_, R>) -> crate::Result<Node> {
        let mut node = Node::default();
        for field in take.fields_with(|n, body| {
            let body_len = body.remaining();
            match n.as_u32() {
                2 => Ok(NodeLen::Child(parse_node(body)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    body.read_exact(&mut buf)?;
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
    /// [`Take32::fields_with`] / [`parse_node`].
    fn parse_node_from_read<R: Read>(reader: &mut R, len: u32) -> crate::Result<Node> {
        let mut node = Node::default();
        for field in reader.read_protobuf_fields_with(len, |n, body| {
            let body_len = body.remaining();
            match n.as_u32() {
                2 => Ok(NodeLen::Child(parse_node(body)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    body.read_exact(&mut buf)?;
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
    fn direct_recursive_node_via_take32_scope() {
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
    /// Same fixed-`R` [`Take32`] pattern as [`direct_recursive_node_via_take32_scope`].
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

    fn parse_a<R: Read + ?Sized>(take: &mut Take32<'_, R>) -> crate::Result<TypeA> {
        let mut msg = TypeA::default();
        for field in take.fields_with(|n, body| {
            let body_len = body.remaining();
            match n.as_u32() {
                2 => Ok(ALen::B(parse_b(body)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    body.read_exact(&mut buf)?;
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

    fn parse_b<R: Read + ?Sized>(take: &mut Take32<'_, R>) -> crate::Result<TypeB> {
        let mut msg = TypeB::default();
        for field in take.fields_with(|n, body| {
            let body_len = body.remaining();
            match n.as_u32() {
                2 => Ok(BLen::A(parse_a(body)?)),
                _ => {
                    let mut buf = vec![0u8; body_len as usize];
                    body.read_exact(&mut buf)?;
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
                    body.read_exact(&mut buf)?;
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
    fn indirect_recursive_a_b_via_take32_scope() {
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
