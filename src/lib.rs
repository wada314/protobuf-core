// Copyright 2021 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Protocol Buffers core library
//!
//! This library provides common definitions, constants, enums, and basic logic
//! for implementing Protocol Buffers in Rust.

#[cfg(any(feature = "read", feature = "write"))]
pub mod field;
pub mod field_number;
pub mod slice;
pub mod tag;
pub mod varint;
pub mod wire_format;

#[cfg(feature = "write")]
pub use self::field::WriteExtProtobuf;
#[cfg(feature = "read")]
pub use self::field::read_slice::{ProtobufFieldSliceIterator, SliceExtProtobuf};
#[cfg(any(feature = "read", feature = "write"))]
pub use self::field::{Field, FieldValue};
#[cfg(feature = "read")]
pub use self::field::{
    IteratorExtProtobuf, ProtobufFieldIterator, ProtobufFieldIteratorFromBytes, ReadExtProtobuf,
};
pub use self::field_number::FieldNumber;
pub use self::slice::SliceAdvance;
pub use self::tag::{IteratorExtTag, ReadExtTag, SliceExtTag, Tag};
pub use self::varint::{IteratorExtVarint, ReadExtVarint, SliceExtVarint, Varint, WriteExtVarint};
pub use self::wire_format::{MAX_FIELD_NUMBER, MAX_MESSAGE_SIZE, MIN_FIELD_NUMBER, WireType};

use ::std::convert::Infallible;
use ::thiserror::Error;

/// Unified error type for all protobuf operations
#[derive(Error, Debug)]
pub enum ProtobufError {
    #[error("Field number {value} is out of valid range [1, 536_870_911]")]
    FieldNumberOutOfRange { value: String },

    #[error("Invalid wire type: {value} (must be 0-5)")]
    InvalidWireType { value: u8 },

    #[error("Varint value {value} is out of range for target type: {target_type}")]
    VarintDowncastOutOfRange {
        value: u64,
        target_type: &'static str,
    },

    #[error("Failed to downcast field value to expected type: {expected_type}")]
    FieldTypeDowncastError { expected_type: String },

    #[error("Malformed tag: field_number={field_number}, wire_type={wire_type}")]
    MalformedTag { field_number: u32, wire_type: u8 },

    #[error("Unexpected EOF while parsing field")]
    UnexpectedEof,

    #[error("I/O error: {0}")]
    IoError(#[from] ::std::io::Error),
}

impl From<Infallible> for ProtobufError {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

/// Custom Result type for protobuf operations
pub type Result<T> = ::std::result::Result<T, ProtobufError>;
