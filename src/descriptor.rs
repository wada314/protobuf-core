//! Protocol Buffers descriptor module
//!
//! This module contains the data structures and constants defined in descriptor.proto
//! and plugin.proto, along with basic encoding/decoding capabilities.

use crate::{ProtobufError, Result};
use std::io::{Read, Write};

// Protobuf integer types (wrapped for type safety)
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProtobufInt32(i32);

impl ProtobufInt32 {
    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProtobufInt64(i64);

impl ProtobufInt64 {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i64 {
        self.0
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProtobufUInt32(u32);

impl ProtobufUInt32 {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u32 {
        self.0
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProtobufUInt64(u64);

impl ProtobufUInt64 {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProtobufSInt32(i32);

impl ProtobufSInt32 {
    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProtobufSInt64(i64);

impl ProtobufSInt64 {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i64 {
        self.0
    }
}

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

/// Field value types for safe access
pub enum FieldValue<'a> {
    Int32(ProtobufInt32),
    Int64(ProtobufInt64),
    UInt32(ProtobufUInt32),
    UInt64(ProtobufUInt64),
    SInt32(ProtobufSInt32),
    SInt64(ProtobufSInt64),
    Bool(bool),
    String(&'a str),
    Bytes(&'a [u8]),
    Float(f32),
    Double(f64),
    Message(&'a dyn DescriptorMessage),
    MessageVec(&'a [Box<dyn DescriptorMessage>]),
}

/// Metadata for a protobuf field
#[derive(Debug, Clone)]
pub struct FieldMetadata {
    pub number: u32,
    pub wire_type: crate::wire_format::WireType,
    pub field_label: FieldLabel,
    pub value_type: ValueType,
    pub default_value: DefaultValue,
    /// Closure to get a field value from a message
    pub getter: fn(&dyn std::any::Any) -> FieldValue,
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
    fn encode(&self, writer: &mut dyn Write) -> Result<usize>;

    /// Decode a message from the given reader
    fn decode(&mut self, reader: &mut dyn Read) -> Result<usize>;
}

// Common implementation for all types that have metadata
impl<T: HasMetadata> DescriptorMessage for T {
    fn encoded_size(&self) -> usize {
        let metadata = Self::metadata();
        let mut total_size = 0;

        for field in &metadata.fields {
            // Get the field value using the getter closure
            let field_value = (field.getter)(self as &dyn std::any::Any);

            // Calculate tag size (field number + wire type)
            let tag = (field.number << 3) | field.wire_type as u32;
            let tag_size = varint_size(tag as u64);
            total_size += tag_size;

            // Calculate value size based on wire type and actual value
            let value_size = calculate_field_size_from_value(&field_value);
            total_size += value_size;
        }

        total_size
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

/// Trait for protobuf integer types that can be converted to varint and calculate their size
pub trait ToVarintSize {
    /// Calculate the size of this value when encoded as a protobuf varint
    fn to_varint_size(&self) -> usize;

    /// Check if this value is within the valid range for its protobuf type
    fn is_valid_protobuf_value(&self) -> bool;
}

/// Calculate the size of a varint value in bytes
fn varint_size(mut value: u64) -> usize {
    let mut size = 1;
    while value >= 0x80 {
        value >>= 7;
        size += 1;
    }
    size
}

// Implement ToVarintSize for protobuf integer types
impl ToVarintSize for ProtobufInt32 {
    fn to_varint_size(&self) -> usize {
        // Protobuf Int32: no encoding, just varint
        varint_size(self.0 as u64)
    }

    fn is_valid_protobuf_value(&self) -> bool {
        // Protobuf Int32 range: -2^31 to 2^31-1
        self.0 >= i32::MIN && self.0 <= i32::MAX
    }
}

impl ToVarintSize for ProtobufInt64 {
    fn to_varint_size(&self) -> usize {
        // Protobuf Int64: no encoding, just varint
        varint_size(self.0 as u64)
    }

    fn is_valid_protobuf_value(&self) -> bool {
        // Protobuf Int64 range: -2^63 to 2^63-1
        self.0 >= i64::MIN && self.0 <= i64::MAX
    }
}

impl ToVarintSize for ProtobufUInt32 {
    fn to_varint_size(&self) -> usize {
        varint_size(self.0 as u64)
    }

    fn is_valid_protobuf_value(&self) -> bool {
        // Protobuf UInt32 range: 0 to 2^32-1
        self.0 <= u32::MAX
    }
}

impl ToVarintSize for ProtobufUInt64 {
    fn to_varint_size(&self) -> usize {
        varint_size(self.0)
    }

    fn is_valid_protobuf_value(&self) -> bool {
        // Protobuf UInt64 range: 0 to 2^64-1
        self.0 <= u64::MAX
    }
}

impl ToVarintSize for ProtobufSInt32 {
    fn to_varint_size(&self) -> usize {
        // Protobuf SInt32: zigzag encoding
        let val = if self.0 < 0 {
            (-self.0 as u64) * 2 + 1
        } else {
            self.0 as u64
        };
        varint_size(val)
    }

    fn is_valid_protobuf_value(&self) -> bool {
        // Protobuf SInt32 range: -2^31 to 2^31-1
        self.0 >= i32::MIN && self.0 <= i32::MAX
    }
}

impl ToVarintSize for ProtobufSInt64 {
    fn to_varint_size(&self) -> usize {
        // Protobuf SInt64: zigzag encoding
        let val = if self.0 < 0 {
            (-self.0 as u64) * 2 + 1
        } else {
            self.0 as u64
        };
        varint_size(val)
    }

    fn is_valid_protobuf_value(&self) -> bool {
        // Protobuf SInt64 range: -2^63 to 2^63-1
        self.0 >= i64::MIN && self.0 <= i64::MAX
    }
}

impl ToVarintSize for bool {
    fn to_varint_size(&self) -> usize {
        varint_size(if *self { 1 } else { 0 })
    }

    fn is_valid_protobuf_value(&self) -> bool {
        // Protobuf Bool: true or false
        true
    }
}

/// Calculate the size of a field value based on its actual value
fn calculate_field_size_from_value(field_value: &FieldValue) -> usize {
    match field_value {
        FieldValue::Int32(value) => value.to_varint_size(),
        FieldValue::Int64(value) => value.to_varint_size(),
        FieldValue::UInt32(value) => value.to_varint_size(),
        FieldValue::UInt64(value) => value.to_varint_size(),
        FieldValue::SInt32(value) => value.to_varint_size(),
        FieldValue::SInt64(value) => value.to_varint_size(),
        FieldValue::Bool(value) => value.to_varint_size(),
        FieldValue::String(value) => {
            let str_len = value.len();
            varint_size(str_len as u64) + str_len
        }
        FieldValue::Bytes(value) => {
            let bytes_len = value.len();
            varint_size(bytes_len as u64) + bytes_len
        }
        FieldValue::Float(_) => 4,  // Fixed 4 bytes for f32
        FieldValue::Double(_) => 8, // Fixed 8 bytes for f64
        FieldValue::Message(message) => {
            // For message types, we can now call encoded_size() directly
            let message_size = message.encoded_size();
            varint_size(message_size as u64) + message_size
        }
        FieldValue::MessageVec(messages) => {
            // For message arrays, calculate total size
            let mut total_size = 0;
            for message in *messages {
                let message_size = message.encoded_size();
                total_size += varint_size(message_size as u64) + message_size;
            }
            total_size
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
