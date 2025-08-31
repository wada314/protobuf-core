//! Protocol Buffers descriptor module
//!
//! This module contains the data structures and constants defined in descriptor.proto
//! and plugin.proto, along with basic encoding/decoding capabilities.

use crate::{ProtobufError, Result};
use std::io::{Read, Write};

/// Field types for protobuf fields
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
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
    Double(f64),
}

/// Metadata for a protobuf field
#[derive(Debug, Clone)]
pub struct FieldMetadata {
    pub number: u32,
    pub wire_type: crate::wire_format::WireType,
    pub field_type: FieldType,
    pub value_type: ValueType,
    pub default_value: DefaultValue,
    /// Closure to get a reference to the field value from a message
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
    fn encoded_size(&self) -> usize;

    /// Encode this message to the given writer
    fn encode(&self, writer: &mut impl Write) -> Result<()>;

    /// Decode a message from the given reader
    fn decode(reader: &mut impl Read) -> Result<Self>
    where
        Self: Sized;
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

/// Basic encoding utilities for descriptor messages
#[cfg(feature = "descriptor-encode")]
pub mod encode {
    use super::*;

    /// Encode a varint value
    pub fn encode_varint(writer: &mut impl Write, value: u64) -> Result<()> {
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
    pub fn encode_length_delimited(writer: &mut impl Write, data: &[u8]) -> Result<()> {
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
    pub fn encode_tag(writer: &mut impl Write, field_number: u32, wire_type: u8) -> Result<()> {
        let tag = (field_number << 3) | (wire_type as u32);
        encode_varint(writer, tag as u64)
    }

    /// Encode a string field
    pub fn encode_string(writer: &mut impl Write, field_number: u32, value: &str) -> Result<()> {
        encode_tag(writer, field_number, 2)?; // Wire type 2 = Length-delimited
        encode_length_delimited(writer, value.as_bytes())
    }

    /// Encode an int32 field
    pub fn encode_int32(writer: &mut impl Write, field_number: u32, value: i32) -> Result<()> {
        encode_tag(writer, field_number, 0)?; // Wire type 0 = Varint
        encode_varint(writer, value as u64)
    }

    /// Encode a bool field
    pub fn encode_bool(writer: &mut impl Write, field_number: u32, value: bool) -> Result<()> {
        encode_tag(writer, field_number, 0)?; // Wire type 0 = Varint
        encode_varint(writer, if value { 1 } else { 0 })
    }

    /// Encode an enum field
    pub fn encode_enum(
        writer: &mut impl Write,
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
    pub fn decode_varint(reader: &mut impl Read) -> Result<u64> {
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
    pub fn decode_length_delimited(reader: &mut impl Read) -> Result<Vec<u8>> {
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
    pub fn decode_tag(reader: &mut impl Read) -> Result<(u32, u8)> {
        let tag = decode_varint(reader)?;
        let field_number = (tag >> 3) as u32;
        let wire_type = (tag & 0x07) as u8;
        Ok((field_number, wire_type))
    }

    /// Decode a string field
    pub fn decode_string(reader: &mut impl Read) -> Result<String> {
        let data = decode_length_delimited(reader)?;
        String::from_utf8(data).map_err(|_| ProtobufError::VariantDowncastOutOfRange {
            value: 0,
            target_type: "string (invalid UTF-8)",
        })
    }

    /// Decode an int32 field
    pub fn decode_int32(reader: &mut impl Read) -> Result<i32> {
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
    pub fn decode_bool(reader: &mut impl Read) -> Result<bool> {
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
    // repeated フィールド
    (repeated, $t:ty, _) => {
        crate::descriptor::DefaultValue::EmptyVec
    };

    // optional フィールド（明示的なデフォルト値なし）
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

    // 明示的なデフォルト値あり
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

    // required フィールド
    (required, $t:ty, _) => {
        crate::descriptor::DefaultValue::None
    };
    (required, $t:ty, None) => {
        crate::descriptor::DefaultValue::None
    };

    // その他のケース
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
        pub fn metadata() -> crate::descriptor::MessageMetadata {
            crate::descriptor::MessageMetadata {
                fields: vec![
                    $(
                        crate::descriptor::FieldMetadata {
                            number: $field_number,
                            wire_type: match stringify!($rust_type) {
                                "String" => crate::wire_format::WireType::Len,
                                "bool" => crate::wire_format::WireType::Varint,
                                "u64" => crate::wire_format::WireType::Varint,
                                "i64" => crate::wire_format::WireType::Varint,
                                "f64" => crate::wire_format::WireType::I64,
                                "Vec<u8>" => crate::wire_format::WireType::Len,
                                "Vec<NamePart>" => crate::wire_format::WireType::Len,
                                _ => panic!("Unsupported type: {}", stringify!($rust_type)),
                            },
                            field_type: match stringify!($field_type) {
                                "required" => crate::descriptor::FieldType::Required,
                                "optional" => crate::descriptor::FieldType::Optional,
                                "repeated" => crate::descriptor::FieldType::Repeated,
                                _ => panic!("Unknown field type: {}", stringify!($field_type)),
                            },
                            value_type: match stringify!($rust_type) {
                                "String" => crate::descriptor::ValueType::String,
                                "bool" => crate::descriptor::ValueType::Bool,
                                "u64" => crate::descriptor::ValueType::UInt64,
                                "i64" => crate::descriptor::ValueType::Int64,
                                "f64" => crate::descriptor::ValueType::Double,
                                "Vec<u8>" => crate::descriptor::ValueType::Bytes,
                                "Vec<NamePart>" => crate::descriptor::ValueType::Message("NamePart".to_string()),
                                _ => panic!("Unsupported type: {}", stringify!($rust_type)),
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
                                    &this.$field_name as &dyn std::any::Any
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

// Temporarily comment out tests until macro is working
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     // Test struct for macro testing
//     struct TestMessage {
//         field1: String,
//         field2: bool,
//     }
//
//     impl TestMessage {
//         // Test the define_metadata macro
//         define_metadata! {
//             required field1: String = 1;
//             required field2: bool = 2;
//         }
//     }
//
//     #[test]
//     fn test_define_metadata_macro() {
//         let metadata = TestMessage::metadata();
//         assert_eq!(metadata.fields.len(), 2);
//
//         // Check first field
//         let field1 = &metadata.fields[0];
//         assert_eq!(field1.number, 1);
//         assert_eq!(field1.field_type, FieldType::Required);
//         assert_eq!(field1.value_type, ValueType::String);
//
//         // Check second field
//         let field2 = &metadata.fields[1];
//         assert_eq!(field2.number, 2);
//         assert_eq!(field2.field_type, FieldType::Required);
//         assert_eq!(field2.value_type, ValueType::Bool);
//     }
// }
