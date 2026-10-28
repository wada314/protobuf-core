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
