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

#[cfg(test)]
mod tests {
    use crate::field::FieldValue;

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

        let truncated_i32 = [0x0D, 0x01, 0x02, 0x03][..]
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_i32, Err(ProtobufError::UnexpectedEof)));

        let truncated_i64 = [0x09, 0, 0, 0, 0, 0, 0, 0][..]
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_i64, Err(ProtobufError::UnexpectedEof)));

        let truncated_len = [0x0A, 0x05, b'a', b'b'][..]
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_len, Err(ProtobufError::UnexpectedEof)));

        let length_past_u32 = [0x0A, 0x80, 0x80, 0x80, 0x80, 0x10][..]
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
