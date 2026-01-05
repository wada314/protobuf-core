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

//! Slice advancement utilities for Protocol Buffers.
//!
//! This module provides the `SliceAdvance` trait for types that can advance
//! by consuming bytes, which is useful for parsing operations.

use ::std::convert::AsRef;
use ::std::io::Cursor;

/// Trait for types that can advance by consuming bytes.
///
/// This trait allows types to "progress" forward in a slice or buffer,
/// which is necessary for parsing operations that consume bytes sequentially.
///
/// # Example
/// ```
/// use protobuf_core::SliceAdvance;
///
/// let data = [0x08, 0x96, 0x01];
/// let mut slice = &data[..];
/// slice.advance(1);
/// assert_eq!(slice, &[0x96, 0x01]);
/// ```
pub trait SliceAdvance {
    /// Advance by `consumed` bytes.
    ///
    /// This conceptually advances the position in the slice or buffer
    /// by the given number of bytes.
    fn advance(&mut self, consumed: usize);
}

/// Implementation for `&[u8]` - advances by reassigning the reference.
impl SliceAdvance for &[u8] {
    fn advance(&mut self, consumed: usize) {
        *self = &self[consumed..];
    }
}

/// Implementation for `Cursor<T>` where `T: AsRef<[u8]>` - advances by updating position.
impl<T: AsRef<[u8]>> SliceAdvance for Cursor<T> {
    fn advance(&mut self, consumed: usize) {
        let current_pos = self.position();
        self.set_position(current_pos + consumed as u64);
    }
}

