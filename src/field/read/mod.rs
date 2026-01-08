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

//! Field reading utilities for Protocol Buffers
//!
//! This module provides low-level utilities for reading raw protobuf fields.
//! It is organized by the destination type (owned vs borrowed):
//!
//! - `owned`: Returns `Field<Vec<u8>>` - owned data suitable for streaming sources
//! - `ref`: Returns `Field<&'a [u8]>` - borrowed references suitable for slice sources

pub mod owned;
pub mod ref_;

// Re-export for convenience
pub use owned::{
    IteratorExtProtobuf, ProtobufFieldIterator, ProtobufFieldIteratorFromBytes, ReadExtProtobuf,
    TryIteratorExtProtobuf,
};
pub use ref_::{AsRefExtProtobuf, ProtobufFieldSliceIterator};
