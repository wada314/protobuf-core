//! Protocol Buffers streaming parser
//!
//! This module provides a simple streaming parser for Protocol Buffers messages.
//! It uses an event-driven approach where a closure is called for each field.

use crate::tag::Tag;
use crate::varint::Varint;
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};
use std::io::Read;

/// A parsed field value
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Variable-width integers (int32, int64, uint32, uint64, sint32, sint64, bool, enum)
    Varint(Varint),
    /// 32-bit fixed-width values (fixed32, sfixed32, float)
    I32([u8; 4]),
    /// 64-bit fixed-width values (fixed64, sfixed64, double)
    I64([u8; 8]),
    /// Length-delimited values (string, bytes, embedded messages, packed repeated fields)
    Len(Vec<u8>),
}

/// A parsing event
#[derive(Debug, Clone, PartialEq)]
pub enum ParseEvent {
    /// A field was parsed
    Field { tag: Tag, value: FieldValue },
}

/// Parse a protobuf stream and call the event handler for each field
///
/// # Arguments
/// * `reader` - The input stream to parse
/// * `event_handler` - A closure that will be called for each parsed field
///
/// # Example
/// ```
/// use std::io::Cursor;
/// use protobuf_core::parser::{parse_stream, ParseEvent, FieldValue};
///
/// let data = vec![0x08, 0x96, 0x01]; // field 1: 150
/// let reader = Cursor::new(data);
///
/// parse_stream(reader, |event| {
///     match event {
///         ParseEvent::Field { tag, value } => {
///             match value {
///                 FieldValue::Varint(varint) => {
///                     println!("Field {}: {}", tag.field_number.as_u32(), varint.to_uint64());
///                 },
///                 _ => {}
///             }
///         }
///     }
///     Ok(())
/// })?;
/// ```
pub fn parse_stream<R, F>(mut reader: R, mut event_handler: F) -> Result<()>
where
    R: Read,
    F: FnMut(ParseEvent) -> Result<()>,
{
    loop {
        let event = parse_next_event(&mut reader)?;
        match event {
            Some(event) => event_handler(event)?,
            None => break, // EOF
        }
    }
    Ok(())
}

/// Parse a single event from the stream (private)
fn parse_next_event<R>(reader: &mut R) -> Result<Option<ParseEvent>>
where
    R: Read,
{
    // Read tag
    let tag = match read_tag_from_reader(reader)? {
        Some(tag) => tag,
        None => return Ok(None),
    };

    let value = match tag.wire_type {
        WireType::Varint => {
            let varint = read_varint_from_reader(reader)?;
            FieldValue::Varint(varint)
        }
        WireType::I32 => {
            let mut bytes = [0u8; 4];
            reader.read_exact(&mut bytes)?;
            FieldValue::I32(bytes)
        }
        WireType::I64 => {
            let mut bytes = [0u8; 8];
            reader.read_exact(&mut bytes)?;
            FieldValue::I64(bytes)
        }
        WireType::Len => {
            let length = read_varint_from_reader(reader)?.try_to_uint32()? as usize;
            let mut data = vec![0u8; length];
            reader.read_exact(&mut data)?;
            FieldValue::Len(data)
        }
        _ => {
            return Err(ProtobufError::InvalidWireType {
                value: tag.wire_type as u8,
            })
        }
    };

    Ok(Some(ParseEvent::Field { tag, value }))
}

/// Read a tag from the reader (private)
fn read_tag_from_reader<R>(reader: &mut R) -> Result<Option<Tag>>
where
    R: Read,
{
    use crate::tag::read_tag;

    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];

    // Read varint for tag
    loop {
        let n = reader.read(&mut byte)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        buffer.push(byte[0]);
        if (byte[0] & 0x80) == 0 {
            break; // Last byte
        }
    }

    let mut iter = buffer.into_iter();
    read_tag(&mut iter)
}

/// Read a varint from the reader (private)
fn read_varint_from_reader<R>(reader: &mut R) -> Result<Varint>
where
    R: Read,
{
    use crate::varint::ReadExtVarint;
    reader.read_varint()?.ok_or_else(|| {
        ProtobufError::IoError(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "Unexpected EOF while reading varint",
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_varint_field() {
        let data = vec![0x08, 0x96, 0x01]; // field 1: 150
        let reader = Cursor::new(data);

        let mut fields = Vec::new();
        parse_stream(reader, |event| {
            match event {
                ParseEvent::Field { tag, value } => {
                    fields.push((tag.field_number.as_u32(), value));
                }
            }
            Ok(())
        })
        .unwrap();

        assert_eq!(fields.len(), 1);
        let (field_num, value) = &fields[0];
        assert_eq!(*field_num, 1);
        match value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }
    }

    #[test]
    fn test_parse_len_field() {
        let data = vec![0x12, 0x03, 0x48, 0x65, 0x6c]; // field 2: "Hel"
        let reader = Cursor::new(data);

        let mut fields = Vec::new();
        parse_stream(reader, |event| {
            match event {
                ParseEvent::Field { tag, value } => {
                    fields.push((tag.field_number.as_u32(), value));
                }
            }
            Ok(())
        })
        .unwrap();

        assert_eq!(fields.len(), 1);
        let (field_num, value) = &fields[0];
        assert_eq!(*field_num, 2);
        match value {
            FieldValue::Len(data) => {
                assert_eq!(data, b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_parse_i32_field() {
        let data = vec![0x15, 0x00, 0x00, 0x00, 0x00]; // field 2: 0 (i32)
        let reader = Cursor::new(data);

        let mut fields = Vec::new();
        parse_stream(reader, |event| {
            match event {
                ParseEvent::Field { tag, value } => {
                    fields.push((tag.field_number.as_u32(), value));
                }
            }
            Ok(())
        })
        .unwrap();

        assert_eq!(fields.len(), 1);
        let (field_num, value) = &fields[0];
        assert_eq!(*field_num, 2);
        match value {
            FieldValue::I32(bytes) => {
                assert_eq!(*bytes, [0x00, 0x00, 0x00, 0x00]);
            }
            _ => panic!("Expected I32 field"),
        }
    }

    #[test]
    fn test_parse_i64_field() {
        let data = vec![0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // field 3: 0 (i64)
        let reader = Cursor::new(data);

        let mut fields = Vec::new();
        parse_stream(reader, |event| {
            match event {
                ParseEvent::Field { tag, value } => {
                    fields.push((tag.field_number.as_u32(), value));
                }
            }
            Ok(())
        })
        .unwrap();

        assert_eq!(fields.len(), 1);
        let (field_num, value) = &fields[0];
        assert_eq!(*field_num, 3);
        match value {
            FieldValue::I64(bytes) => {
                assert_eq!(*bytes, [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
            }
            _ => panic!("Expected I64 field"),
        }
    }

    #[test]
    fn test_parse_multiple_fields() {
        let data = vec![
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ];
        let reader = Cursor::new(data);

        let mut fields = Vec::new();
        parse_stream(reader, |event| {
            match event {
                ParseEvent::Field { tag, value } => {
                    fields.push((tag.field_number.as_u32(), value));
                }
            }
            Ok(())
        })
        .unwrap();

        assert_eq!(fields.len(), 2);

        // Check first field
        let (field_num, value) = &fields[0];
        assert_eq!(*field_num, 1);
        match value {
            FieldValue::Varint(varint) => {
                assert_eq!(varint.to_uint64(), 150);
            }
            _ => panic!("Expected Varint field"),
        }

        // Check second field
        let (field_num, value) = &fields[1];
        assert_eq!(*field_num, 2);
        match value {
            FieldValue::Len(data) => {
                assert_eq!(data, b"Hel");
            }
            _ => panic!("Expected Len field"),
        }
    }

    #[test]
    fn test_parse_empty_stream() {
        let data = vec![];
        let reader = Cursor::new(data);

        let mut fields = Vec::new();
        parse_stream(reader, |event| {
            match event {
                ParseEvent::Field { tag, value } => {
                    fields.push((tag.field_number.as_u32(), value));
                }
            }
            Ok(())
        })
        .unwrap();

        assert_eq!(fields.len(), 0);
    }
}
