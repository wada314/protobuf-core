//! Error a field iterator keeps returning after the first failure.

use crate::wire_format::ensure_message_size;
use crate::{ProtobufError, Result};

/// First failure of a field iterator.
///
/// Empty until a message length is over [`MAX_MESSAGE_SIZE`](crate::MAX_MESSAGE_SIZE)
/// or a read fails. Later reads return this error and do not continue.
pub(crate) struct StickyError {
    error: Option<ProtobufError>,
}

impl StickyError {
    pub(crate) const fn empty() -> Self {
        Self { error: None }
    }

    /// [`ProtobufError::SizeLimitExceeded`] when `len` is above `MAX_MESSAGE_SIZE`.
    pub(crate) fn from_message_len(len: usize) -> Self {
        Self {
            error: ensure_message_size(len).err(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_error(error: ProtobufError) -> Self {
        Self { error: Some(error) }
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.error.is_none()
    }

    /// `Err` with a copy of the stored error, or `Ok(())` when empty.
    pub(crate) fn check(&self) -> Result<()> {
        match &self.error {
            Some(err) => Err(copy_error(err)),
            None => Ok(()),
        }
    }

    /// On `Err`, keep that error if none is stored yet, and return a copy.
    pub(crate) fn capture<T>(&mut self, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(err) => Err(self.record(err)),
        }
    }

    fn record(&mut self, err: ProtobufError) -> ProtobufError {
        if let Some(existing) = &self.error {
            return copy_error(existing);
        }
        let shown = copy_error(&err);
        self.error = Some(err);
        shown
    }
}

/// Owned copy so the iterator can keep the original and still return one.
///
/// [`ProtobufError::IoError`](crate::ProtobufError::IoError) keeps the kind and
/// the display text. Its source chain is not copied.
fn copy_error(err: &ProtobufError) -> ProtobufError {
    match err {
        ProtobufError::FieldNumberOutOfRange { value } => {
            ProtobufError::FieldNumberOutOfRange { value: *value }
        }
        ProtobufError::InvalidWireType { value } => {
            ProtobufError::InvalidWireType { value: *value }
        }
        ProtobufError::MismatchedEndGroup { expected, got } => ProtobufError::MismatchedEndGroup {
            expected: *expected,
            got: *got,
        },
        ProtobufError::VarintDowncastOutOfRange { value, target_type } => {
            ProtobufError::VarintDowncastOutOfRange {
                value: *value,
                target_type,
            }
        }
        ProtobufError::VarintTooLong => ProtobufError::VarintTooLong,
        ProtobufError::UnexpectedEof => ProtobufError::UnexpectedEof,
        ProtobufError::IoError(err) => {
            ProtobufError::IoError(::std::io::Error::new(err.kind(), err.to_string()))
        }
        ProtobufError::SizeLimitExceeded { value, limit } => ProtobufError::SizeLimitExceeded {
            value: *value,
            limit: *limit,
        },
    }
}
