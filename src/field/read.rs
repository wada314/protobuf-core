//! Field reading utilities for Protocol Buffers
//!
//! This module provides low-level utilities for reading raw protobuf fields from
//! `std::io::Read` (with an explicit `u32` message byte budget) and from byte
//! slices (`AsRef<[u8]>`). Streaming Len handlers receive a scoped
//! [`Take32<'_, R>`](Take32) (`remaining()` is the payload length; leftover discarded after the closure).
//! Slice sources borrow Len values as `&[u8]`.
//!
//! Byte iterators are not supported; wrap bytes as a slice ([`AsRefExtProtobuf`])
//! or as a [`Read`] with a `u32` `len` budget ([`ReadExtProtobuf`]).

mod group;
mod limited;
mod slice;
pub(crate) mod wire;

pub use self::group::{GroupBody, until_group_ends};
pub use self::limited::{FieldIterFromRead, FieldIterMut, ReadExtProtobuf, Take32, VecLenPayload};
pub use self::slice::{AsRefExtProtobuf, FieldIterFromSlice};
