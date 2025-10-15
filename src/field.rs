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

//! Field-level I/O utilities for Protocol Buffers
//!
//! This module provides primitive utilities for reading and writing raw protobuf fields.
//! These are building blocks for constructing higher-level parsers and serializers,
//! not a complete message parser or serializer.
//!
//! ## Core Types
//!
//! The `Field` and `FieldValue` types are always available when this module is enabled.
//!
//! ## Reading (Deserialization)
//!
//! Available when the `read` feature is enabled.
//!
//! The utilities read fields sequentially from input sources that implement `std::io::Read`,
//! returning raw field values (varint bytes, fixed-width bytes, or length-delimited bytes)
//! without interpretation of the semantic meaning.
//!
//! ## Writing (Serialization)
//!
//! Available when the `write` feature is enabled.
//!
//! The utilities write fields to output targets that implement `std::io::Write`,
//! encoding field numbers, wire types, and values into the protobuf wire format.

pub mod value;

#[cfg(feature = "read")]
pub mod read;

#[cfg(feature = "write")]
pub mod write;

// Re-export core types (always available)
pub use value::{Field, FieldValue};

// Re-export read functionality
#[cfg(feature = "read")]
pub use read::{ProtobufFieldIterator, ReadExtProtobuf};

// Re-export write functionality
#[cfg(feature = "write")]
pub use write::WriteExtProtobuf;
