//! Field reading utilities for Protocol Buffers
//!
//! This module provides low-level utilities for reading raw protobuf fields from
//! `std::io::Read` (with an explicit byte budget) and from byte slices (`AsRef<[u8]>`).
//! Streaming sources hand each length-delimited body to the caller as a
//! [`Limited`] budget (`OnLen` / `read_protobuf_fields_with`). Slice sources
//! borrow Len values as `&[u8]`.
//!
//! Byte iterators are not supported; wrap bytes as a slice ([`AsRefExtProtobuf`])
//! or as a [`Read`] with a `len` budget ([`ReadExtProtobuf`]).

mod group;
mod limited;
mod slice;
pub(crate) mod wire;

pub use self::group::{GroupBody, until_group_ends};
pub use self::limited::{FieldIterFromRead, Limited, OnLen, ReadExtProtobuf, VecLenPayload};
pub use self::slice::{AsRefExtProtobuf, FieldIterFromSlice};
