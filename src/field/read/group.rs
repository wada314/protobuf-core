//! Protobuf group skip/preserve predicate and body iterator adapter.

use crate::field::{Field, FieldValue};
use crate::field_number::FieldNumber;
use crate::tag::Tag;
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};

/// Predicate for protobuf **group** skip / preserve: returns `Ok(true)` when
/// the matching end-group tag is reached.
///
/// Pass this to [`FieldIterFromSlice::skip_until`] /
/// [`FieldIterFromRead::skip_until`] / [`FieldIterFromSlice::take_until`] /
/// [`FieldIterFromRead::copy_until`] after the start-group tag for
/// `field_number` has already been consumed. Nested groups are tracked with
/// an explicit field-number stack inside the returned closure.
///
/// This is only the stop condition — see those methods for discard / preserve
/// examples.
pub fn until_group_ends(field_number: FieldNumber) -> impl FnMut(Tag) -> Result<bool> {
    let mut stack = vec![field_number];
    move |tag| match (tag.wire_type, stack.last()) {
        (WireType::SGroup, _) => {
            stack.push(tag.field_number);
            Ok(false)
        }
        (WireType::EGroup, Some(&expected)) if expected == tag.field_number => {
            stack.pop();
            Ok(stack.is_empty())
        }
        (WireType::EGroup, Some(&expected)) => Err(ProtobufError::MismatchedEndGroup {
            expected,
            got: tag.field_number,
        }),
        (WireType::EGroup, None) => {
            // `skip_until` / `take_until` / `copy_until` stop when this pred
            // returns `Ok(true)` (stack empty). Calling it again is misuse.
            unreachable!("until_group_ends predicate called after the tracked group closed")
        }
        _ => Ok(false),
    }
}

/// Field-iterator adapter that stops at the matching end-group tag.
///
/// Returned by [`FieldIterFromSlice::group_body`] /
/// [`FieldIterFromRead::group_body`]. Call after the start-group tag for
/// `field_number` has already been consumed.
///
/// # Termination
///
/// [`Iterator::next`] returns `None` only after the matching outermost
/// [`FieldValue::EGroup`] is seen (that tag is **consumed and not yielded**).
/// If the underlying field iterator returns `None` before that end tag, this
/// adapter yields [`ProtobufError::UnexpectedEof`].
///
/// Like a normal [`Iterator`], **nothing is guaranteed after the first
/// `None`**. A later `next` may pull again from the underlying iterator and
/// yield fields **past** the group, or hit soft EOF and yield
/// [`UnexpectedEof`](ProtobufError::UnexpectedEof) again. If you need sticky
/// `None`, wrap with [`Iterator::fuse`] (for example
/// `fields.group_body(n).fuse()`).
///
/// After a yielded [`Err`], further `next` calls are likewise unspecified
/// (the cursor may already have advanced). Typical `Result`-item consumers
/// stop at the first error (`?`, `collect::<Result<_, _>>()`,
/// `try_for_each`, …); this adapter does not fuse on `Err`.
///
/// # Nesting model (depth only)
///
/// In contrast to [`until_group_ends`], this adapter remembers only the
/// outermost group's field number. Nested [`FieldValue::SGroup`] /
/// [`FieldValue::EGroup`] only adjust an integer **depth**; inner end-group
/// field numbers are not checked against their start tags.
///
/// That means the caller is responsible for validating nested group field
/// numbers if that matters. Apply the same adapter again when an inner
/// [`FieldValue::SGroup`] is seen (nested borrows via
/// [`GroupBody::group_body`]):
///
/// ```
/// use ::protobuf_core::{AsRefExtProtobuf, FieldValue};
///
/// let bytes = [
///     0x0B, // 1 SGroup
///     0x13, // 2 SGroup
///     0x18, 0x01, // 3 varint 1
///     0x14, // 2 EGroup
///     0x0C, // 1 EGroup
/// ];
/// let mut fields = bytes.as_slice().read_protobuf_fields();
/// let start = fields.next().unwrap().unwrap();
/// assert!(matches!(start.value, FieldValue::SGroup));
///
/// let mut outer = fields.group_body(start.field_number);
/// let inner_start = outer.next().unwrap().unwrap();
/// assert!(matches!(inner_start.value, FieldValue::SGroup));
/// assert_eq!(inner_start.field_number.as_u32(), 2);
///
/// {
///     // Nested adapter checks the inner group's end field number.
///     let mut inner = outer.group_body(inner_start.field_number);
///     let v = inner.next().unwrap().unwrap();
///     assert_eq!(v.field_number.as_u32(), 3);
///     assert!(inner.next().is_none()); // consumed inner EGroup
/// }
/// assert!(outer.next().is_none()); // consumed outer EGroup
/// ```
///
/// If you ignore inner `SGroup` values and keep polling the outer adapter, an
/// inner `EGroup` only decreases depth — its field number is not verified. A
/// mismatched end tag is reported only for the tracked (outermost) group
/// when `depth == 0`.
#[derive(Debug)]
pub struct GroupBody<I> {
    inner: I,
    field_number: FieldNumber,
    /// How many nested groups are open *inside* the tracked group.
    depth: usize,
}

impl<I> GroupBody<I> {
    /// Start adapting `inner` after the start-group for `field_number` was consumed.
    #[inline]
    pub(crate) fn new(inner: I, field_number: FieldNumber) -> Self {
        Self {
            inner,
            field_number,
            depth: 0,
        }
    }

    /// Field number of the tracked (outermost) start-group.
    #[inline]
    pub fn field_number(&self) -> FieldNumber {
        self.field_number
    }

    /// Unwrap the underlying field iterator. After a successful full parse the
    /// matching end-group has already been consumed from `inner`.
    #[inline]
    pub fn into_inner(self) -> I {
        self.inner
    }

    /// Borrow the underlying field iterator.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.inner
    }

    /// Mutably borrow the underlying field iterator.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.inner
    }
}

impl<I> GroupBody<I>
where
    I: Iterator,
{
    /// Continue with another [`GroupBody`] over nested fields from this
    /// adapter (same depth rules for the nested field number).
    ///
    /// Typically called after observing [`FieldValue::SGroup`] from `self`.
    #[inline]
    pub fn group_body(&mut self, field_number: FieldNumber) -> GroupBody<&mut Self> {
        GroupBody {
            inner: self,
            field_number,
            depth: 0,
        }
    }
}

impl<I, L> Iterator for GroupBody<I>
where
    I: Iterator<Item = Result<Field<L>>>,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        // Soft EOF before a matching end-group is truncated input. A clean
        // finish returns `None` when the matching EGroup is observed below —
        // that path never sees `inner` return `None`.
        let Some(result) = self.inner.next() else {
            return Some(Err(ProtobufError::UnexpectedEof));
        };
        let Ok(field) = result else {
            return Some(result);
        };
        match field.value {
            FieldValue::SGroup => {
                self.depth += 1;
                Some(Ok(field))
            }
            FieldValue::EGroup if self.depth > 0 => {
                self.depth -= 1;
                Some(Ok(field))
            }
            // Matching end of the tracked group: consume, do not yield.
            FieldValue::EGroup if field.field_number == self.field_number => None,
            FieldValue::EGroup => Some(Err(ProtobufError::MismatchedEndGroup {
                expected: self.field_number,
                got: field.field_number,
            })),
            _ => Some(Ok(field)),
        }
    }
}

#[cfg(test)]
mod group_body_adapter {
    use crate::field_number::FieldNumber;
    use crate::{AsRefExtProtobuf, ReadExtProtobuf};
    use crate::{FieldValue, ProtobufError};
    use ::std::io::Cursor;

    fn slice_fields(bytes: &[u8]) -> crate::FieldIterFromSlice<'_> {
        AsRefExtProtobuf::read_protobuf_fields(bytes)
    }

    fn expect_sgroup(fields: &mut crate::FieldIterFromSlice<'_>) -> FieldNumber {
        let f = fields.next().unwrap().unwrap();
        assert!(matches!(f.value, FieldValue::SGroup));
        f.field_number
    }

    #[test]
    fn empty_group_yields_nothing_then_continues() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        assert!(fields.group_body(n).next().is_none());
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 1);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
    }

    #[test]
    fn group_body_accessors_expose_field_number_and_inner() {
        let bytes = [0x0B, 0x0C, 0x10, 0x02];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let mut body = fields.group_body(n);
        assert_eq!(body.field_number(), n);
        let _ = body.inner();
        let _ = body.inner_mut();
        assert!(body.next().is_none()); // empty group body
        let after = body.into_inner().next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 2);
    }

    #[test]
    fn group_body_propagates_inner_decode_error() {
        // SGroup then truncated fixed32 tag/payload → Err from inner parse.
        let bytes = [0x0B, 0x0D, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let err = fields.group_body(n).next().unwrap().unwrap_err();
        assert!(matches!(err, ProtobufError::UnexpectedEof));
    }

    #[test]
    fn after_none_further_next_may_read_past_group() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let mut body = fields.group_body(n);
        assert!(body.next().is_none());
        // Not fused: another next pulls the field after the group.
        let after = body.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 1);
    }

    #[test]
    fn fuse_keeps_none_after_group_ends() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let mut body = fields.group_body(n).fuse();
        assert!(body.next().is_none());
        assert!(body.next().is_none());
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 1);
    }

    #[test]
    fn yields_body_fields_and_consumes_end() {
        let bytes = [0x0B, 0x10, 0x96, 0x01, 0x0C, 0x18, 0x07];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let body: Vec<_> = fields.group_body(n).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].field_number.as_u32(), 2);
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
    }

    #[test]
    fn nested_adapter_checks_inner_end_number() {
        let bytes = [
            0x0B, // 1 SGroup
            0x13, // 2 SGroup
            0x18, 0x01, // 3 varint 1
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
            0x20, 0x02, // 4 varint 2
        ];
        let mut fields = slice_fields(&bytes);
        let outer_n = expect_sgroup(&mut fields);
        let mut outer = fields.group_body(outer_n);
        let inner_start = outer.next().unwrap().unwrap();
        assert!(matches!(inner_start.value, FieldValue::SGroup));
        {
            let mut inner = outer.group_body(inner_start.field_number);
            let v = inner.next().unwrap().unwrap();
            assert_eq!(v.field_number.as_u32(), 3);
            assert!(inner.next().is_none());
        }
        assert!(outer.next().is_none());
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 4);
    }

    #[test]
    fn depth_only_allows_same_number_nesting_without_inner_adapter() {
        // SGroup(1) SGroup(1) varint EGroup(1) EGroup(1)
        let bytes = [0x0B, 0x0B, 0x10, 0x01, 0x0C, 0x0C];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let body: Vec<_> = fields.group_body(n).collect::<Result<Vec<_>, _>>().unwrap();
        // Inner SGroup + varint + inner EGroup yielded; outer EGroup consumed.
        assert_eq!(body.len(), 3);
        assert!(matches!(body[0].value, FieldValue::SGroup));
        assert!(matches!(body[1].value, FieldValue::Varint(_)));
        assert!(matches!(body[2].value, FieldValue::EGroup));
        assert!(fields.next().is_none());
    }

    #[test]
    fn mismatched_outer_end_is_error() {
        let bytes = [0x0B, 0x14];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let err = fields.group_body(n).next().unwrap().unwrap_err();
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
    fn outer_adapter_does_not_verify_mismatched_inner_end_number() {
        // SGroup(1) SGroup(2) EGroup(1): at depth>0 the EGroup is treated as a
        // nested end (depth--) without checking field number 1 vs 2, and is
        // yielded. The outer matching end never appears → UnexpectedEof.
        let bytes = [0x0B, 0x13, 0x0C];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let mut body = fields.group_body(n);
        assert!(matches!(
            body.next().unwrap().unwrap().value,
            FieldValue::SGroup
        ));
        let eg = body.next().unwrap().unwrap();
        assert!(matches!(eg.value, FieldValue::EGroup));
        assert_eq!(eg.field_number.as_u32(), 1);
        assert!(matches!(
            body.next().unwrap().unwrap_err(),
            ProtobufError::UnexpectedEof
        ));
    }

    #[test]
    fn truncated_group_body_is_unexpected_eof() {
        let bytes = [0x0B, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let err = fields
            .group_body(n)
            .collect::<Result<Vec<_>, _>>()
            .unwrap_err();
        assert!(matches!(err, ProtobufError::UnexpectedEof));
    }

    #[test]
    fn works_on_read_iterator() {
        let bytes = [0x0B, 0x10, 0x01, 0x0C, 0x18, 0x07];
        let len = bytes.len() as u32;
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        let body: Vec<_> = fields
            .group_body(start.field_number)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(body.len(), 1);
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
    }
}

#[cfg(test)]
mod group_preserve {
    use super::until_group_ends;
    use crate::field_number::FieldNumber;
    use crate::{AsRefExtProtobuf, ReadExtProtobuf};
    use crate::{FieldValue, ProtobufError, WriteExtProtobuf};
    use ::std::io::Cursor;

    fn slice_fields(bytes: &[u8]) -> crate::FieldIterFromSlice<'_> {
        AsRefExtProtobuf::read_protobuf_fields(bytes)
    }

    fn expect_sgroup(fields: &mut crate::FieldIterFromSlice<'_>) -> FieldNumber {
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
    fn read_copy_until_preserves_group_body() {
        let bytes = message_with_group_then_varint();
        let len = bytes.len() as u32;
        let mut fields = Cursor::new(bytes.clone()).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x10, 0x96, 0x01, 0x0C]);
        assert_eq!(written, out.len());

        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn read_copy_until_empty_group_body() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let len = bytes.len() as u32;
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x0C]);
        assert_eq!(written, 1);

        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 1);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
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
        let len = bytes.len() as u32;
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x13, 0x12, 0x02, b'x', b'y', 0x14, 0x0C]);
        assert_eq!(written, out.len());
        assert!(fields.next().is_none());
    }

    #[test]
    fn read_copy_until_mismatched_end_group() {
        let bytes = [0x0B, 0x14];
        let len = bytes.len() as u32;
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
    fn read_copy_until_truncated_group_is_unexpected_eof() {
        let bytes = [0x0B, 0x08, 0x01];
        let len = bytes.len() as u32;
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut out = Vec::new();
        let err = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap_err();
        assert!(matches!(err, ProtobufError::UnexpectedEof));
        // Copied the incomplete body's varint field before EOF waiting for EGroup.
        assert_eq!(out, vec![0x08, 0x01]);
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

        let len = group_only.len() as u32;
        let mut read_fields = Cursor::new(group_only.to_vec()).read_protobuf_fields(len);
        let start = read_fields.next().unwrap().unwrap();
        let mut from_read = Vec::new();
        let start_len = from_read.write_protobuf_field(&start).unwrap();
        let body_written = read_fields
            .copy_until(until_group_ends(start.field_number), &mut from_read)
            .unwrap();

        assert_eq!(from_slice, group_only);
        assert_eq!(from_read, from_slice);
        assert_eq!(start_len + body_written, from_read.len());
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
        let len = bytes.len() as u32;
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x12, 0x81, 0x00, b'x', 0x0C]);
        assert_eq!(written, out.len());
    }

    #[test]
    fn read_copy_until_preserves_fixed32_and_fixed64() {
        let bytes = [
            0x0B, // 1 SGroup
            0x15, 0x01, 0x02, 0x03, 0x04, // 2 fixed32
            0x09, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, // 1 fixed64
            0x0C, // 1 EGroup
        ];
        let expected_body = &bytes[1..];
        let len = bytes.len() as u32;
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, expected_body);
        assert_eq!(written, out.len());
        assert!(fields.next().is_none());
    }
}

#[cfg(test)]
mod group_skip {
    use super::super::slice::SliceReader;
    use super::super::wire::WireCursor;
    use super::until_group_ends;
    use crate::field_number::FieldNumber;
    use crate::wire_format::WireType;
    use crate::{AsRefExtProtobuf, ReadExtProtobuf};
    use crate::{FieldValue, ProtobufError};

    fn field(n: u32) -> FieldNumber {
        FieldNumber::try_new(n).unwrap()
    }

    fn slice_fields(bytes: &[u8]) -> crate::FieldIterFromSlice<'_> {
        AsRefExtProtobuf::read_protobuf_fields(bytes)
    }

    /// Take the next field and assert it is an `SGroup`; return its field number.
    fn expect_sgroup(fields: &mut crate::FieldIterFromSlice<'_>) -> FieldNumber {
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
        let remaining = (reader.get_ref().len() - reader.position() as usize) as u32;
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
    fn slice_reader_read_varint_with_raw_returns_input_subslice() {
        use crate::wire_format::MAX_VARINT_BYTES;

        let bytes = [0x96u8, 0x01, 0x08];
        let mut slice = bytes.as_slice();
        let mut cursor = SliceReader { slice: &mut slice };
        let mut buf = [0u8; MAX_VARINT_BYTES];
        let (varint, raw) = cursor.read_varint_with_raw(&mut buf).unwrap();
        assert_eq!(varint.to_uint64(), 150);
        assert_eq!(raw, &[0x96, 0x01]);
        assert!(::std::ptr::eq(raw.as_ptr(), bytes.as_ptr()));
        assert_eq!(slice, &[0x08]);
    }

    #[test]
    fn slice_reader_copy_field_copies_fixed_and_len() {
        // field 2 fixed32, field 3 fixed64, field 4 len "ab"
        let bytes = [
            0x15, 0x01, 0x02, 0x03, 0x04, // fixed32
            0x19, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, // fixed64
            0x22, 0x02, b'a', b'b', // len
        ];
        let mut slice = bytes.as_slice();
        let mut cursor = SliceReader { slice: &mut slice };
        let mut out = Vec::new();
        let (tag, n) = cursor.copy_field(&mut out).unwrap();
        assert_eq!(tag.wire_type, WireType::Int32);
        assert_eq!(n, 5);
        let (tag, n) = cursor.copy_field(&mut out).unwrap();
        assert_eq!(tag.wire_type, WireType::Int64);
        assert_eq!(n, 9);
        let (tag, n) = cursor.copy_field(&mut out).unwrap();
        assert_eq!(tag.wire_type, WireType::Len);
        assert_eq!(n, 4);
        assert_eq!(out, bytes);
        assert!(slice.is_empty());
    }

    #[test]
    fn skip_via_field_iter_from_read() {
        use ::std::io::Cursor;

        let bytes = message_with_group_then_varint();
        let len = bytes.len() as u32;
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
        let len = bytes.len() as u32;
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

        let len = bytes.len() as u32;
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
