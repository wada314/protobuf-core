//! Protocol Buffers descriptor module
//!
//! This module contains the data structures and constants defined in descriptor.proto
//! and plugin.proto, along with basic encoding/decoding capabilities.

use crate::{ProtobufError, Result};
use derive_from::From;
use std::io::{Read, Write};

// Protobuf integer types (wrapped for type safety)
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, From)]
pub struct PbInt32(i32);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, From)]
pub struct PbInt64(i64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, From)]
pub struct PbUInt32(u32);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, From)]
pub struct PbUInt64(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, From)]
pub struct PbSInt32(i32);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, From)]
pub struct PbSInt64(i64);

/// Field labels for protobuf fields (cardinality)
#[derive(Debug, Clone, PartialEq)]
pub enum FieldLabel {
    Optional,
    Repeated,
    Required,
}

/// Value types for protobuf fields
#[derive(Debug, Clone, PartialEq)]
pub enum ValueType {
    Int32,
    Int64,
    UInt32,
    UInt64,
    Bool,
    String,
    Bytes,
    Float,
    Double,
    Enum(String),
    Message(String),
}

/// Default values for protobuf fields
#[derive(Debug, Clone)]
pub enum DefaultValue {
    None,
    Bool(bool),
    Int32(i32),
    Int64(i64),
    UInt32(u32),
    UInt64(u64),
    String(String),
    EmptyVec,
    Float(f32),
    Double(f64),
}

/// Protobuf to Rust Type Mapping
///
/// This mapping defines how protobuf value types correspond to Rust types.
///
/// For proto2, we have three field labels:
/// - optional: Can be unset (None) or set to a value
/// - required: Must be set to a value (never None)  
/// - repeated: Always a Vec<T> (can be empty)
///
/// Base type mapping (value_type => Rust type):
/// Int32    => i32
/// Int64    => i64
/// UInt32   => u32
/// UInt64   => u64
/// SInt32   => i32 (ZigZag encoded)
/// SInt64   => i64 (ZigZag encoded)
/// Bool     => bool
/// String   => String
/// Bytes    => Vec<u8>
/// Float    => f32
/// Double   => f64
/// Enum     => i32 (enum value)
/// Message  => Box<dyn DescriptorMessage>
///
/// For repeated fields, the base type is simply wrapped with Vec<T>:
/// repeated Int32    => Vec<i32>
/// repeated Int64    => Vec<i64>
/// repeated UInt32   => Vec<u32>
/// repeated UInt64   => Vec<u64>
/// repeated SInt32   => Vec<i32>
/// repeated SInt64   => Vec<i64>
/// repeated Bool     => Vec<bool>
/// repeated String   => Vec<String>
/// repeated Bytes    => Vec<Vec<u8>>
/// repeated Float    => Vec<f32>
/// repeated Double   => Vec<f64>
/// repeated Enum     => Vec<i32>
/// repeated Message  => Vec<Box<dyn DescriptorMessage>>
///
/// Note: SInt32 and SInt64 are ZigZag encoded but stored as regular i32/i64 in Rust.
/// The encoding/decoding is handled by the wire format layer.

/// Metadata for a protobuf field
#[derive(Debug, Clone)]
pub struct FieldMetadata {
    pub number: u32,
    pub wire_type: crate::wire_format::WireType,
    pub field_label: FieldLabel,
    pub value_type: ValueType,
    pub default_value: DefaultValue,
    /// Closure to get a field value from a message
    pub getter: fn(&dyn std::any::Any) -> &dyn std::any::Any,
    /// Closure to get a mutable reference to the field value from a message
    pub setter: fn(&mut dyn std::any::Any) -> &mut dyn std::any::Any,
}

/// Metadata for a protobuf message
#[derive(Debug, Clone)]
pub struct MessageMetadata {
    pub fields: Vec<FieldMetadata>,
}

/// Basic trait for descriptor messages that can be encoded/decoded
pub trait DescriptorMessage {
    /// Calculate the encoded size of this message in bytes
    fn encoded_size(&self) -> Result<usize, ProtobufError>;

    /// Encode this message to the given writer
    fn encode(&self, writer: &mut dyn Write) -> Result<usize>;

    /// Decode a message from the given reader
    fn decode(&mut self, reader: &mut dyn Read) -> Result<usize>;
}

// Common implementation for all types that have metadata
impl<T: HasMetadata> DescriptorMessage for T {
    fn encoded_size(&self) -> Result<usize, ProtobufError> {
        let metadata = Self::metadata();
        let mut total_size = 0;

        for field in &metadata.fields {
            // Get the field value using the getter closure
            let field_value = (field.getter)(self as &dyn std::any::Any);

            // Calculate tag size (field number + wire type)
            let tag = (field.number << 3) | field.wire_type as u32;
            let tag_size = varint_size(tag as u64);
            total_size += tag_size;

            // Calculate value size based on field metadata and actual value
            let value_size = calculate_field_size(
                &field.field_label,
                &field.value_type,
                &field.wire_type,
                field_value,
            )?;
            total_size += value_size;
        }

        Ok(total_size)
    }

    fn encode(&self, writer: &mut dyn Write) -> Result<usize> {
        todo!("Implement encode using metadata")
    }

    fn decode(&mut self, reader: &mut dyn Read) -> Result<usize> {
        todo!("Implement decode using metadata")
    }
}

/// Trait for enum-like values that can be encoded/decoded
pub trait DescriptorEnum {
    /// Get the numeric value of this enum
    fn value(&self) -> i32;

    /// Create an enum from a numeric value
    fn from_value(value: i32) -> Self;

    /// Check if this value is known (has a defined constant)
    fn is_known(&self) -> bool;
}

/// Private trait that provides metadata for protobuf messages
pub(crate) trait HasMetadata: 'static {
    fn metadata() -> crate::descriptor::MessageMetadata;
}

/// Calculate the size of a varint value in bytes using our common Variant implementation
fn varint_size(value: u64) -> usize {
    let variant: crate::variant::Variant = crate::PbUInt64::from(value).into();
    variant.varint_size()
}

/// Generic helper function to calculate field size for scalar and repeated fields
fn calculate_scalar_field_size<T: 'static>(
    value: &dyn std::any::Any,
    is_repeated: bool,
    scalar_type_name: &'static str,
    size_calculator: impl Fn(T) -> usize,
) -> Result<usize, ProtobufError> {
    if is_repeated {
        let vec_type_name = format!("Vec<{}>", scalar_type_name);
        let vec = value
            .downcast_ref::<Vec<T>>()
            .ok_or(ProtobufError::FieldTypeDowncastError {
                expected_type: vec_type_name,
            })?;
        Ok(vec.iter().map(|&v| size_calculator(v)).sum())
    } else {
        let &v = value
            .downcast_ref::<T>()
            .ok_or(ProtobufError::FieldTypeDowncastError {
                expected_type: scalar_type_name.to_string(),
            })?;
        Ok(size_calculator(v))
    }
}

/// Calculate the size of a field value based on its metadata and actual value
fn calculate_field_size(
    field_label: &FieldLabel,
    value_type: &ValueType,
    wire_type: &crate::wire_format::WireType,
    value: &dyn std::any::Any,
) -> Result<usize, ProtobufError> {
    use crate::variant::Variant;

    let is_repeated = matches!(field_label, FieldLabel::Repeated);

    match value_type {
        ValueType::Int32 => calculate_scalar_field_size(value, is_repeated, "i32", |v| {
            let variant: Variant = v.into();
            variant.varint_size()
        }),
        ValueType::Int64 => calculate_scalar_field_size(value, is_repeated, "i64", |v| {
            let variant: Variant = v.into();
            variant.varint_size()
        }),
        ValueType::UInt32 => calculate_scalar_field_size(value, is_repeated, "u32", |v| {
            let variant: Variant = v.into();
            variant.varint_size()
        }),
        ValueType::UInt64 => calculate_scalar_field_size(value, is_repeated, "u64", |v| {
            let variant: Variant = v.into();
            variant.varint_size()
        }),
        ValueType::Bool => {
            calculate_scalar_field_size(
                value,
                is_repeated,
                "bool",
                |_| 1, // Each bool is 1 byte in varint encoding
            )
        }
        ValueType::String => {
            if is_repeated {
                let vec = value.downcast_ref::<Vec<String>>().ok_or(
                    ProtobufError::FieldTypeDowncastError {
                        expected_type: "Vec<String>".to_string(),
                    },
                )?;
                Ok(vec
                    .iter()
                    .map(|s| {
                        let str_len = s.len();
                        varint_size(str_len as u64) + str_len
                    })
                    .sum())
            } else {
                let s = value.downcast_ref::<String>().ok_or(
                    ProtobufError::FieldTypeDowncastError {
                        expected_type: "String".to_string(),
                    },
                )?;
                let str_len = s.len();
                Ok(varint_size(str_len as u64) + str_len)
            }
        }
        ValueType::Bytes => {
            if is_repeated {
                let vec = value.downcast_ref::<Vec<Vec<u8>>>().ok_or(
                    ProtobufError::FieldTypeDowncastError {
                        expected_type: "Vec<Vec<u8>>".to_string(),
                    },
                )?;
                Ok(vec
                    .iter()
                    .map(|bytes| {
                        let bytes_len = bytes.len();
                        varint_size(bytes_len as u64) + bytes_len
                    })
                    .sum())
            } else {
                let bytes = value.downcast_ref::<Vec<u8>>().ok_or(
                    ProtobufError::FieldTypeDowncastError {
                        expected_type: "Vec<u8>".to_string(),
                    },
                )?;
                let bytes_len = bytes.len();
                Ok(varint_size(bytes_len as u64) + bytes_len)
            }
        }
        ValueType::Float => {
            calculate_scalar_field_size(
                value,
                is_repeated,
                "f32",
                |_| 4, // Each f32 is 4 bytes
            )
        }
        ValueType::Double => {
            calculate_scalar_field_size(
                value,
                is_repeated,
                "f64",
                |_| 8, // Each f64 is 8 bytes
            )
        }
        ValueType::Enum(_) => calculate_scalar_field_size(value, is_repeated, "i32", |v| {
            let variant: Variant = v.into();
            variant.varint_size()
        }),
        ValueType::Message(_) => {
            if is_repeated {
                let vec = value
                    .downcast_ref::<Vec<Box<dyn DescriptorMessage>>>()
                    .ok_or(ProtobufError::FieldTypeDowncastError {
                        expected_type: "Vec<Box<dyn DescriptorMessage>>".to_string(),
                    })?;
                Ok(vec
                    .iter()
                    .map(|message| {
                        let message_size = message.encoded_size()?;
                        Ok(varint_size(message_size as u64) + message_size)
                    })
                    .sum::<Result<usize, _>>()?)
            } else {
                let message = value.downcast_ref::<Box<dyn DescriptorMessage>>().ok_or(
                    ProtobufError::FieldTypeDowncastError {
                        expected_type: "Box<dyn DescriptorMessage>".to_string(),
                    },
                )?;
                let message_size = message.encoded_size()?;
                Ok(varint_size(message_size as u64) + message_size)
            }
        }
    }
}

/// Basic encoding utilities for descriptor messages
#[cfg(feature = "descriptor-encode")]
pub mod encode {
    use super::*;

    /// Encode a varint value
    pub fn encode_varint(writer: &mut dyn Write, value: u64) -> Result<()> {
        let mut val = value;
        while val >= 0x80 {
            writer
                .write_all(&[((val & 0x7F) | 0x80) as u8])
                .map_err(|_e| ProtobufError::VariantDowncastOutOfRange {
                    value: 0,
                    target_type: "varint encoding",
                })?;
            val >>= 7;
        }
        writer
            .write_all(&[val as u8])
            .map_err(|_e| ProtobufError::VariantDowncastOutOfRange {
                value: 0,
                target_type: "varint encoding",
            })?;
        Ok(())
    }

    /// Encode a length-delimited field
    pub fn encode_length_delimited(writer: &mut dyn Write, data: &[u8]) -> Result<()> {
        encode_varint(writer, data.len() as u64)?;
        writer
            .write_all(data)
            .map_err(|_e| ProtobufError::VariantDowncastOutOfRange {
                value: 0,
                target_type: "length-delimited encoding",
            })?;
        Ok(())
    }

    /// Encode a protobuf tag (field number + wire type)
    pub fn encode_tag(writer: &mut dyn Write, field_number: u32, wire_type: u8) -> Result<()> {
        let tag = (field_number << 3) | (wire_type as u32);
        encode_varint(writer, tag as u64)
    }

    /// Encode a string field
    pub fn encode_string(writer: &mut dyn Write, field_number: u32, value: &str) -> Result<()> {
        encode_tag(writer, field_number, 2)?; // Wire type 2 = Length-delimited
        encode_length_delimited(writer, value.as_bytes())
    }

    /// Encode an int32 field
    pub fn encode_int32(writer: &mut dyn Write, field_number: u32, value: i32) -> Result<()> {
        encode_tag(writer, field_number, 0)?; // Wire type 0 = Varint
        encode_varint(writer, value as u64)
    }

    /// Encode a bool field
    pub fn encode_bool(writer: &mut dyn Write, field_number: u32, value: bool) -> Result<()> {
        encode_tag(writer, field_number, 0)?; // Wire type 0 = Varint
        encode_varint(writer, if value { 1 } else { 0 })
    }

    /// Encode an enum field
    pub fn encode_enum(
        writer: &mut dyn Write,
        field_number: u32,
        value: &impl DescriptorEnum,
    ) -> Result<()> {
        encode_tag(writer, field_number, 0)?; // Wire type 0 = Varint
        encode_varint(writer, value.value() as u64)
    }
}

/// Basic decoding utilities for descriptor messages
#[cfg(feature = "descriptor-decode")]
pub mod decode {
    use super::*;

    /// Decode a varint value
    pub fn decode_varint(reader: &mut dyn Read) -> Result<u64> {
        let mut result = 0u64;
        let mut shift = 0u32;

        loop {
            let mut byte = [0u8; 1];
            reader.read_exact(&mut byte).map_err(|_e| {
                ProtobufError::VariantDowncastOutOfRange {
                    value: 0,
                    target_type: "varint decoding",
                }
            })?;
            let b = byte[0];

            result |= ((b & 0x7F) as u64) << shift;
            if (b & 0x80) == 0 {
                break;
            }
            shift += 7;
            if shift >= 64 {
                return Err(ProtobufError::VariantDowncastOutOfRange {
                    value: result,
                    target_type: "varint (too long)",
                });
            }
        }

        Ok(result)
    }

    /// Decode a length-delimited field
    pub fn decode_length_delimited(reader: &mut dyn Read) -> Result<Vec<u8>> {
        let length = decode_varint(reader)? as usize;
        let mut data = vec![0u8; length];
        reader
            .read_exact(&mut data)
            .map_err(|_e| ProtobufError::VariantDowncastOutOfRange {
                value: 0,
                target_type: "length-delimited decoding",
            })?;
        Ok(data)
    }

    /// Decode a protobuf tag
    pub fn decode_tag(reader: &mut dyn Read) -> Result<(u32, u8)> {
        let tag = decode_varint(reader)?;
        let field_number = (tag >> 3) as u32;
        let wire_type = (tag & 0x07) as u8;
        Ok((field_number, wire_type))
    }

    /// Decode a string field
    pub fn decode_string(reader: &mut dyn Read) -> Result<String> {
        let data = decode_length_delimited(reader)?;
        String::from_utf8(data).map_err(|_| ProtobufError::VariantDowncastOutOfRange {
            value: 0,
            target_type: "string (invalid UTF-8)",
        })
    }

    /// Decode an int32 field
    pub fn decode_int32(reader: &mut dyn Read) -> Result<i32> {
        let value = decode_varint(reader)?;
        if value > i32::MAX as u64 {
            return Err(ProtobufError::VariantDowncastOutOfRange {
                value,
                target_type: "i32",
            });
        }
        Ok(value as i32)
    }

    /// Decode a bool field
    pub fn decode_bool(reader: &mut dyn Read) -> Result<bool> {
        let value = decode_varint(reader)?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ProtobufError::VariantDowncastOutOfRange {
                value,
                target_type: "bool",
            }),
        }
    }
}

// Helper macros for field metadata generation
#[macro_export]
macro_rules! get_wire_type {
    (String) => {
        crate::wire_format::WireType::Len
    };
    (bool) => {
        crate::wire_format::WireType::Varint
    };
    (u64) => {
        crate::wire_format::WireType::Varint
    };
    (i64) => {
        crate::wire_format::WireType::Varint
    };
    (f64) => {
        crate::wire_format::WireType::I64
    };
    (Vec<u8>) => {
        crate::wire_format::WireType::Len
    };
    (Vec<NamePart>) => {
        crate::wire_format::WireType::Len
    };
    ($t:ty) => {
        compile_error!(concat!("Unsupported type: ", stringify!($t)))
    };
}

#[macro_export]
macro_rules! get_value_type {
    (String) => {
        crate::descriptor::ValueType::String
    };
    (bool) => {
        crate::descriptor::ValueType::Bool
    };
    (u64) => {
        crate::descriptor::ValueType::UInt64
    };
    (i64) => {
        crate::descriptor::ValueType::Int64
    };
    (f64) => {
        crate::descriptor::ValueType::Double
    };
    (Vec<u8>) => {
        crate::descriptor::ValueType::Bytes
    };
    (Vec<NamePart>) => {
        crate::descriptor::ValueType::Message("NamePart".to_string())
    };
    ($t:ty) => {
        compile_error!(concat!("Unsupported type: ", stringify!($t)))
    };
}

#[macro_export]
macro_rules! get_default_value {
    // Repeated fields
    (repeated, $t:ty, _) => {
        crate::descriptor::DefaultValue::EmptyVec
    };

    // Optional fields (no explicit default value)
    (optional, String, None) => {
        crate::descriptor::DefaultValue::String(String::new())
    };
    (optional, u64, None) => {
        crate::descriptor::DefaultValue::UInt64(0)
    };
    (optional, i64, None) => {
        crate::descriptor::DefaultValue::Int64(0)
    };
    (optional, f64, None) => {
        crate::descriptor::DefaultValue::Double(0.0)
    };
    (optional, bool, None) => {
        crate::descriptor::DefaultValue::Bool(false)
    };
    (optional, Vec<u8>, None) => {
        crate::descriptor::DefaultValue::EmptyVec
    };
    (optional, $t:ty, None) => {
        crate::descriptor::DefaultValue::None
    };

    // Explicit default values
    (_, bool, $val:expr) => {
        crate::descriptor::DefaultValue::Bool($val)
    };
    (_, u64, $val:expr) => {
        crate::descriptor::ValueType::UInt64($val)
    };
    (_, i64, $val:expr) => {
        crate::descriptor::ValueType::Int64($val)
    };
    (_, f64, $val:expr) => {
        crate::descriptor::DefaultValue::Double($val)
    };
    (_, String, $val:expr) => {
        crate::descriptor::DefaultValue::String($val.to_string())
    };

    // Required fields
    (required, $t:ty, _) => {
        crate::descriptor::DefaultValue::None
    };
    (required, $t:ty, None) => {
        crate::descriptor::DefaultValue::None
    };

    // Other cases
    ($field_type:ident, $t:ty, $default:expr) => {
        compile_error!(concat!(
            "Unsupported combination: ",
            stringify!($field_type),
            ", ",
            stringify!($t)
        ))
    };
}

/// Macro to define protobuf field metadata (flat structure)
#[macro_export]
macro_rules! define_metadata {
    (
        $(
            $field_type:ident $field_name:ident: $rust_type:ty = $field_number:literal;
        )*
    ) => {
        fn metadata() -> crate::descriptor::MessageMetadata {
            crate::descriptor::MessageMetadata {
                fields: vec![
                    $(
                        crate::descriptor::FieldMetadata {
                            number: $field_number,
                                                        wire_type: {
                                let type_str = stringify!($rust_type);
                                match type_str {
                                    // Numeric types (Varint)
                                    "bool" | "u32" | "u64" | "i32" | "i64" => {
                                        crate::wire_format::WireType::Varint
                                    }
                                    // Floating point types (I64)
                                    "f32" | "f64" => {
                                        crate::wire_format::WireType::I64
                                    }
                                    // String and byte types (Len)
                                    "String" | "Vec<u8>" => {
                                        crate::wire_format::WireType::Len
                                    }
                                    // Any Vec type (Len)
                                    vec_type if vec_type.starts_with("Vec<") => {
                                        crate::wire_format::WireType::Len
                                    }
                                    _ => panic!("Unsupported type: {}", type_str)
                                }
                            },
                            field_label: match stringify!($field_type) {
                                "required" => crate::descriptor::FieldLabel::Required,
                                "optional" => crate::descriptor::FieldLabel::Optional,
                                "repeated" => crate::descriptor::FieldLabel::Repeated,
                                _ => panic!("Unknown field label: {}", stringify!($field_type)),
                            },
                                                        value_type: {
                                let type_str = stringify!($rust_type);
                                match type_str {
                                    // Primitive types
                                    "String" => crate::descriptor::ValueType::String,
                                    "bool" => crate::descriptor::ValueType::Bool,
                                    "u32" => crate::descriptor::ValueType::UInt32,
                                    "u64" => crate::descriptor::ValueType::UInt64,
                                    "i32" => crate::descriptor::ValueType::Int32,
                                    "i64" => crate::descriptor::ValueType::Int64,
                                    "f32" => crate::descriptor::ValueType::Float,
                                    "f64" => crate::descriptor::ValueType::Double,
                                    // Byte array
                                    "Vec<u8>" => crate::descriptor::ValueType::Bytes,
                                    // Any Vec type (treated as message arrays)
                                    vec_type if vec_type.starts_with("Vec<") => {
                                        // Extract the message type name from Vec<MessageType>
                                        let message_type = &vec_type[4..vec_type.len()-1];
                                        crate::descriptor::ValueType::Message(message_type.to_string())
                                    }
                                    _ => panic!("Unsupported type: {}", type_str)
                                }
                            },
                            default_value: match stringify!($field_type) {
                                "repeated" => crate::descriptor::DefaultValue::EmptyVec,
                                "optional" => match stringify!($rust_type) {
                                    "String" => crate::descriptor::DefaultValue::String(String::new()),
                                    "u64" => crate::descriptor::DefaultValue::UInt64(0),
                                    "i64" => crate::descriptor::DefaultValue::Int64(0),
                                    "f64" => crate::descriptor::DefaultValue::Double(0.0),
                                    "bool" => crate::descriptor::DefaultValue::Bool(false),
                                    "Vec<u8>" => crate::descriptor::DefaultValue::EmptyVec,
                                    _ => crate::descriptor::DefaultValue::None,
                                },
                                "required" => crate::descriptor::DefaultValue::None,
                                _ => panic!("Unknown field type: {}", stringify!($field_type)),
                            },
                                                         getter: |msg: &dyn std::any::Any| {
                                 if let Some(this) = msg.downcast_ref::<Self>() {
                                     // For now, we'll use a simple approach that returns the field value
                                     // This will be improved in the next iteration
                                     panic!("FieldValue getter not yet implemented for {}", stringify!($rust_type))
                                 } else {
                                     panic!("Invalid message type")
                                 }
                             },
                            setter: |msg: &mut dyn std::any::Any| {
                                if let Some(this) = msg.downcast_mut::<Self>() {
                                    &mut this.$field_name as &mut dyn std::any::Any
                                } else {
                                    panic!("Invalid message type")
                                }
                            },
                        }
                    ),*
                ],
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test struct for macro testing
    struct TestMessage {
        field1: String,
        field2: bool,
    }

    impl TestMessage {
        // Test the define_metadata macro
        define_metadata! {
            required field1: String = 1;
            required field2: bool = 2;
        }
    }

    #[test]
    fn test_define_metadata_macro() {
        let metadata = TestMessage::metadata();
        assert_eq!(metadata.fields.len(), 2);

        // Check first field
        let field1 = &metadata.fields[0];
        assert_eq!(field1.number, 1);
        assert_eq!(field1.field_label, FieldLabel::Required);
        assert_eq!(field1.value_type, ValueType::String);

        // Check second field
        let field2 = &metadata.fields[1];
        assert_eq!(field2.number, 2);
        assert_eq!(field2.field_label, FieldLabel::Required);
        assert_eq!(field2.value_type, ValueType::Bool);
    }
}
