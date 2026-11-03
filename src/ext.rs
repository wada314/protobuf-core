//! Extension traits for reading and writing protobuf wire data.
//!
//! Use `use protobuf_core::ext::*;` to bring every I/O extension trait into
//! scope in one import. This module intentionally does **not** re-export short
//! type names such as [`Field`](crate::Field) or [`Tag`](crate::Tag); import
//! those from the crate root when you need them in signatures.
//!
//! The same traits are also re-exported at the crate root for individual
//! imports.
//!
//! # Example
//!
//! ```
//! use protobuf_core::ext::*;
//! use protobuf_core::{Field, FieldNumber, FieldValue};
//!
//! let bytes: &[u8] = &[0x08, 0x96, 0x01]; // field 1: 150
//! let field = bytes.read_protobuf_fields().next().unwrap().unwrap();
//! assert_eq!(field.field_number.as_u32(), 1);
//!
//! let mut out = Vec::new();
//! let field: Field<Vec<u8>> = Field::new(
//!     FieldNumber::try_new(1).unwrap(),
//!     FieldValue::from_uint64(150),
//! );
//! out.write_protobuf_field(&field).unwrap();
//! assert_eq!(out, bytes);
//! ```

pub use crate::field::{AsRefExtProtobuf, ReadExtProtobuf, WriteExtProtobuf};
pub use crate::tag::ReadExtTag;
pub use crate::varint::{ReadExtVarint, WriteExtVarint};
