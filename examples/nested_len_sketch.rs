//! Nested Len parsing via [`Limited::reborrow_len`] / [`OnLen`].
//!
//! Run: `cargo run --example nested_len_sketch`

use ::protobuf_core::{FieldValue, Limited, ReadExtProtobuf, Result};
use ::std::io::Cursor;

#[derive(Debug, Default, PartialEq)]
struct Child {
    id: u64,
    name: Vec<u8>,
}

#[derive(Debug, PartialEq)]
enum LenValue {
    Bytes(Vec<u8>),
    Child(Child),
}

fn decode_len<R: ::std::io::Read>(
    field_number: ::protobuf_core::FieldNumber,
    body: &mut Limited<'_, R>,
) -> Result<LenValue> {
    match field_number.as_u32() {
        2 => {
            let mut child = Child::default();
            let mut raw = |inner_n: ::protobuf_core::FieldNumber, inner: &mut Limited<'_, R>| {
                let mut bytes = vec![0u8; inner.remaining()];
                inner.read_exact(&mut bytes)?;
                let _ = inner_n;
                Ok(LenValue::Bytes(bytes))
            };
            for field in body.read_protobuf_fields(&mut raw)? {
                match (field.field_number.as_u32(), field.value) {
                    (1, FieldValue::Varint(v)) => child.id = v.to_uint64(),
                    (3, FieldValue::Len(LenValue::Bytes(name))) => child.name = name,
                    _ => {}
                }
            }
            Ok(LenValue::Child(child))
        }
        _ => {
            let mut bytes = vec![0u8; body.remaining()];
            body.read_exact(&mut bytes)?;
            Ok(LenValue::Bytes(bytes))
        }
    }
}

fn main() -> Result<()> {
    // field 1 string "Hi"; field 2 message { id=7, name="ok" }
    let wire: &[u8] = &[
        0x0A, 0x02, b'H', b'i', //
        0x12, 0x06, //
        0x08, 0x07, //
        0x1A, 0x02, b'o', b'k',
    ];

    let fields: Vec<_> = Cursor::new(wire)
        .read_protobuf_fields_with(wire.len(), decode_len)
        .collect::<Result<Vec<_>>>()?;

    assert_eq!(fields.len(), 2);
    assert!(matches!(
        &fields[0].value,
        FieldValue::Len(LenValue::Bytes(b)) if b == b"Hi"
    ));
    assert!(matches!(
        &fields[1].value,
        FieldValue::Len(LenValue::Child(Child { id: 7, name })) if name == b"ok"
    ));

    println!("nested_len_sketch: ok");
    println!("{fields:#?}");
    Ok(())
}
