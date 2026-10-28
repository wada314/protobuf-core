//! Design sketch: nested Len parsing on a shared `Read` without nested `std::io::Take`.
//!
//! Goal: write the nested-message parse path first, then see which methods a
//! length-limited reader trait actually needs.
//!
//! Run: `cargo run --example nested_len_sketch`

use ::protobuf_core::{
    Field, FieldNumber, FieldValue, ProtobufError, ReadExtTag, ReadExtVarint, Result, WireType,
};
use ::std::io::{self, Cursor, Read};

// ---------------------------------------------------------------------------
// Limited reader used by the field loop
// ---------------------------------------------------------------------------

/// A length budget over an underlying [`Read`].
///
/// Nesting does **not** wrap another adapter around `R`. A child borrows the
/// same `&mut R` with its own `remaining`. The parent subtracts `len` up front
/// so both budgets stay consistent without a push/pop stack on `R`.
struct Limited<'a, R> {
    inner: &'a mut R,
    remaining: usize,
}

impl<'a, R: Read> Limited<'a, R> {
    fn new(inner: &'a mut R, remaining: usize) -> Self {
        Self { inner, remaining }
    }

    fn remaining(&self) -> usize {
        self.remaining
    }

    /// Commit `len` bytes from this budget and return a child over the same `R`.
    fn reborrow_len(&mut self, len: usize) -> Result<Limited<'_, R>> {
        if len > self.remaining {
            return Err(ProtobufError::UnexpectedEof);
        }
        self.remaining -= len;
        Ok(Limited {
            inner: self.inner,
            remaining: len,
        })
    }

    fn skip(&mut self, len: usize) -> Result<()> {
        if len > self.remaining {
            return Err(ProtobufError::UnexpectedEof);
        }
        let mut left = len;
        let mut buf = [0u8; 128];
        while left > 0 {
            let n = left.min(buf.len());
            self.inner.read_exact(&mut buf[..n])?;
            left -= n;
        }
        self.remaining -= len;
        Ok(())
    }

    fn read_exact_budget(&mut self, buf: &mut [u8]) -> Result<()> {
        if buf.len() > self.remaining {
            return Err(ProtobufError::UnexpectedEof);
        }
        self.inner.read_exact(buf)?;
        self.remaining -= buf.len();
        Ok(())
    }
}

impl<R: Read> Read for Limited<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || buf.is_empty() {
            return Ok(0);
        }
        let want = buf.len().min(self.remaining);
        let n = self.inner.read(&mut buf[..want])?;
        self.remaining -= n;
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// Recursive Len payload (library user's choice)
// ---------------------------------------------------------------------------

/// What the user stores in `FieldValue::Len(...)`.
#[derive(Debug, PartialEq)]
enum Payload {
    /// string / bytes / packed / unknown blob
    Bytes(Vec<u8>),
    /// embedded message: fields whose own Len slots are again `Payload`
    Message(Vec<Field<Payload>>),
}

/// Decide how to consume one Len body.
///
/// Takes the field number (schema choice) and a reader already bounded to the
/// payload length.
fn on_len(field_number: FieldNumber, body: &mut Limited<'_, impl Read>) -> Result<Payload> {
    // Demo schema: field 2 is a nested message; everything else is raw bytes.
    if field_number.as_u32() == 2 {
        Ok(Payload::Message(read_message(body)?))
    } else {
        let mut bytes = vec![0u8; body.remaining()];
        body.read_exact_budget(&mut bytes)?;
        Ok(Payload::Bytes(bytes))
    }
}

// ---------------------------------------------------------------------------
// Nested message parsing (the code we wanted to write first)
// ---------------------------------------------------------------------------

fn read_message<R: Read>(reader: &mut Limited<'_, R>) -> Result<Vec<Field<Payload>>> {
    let mut fields = Vec::new();
    while reader.remaining() > 0 {
        fields.push(read_field(reader)?);
    }
    Ok(fields)
}

fn read_field<R: Read>(reader: &mut Limited<'_, R>) -> Result<Field<Payload>> {
    // Tag / varint helpers take `&mut dyn Read` (or `impl Read`). Soft EOF is
    // the caller's `remaining() == 0` check in `read_message`; mid-field empty
    // is UnexpectedEof from these helpers.
    let tag = reader.read_tag()?;
    let value = match tag.wire_type {
        WireType::Varint => FieldValue::Varint(reader.read_varint()?),
        WireType::Int32 => {
            let mut buf = [0u8; 4];
            reader.read_exact_budget(&mut buf)?;
            FieldValue::I32(buf)
        }
        WireType::Int64 => {
            let mut buf = [0u8; 8];
            reader.read_exact_budget(&mut buf)?;
            FieldValue::I64(buf)
        }
        WireType::Len => {
            let length = reader.read_varint()?.try_to_uint32()? as usize;
            // Nested adaptation: same underlying Read, new local budget.
            let mut body = reader.reborrow_len(length)?;
            let payload = on_len(tag.field_number, &mut body)?;
            // If the user did not consume the whole body, discard the rest so
            // the parent cursor stays aligned.
            let unread = body.remaining();
            if unread > 0 {
                body.skip(unread)?;
            }
            FieldValue::Len(payload)
        }
        WireType::SGroup => FieldValue::SGroup,
        WireType::EGroup => FieldValue::EGroup,
    };
    Ok(Field::new(tag.field_number, value))
}

// ---------------------------------------------------------------------------
// Demo
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    // Outer message:
    //   field 1 (string): "Hi"          → 0A 02 48 69
    //   field 2 (message):              → 12 <len>
    //     field 1 (varint): 7           → 08 07
    //     field 3 (string): "ok"        → 1A 02 6F 6B
    let wire: &[u8] = &[
        0x0A, 0x02, b'H', b'i', //
        0x12, 0x06, // nested message, 6 bytes
        0x08, 0x07, //
        0x1A, 0x02, b'o', b'k',
    ];

    let mut cursor = Cursor::new(wire);
    let mut root = Limited::new(&mut cursor, wire.len());
    let fields = read_message(&mut root)?;

    assert_eq!(root.remaining(), 0);
    assert_eq!(fields.len(), 2);

    assert_eq!(fields[0].field_number.as_u32(), 1);
    assert!(matches!(&fields[0].value, FieldValue::Len(Payload::Bytes(b)) if b == b"Hi"));

    assert_eq!(fields[1].field_number.as_u32(), 2);
    let FieldValue::Len(Payload::Message(nested)) = &fields[1].value else {
        panic!("field 2 should be a nested message");
    };
    assert_eq!(nested.len(), 2);
    assert!(matches!(&nested[0].value, FieldValue::Varint(v) if v.to_uint64() == 7));
    assert!(matches!(&nested[1].value, FieldValue::Len(Payload::Bytes(b)) if b == b"ok"));

    println!("nested_len_sketch: ok");
    println!("{fields:#?}");
    Ok(())
}
