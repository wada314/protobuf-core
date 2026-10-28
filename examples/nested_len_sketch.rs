//! Design sketch: nested Len parsing on a shared `Read` without nested `std::io::Take`.
//!
//! The library yields `FieldValue::Len(L)` where `L` is entirely the caller's type.
//! On each Len it only: `reborrow_len`, call the user's closure, then drain any
//! unread body bytes. It does **not** prescribe `Vec<Field<L>>` (or any other
//! finished parse tree) as the nested-message payload.
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
// Library-shaped API: `L` is whatever the closure returns
// ---------------------------------------------------------------------------

/// Read fields until this budget is exhausted.
///
/// `on_len` owns the meaning of every Len body. Typical choices:
/// - copy into `Vec<u8>` / custom buffer
/// - fold into a domain struct while iterating nested fields
/// - recursively call `read_fields` and store `Vec<Field<L>>` (optional; not required)
fn read_fields<R, L, F>(reader: &mut Limited<'_, R>, on_len: &mut F) -> Result<Vec<Field<L>>>
where
    R: Read,
    F: FnMut(FieldNumber, &mut Limited<'_, R>) -> Result<L>,
{
    let mut fields = Vec::new();
    while reader.remaining() > 0 {
        fields.push(read_field(reader, on_len)?);
    }
    Ok(fields)
}

fn read_field<R, L, F>(reader: &mut Limited<'_, R>, on_len: &mut F) -> Result<Field<L>>
where
    R: Read,
    F: FnMut(FieldNumber, &mut Limited<'_, R>) -> Result<L>,
{
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
            let mut body = reader.reborrow_len(length)?;
            let payload = on_len(tag.field_number, &mut body)?;
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
// User demo: custom nested type (not `Vec<Field<_>>`)
// ---------------------------------------------------------------------------

/// Domain value the user puts in `FieldValue::Len`.
///
/// Nested messages are a folded struct, not a finished field vector.
#[derive(Debug, PartialEq)]
enum LenValue {
    Bytes(Vec<u8>),
    /// field 2 in the demo schema: id + name, merged while scanning
    Child(Child),
}

#[derive(Debug, Default, PartialEq)]
struct Child {
    id: u64,
    name: Vec<u8>,
}

/// User closure: schema decides how each Len body becomes `LenValue`.
fn decode_len<R: Read>(field_number: FieldNumber, body: &mut Limited<'_, R>) -> Result<LenValue> {
    match field_number.as_u32() {
        // Nested message: fold fields into `Child` without building `Vec<Field<_>>`.
        2 => {
            let mut child = Child::default();
            while body.remaining() > 0 {
                let field = read_field(body, &mut |inner_n, inner_body| {
                    // Child's own Len fields are just bytes in this demo.
                    let mut bytes = vec![0u8; inner_body.remaining()];
                    inner_body.read_exact_budget(&mut bytes)?;
                    let _ = inner_n;
                    Ok(LenValue::Bytes(bytes))
                })?;
                match (field.field_number.as_u32(), field.value) {
                    (1, FieldValue::Varint(v)) => child.id = v.to_uint64(),
                    (3, FieldValue::Len(LenValue::Bytes(name))) => child.name = name,
                    _ => {}
                }
            }
            Ok(LenValue::Child(child))
        }
        // string / bytes / unknown Len
        _ => {
            let mut bytes = vec![0u8; body.remaining()];
            body.read_exact_budget(&mut bytes)?;
            Ok(LenValue::Bytes(bytes))
        }
    }
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
    let fields = read_fields(&mut root, &mut decode_len)?;

    assert_eq!(root.remaining(), 0);
    assert_eq!(fields.len(), 2);

    assert_eq!(fields[0].field_number.as_u32(), 1);
    assert!(matches!(&fields[0].value, FieldValue::Len(LenValue::Bytes(b)) if b == b"Hi"));

    assert_eq!(fields[1].field_number.as_u32(), 2);
    assert!(matches!(
        &fields[1].value,
        FieldValue::Len(LenValue::Child(Child { id: 7, name })) if name == b"ok"
    ));

    println!("nested_len_sketch: ok");
    println!("{fields:#?}");
    Ok(())
}
