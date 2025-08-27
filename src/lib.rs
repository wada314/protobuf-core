//! Protocol Buffers core utility library.
//!
//! This library provides common definitions, constants, enums, and trivial logic
//! for implementing Protocol Buffers in any programming language.

pub mod tag;
pub mod variant;
pub mod wire_format;

// Re-export commonly used items for convenience
pub use tag::{build_tag, parse_tag, read_tag};
pub use variant::{VariantValue, read_variant};
pub use wire_format::{MAX_FIELD_NUMBER, MAX_MESSAGE_SIZE, MIN_FIELD_NUMBER, WireType};
