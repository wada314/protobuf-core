//! Field reading utilities for Protocol Buffers.
//!
//! This module provides low-level utilities for reading raw protobuf fields from
//! `std::io::Read` (with an explicit `u32` message byte budget) and from byte
//! slices (`AsRef<[u8]>`). Streaming Len handlers receive a scoped
//! [`LenBody<'_, R>`](LenBody) (`len()` is the payload length; leftover discarded after the closure).
//! Slice sources borrow Len values as `&[u8]`.
//!
//! Byte iterators are not supported; wrap bytes as a slice ([`AsRefExtProtobuf`])
//! or as a [`Read`] with a `u32` `len` budget ([`ReadExtProtobuf`]).

mod group;
mod len_body;
mod slice;
pub(crate) mod wire;

/// One step of a field iterator that may already have failed.
pub(crate) enum IterPoll {
    /// No error yet. The caller may read.
    Ready,
    /// Pending error, moved out. The iterator is now fused.
    Error(crate::ProtobufError),
    /// An error was already moved out. `next` yields `None`.
    Fused,
}

/// Move a not-yet-delivered error out, or report that the iterator is fused.
pub(crate) fn poll_iter(error: &mut Option<crate::ProtobufError>, fused: &mut bool) -> IterPoll {
    if *fused {
        return IterPoll::Fused;
    }
    match error.take() {
        Some(err) => {
            *fused = true;
            IterPoll::Error(err)
        }
        None => IterPoll::Ready,
    }
}

/// Remember that `err` is being moved out to the caller.
pub(crate) fn fuse_err(fused: &mut bool, err: crate::ProtobufError) -> crate::ProtobufError {
    *fused = true;
    err
}

pub use self::group::{GroupBody, until_group_ends};
pub use self::len_body::{
    FieldIter, FixedIterFromLenBody, LenBody, LenBodyRead, LenHandler, ReadExtProtobuf,
    VarintIterFromLenBody, VecLenPayload,
};
pub use self::slice::{AsRefExtProtobuf, FieldIterFromSlice};
