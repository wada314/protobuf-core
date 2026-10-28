//! Slice (`AsRef<[u8]>`) field iteration (zero-copy Len payloads).

use super::group::GroupBody;
use super::wire::{BufWireCursor, WireCursor};
use crate::field::Field;
use crate::field_number::FieldNumber;
use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::MAX_VARINT_BYTES;
use crate::{ProtobufError, Result};
use ::std::convert::AsRef;
use ::std::io::Write;

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

    #[inline]
    fn read_varint_with_raw<'a>(
        &'a mut self,
        buf: &'a mut [u8; MAX_VARINT_BYTES],
    ) -> Result<(Varint, &'a [u8])> {
        use crate::varint::ReadExtVarint;
        // Decode via ReadExtVarint (tees into `buf`), then return the same-length
        // span from the input slice so callers see zero-copy wire bytes.
        let head = *self.slice;
        let (varint, teed) = self.slice.read_varint_with_raw(buf)?;
        Ok((varint, &head[..teed.len()]))
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
    fn copy_bytes<W: Write>(&mut self, length: usize, out: &mut W) -> Result<usize> {
        let (payload, remaining) = self
            .slice
            .split_at_checked(length)
            .ok_or(ProtobufError::UnexpectedEof)?;
        out.write_all(payload)?;
        *self.slice = remaining;
        Ok(length)
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
    /// Intended mainly for protobuf **group** handling: after an
    /// [`FieldValue::SGroup`] was consumed, pair with [`until_group_ends`] to
    /// discard the body through the matching end tag. Length-delimited
    /// payloads are discarded without allocating.
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
    /// Intended mainly for protobuf **group** preserve: after an
    /// [`FieldValue::SGroup`] was consumed, pair with [`until_group_ends`] to
    /// borrow the body through the matching end tag (start tag already
    /// consumed). Same stop / nesting semantics as
    /// [`skip_until`](Self::skip_until).
    ///
    /// On error the cursor may already have advanced through some fields; there
    /// is no partial span return.
    ///
    /// ```
    /// use ::protobuf_core::{AsRefExtProtobuf, FieldValue, WriteExtProtobuf, until_group_ends};
    ///
    /// let bytes = [0x0B, 0x10, 0x01, 0x0C, 0x18, 0x07];
    /// let mut fields = bytes.as_slice().read_protobuf_fields();
    /// let start = fields.next().unwrap().unwrap();
    /// assert!(matches!(start.value, FieldValue::SGroup));
    ///
    /// // Start tag is already consumed — write it yourself for an unknown-field blob.
    /// let mut preserved = Vec::new();
    /// preserved.write_protobuf_field(&start).unwrap();
    /// let body = fields.take_until(until_group_ends(start.field_number)).unwrap();
    /// preserved.extend_from_slice(body);
    /// assert_eq!(body, &[0x10, 0x01, 0x0C]);
    /// assert_eq!(preserved, &bytes[..bytes.len() - 2]); // without trailing field 3
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

    /// Iterate fields until the matching end-group for `field_number`.
    ///
    /// Intended for parsing a protobuf **group** body after the start-group
    /// tag was consumed. See [`GroupBody`] for nesting rules (depth only;
    /// nested field numbers are the caller's job).
    #[inline]
    pub fn group_body(&mut self, field_number: FieldNumber) -> GroupBody<&mut Self> {
        GroupBody::new(self, field_number)
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
