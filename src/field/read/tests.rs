//! Tests for [`crate::field::read`].
//!
//! Nested submodules import from `super::super` (the `read` module). Do not
//! `use` both `AsRefExtProtobuf` and `ReadExtProtobuf` at this module root —
//! `&[u8]` implements both and `read_protobuf_fields` would be ambiguous.

use crate::field::FieldValue;

// ============================================================================
// AsRefExtProtobuf tests (borrowed references)
// ============================================================================

#[test]
fn test_as_ref_ext_read_varint_field() {
    use super::AsRefExtProtobuf;

    let reader = &[0x08, 0x96, 0x01][..]; // field 1: 150

    let fields: Vec<_> = reader
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_number.as_u32(), 1);
    match &fields[0].value {
        FieldValue::Varint(varint) => {
            assert_eq!(varint.to_uint64(), 150);
        }
        _ => panic!("Expected Varint field"),
    }
}

#[test]
fn test_as_ref_ext_read_len_field() {
    use super::AsRefExtProtobuf;

    let reader = &[0x12, 0x03, 0x48, 0x65, 0x6c][..]; // field 2: "Hel"

    let fields: Vec<_> = reader
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);

    let field = &fields[0];
    assert_eq!(field.field_number.as_u32(), 2);
    match &field.value {
        FieldValue::Len(data) => {
            assert_eq!(&data[..], b"Hel");
        }
        _ => panic!("Expected Len field"),
    }
}

#[test]
fn test_as_ref_ext_read_i32_field() {
    use super::AsRefExtProtobuf;

    let reader = &[0x15, 0x78, 0x56, 0x34, 0x12][..]; // field 2: 0x12345678 (Fixed32)

    let fields: Vec<_> = reader
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);

    let field = &fields[0];
    assert_eq!(field.field_number.as_u32(), 2);
    match &field.value {
        FieldValue::I32(bytes) => {
            assert_eq!(*bytes, [0x78, 0x56, 0x34, 0x12]);
        }
        _ => panic!("Expected I32 field"),
    }
}

#[test]
fn test_as_ref_ext_read_i64_field() {
    use super::AsRefExtProtobuf;

    let reader = &[0x19, 0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12][..]; // field 3: 0x1234567890ABCDEF (Fixed64)

    let fields: Vec<_> = reader
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);

    let field = &fields[0];
    assert_eq!(field.field_number.as_u32(), 3);
    match &field.value {
        FieldValue::I64(bytes) => {
            assert_eq!(*bytes, [0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]);
        }
        _ => panic!("Expected I64 field"),
    }
}

#[test]
fn test_as_ref_ext_read_multiple_fields() {
    use super::AsRefExtProtobuf;

    let reader = &[
        0x08, 0x96, 0x01, // field 1: 150
        0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
    ][..];

    let fields: Vec<_> = reader
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 2);

    // Check first field
    let field = &fields[0];
    assert_eq!(field.field_number.as_u32(), 1);
    match &field.value {
        FieldValue::Varint(varint) => {
            assert_eq!(varint.to_uint64(), 150);
        }
        _ => panic!("Expected Varint field"),
    }

    // Check second field
    let field = &fields[1];
    assert_eq!(field.field_number.as_u32(), 2);
    match &field.value {
        FieldValue::Len(data) => {
            assert_eq!(&data[..], b"Hel");
        }
        _ => panic!("Expected Len field"),
    }
}

#[test]
fn test_as_ref_ext_read_empty_stream() {
    use super::AsRefExtProtobuf;

    let reader = &[][..];

    let fields: Vec<_> = reader
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 0);
}

#[test]
fn test_as_ref_ext_works_with_unsized_type() {
    use super::AsRefExtProtobuf;

    // This test verifies that ?Sized bound allows the trait to work with unsized types
    // Without ?Sized, this would fail to compile because [u8] is unsized
    fn accepts_unsized<T: AsRefExtProtobuf + ?Sized>(x: &T) {
        let _ = x.read_protobuf_fields();
    }

    let data = [0x08, 0x96, 0x01]; // field 1: 150
    accepts_unsized(&data[..]); // &[u8] - unsized type slice

    // Also test with direct unsized slice
    let slice: &[u8] = &[0x08, 0x96, 0x01];
    let fields: Vec<_> = slice
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_number.as_u32(), 1);
}

// ============================================================================
// ReadExtProtobuf tests (owned data from Read)
// ============================================================================

#[test]
fn test_read_ext_read_varint_field() {
    use super::ReadExtProtobuf;
    use ::std::io::Cursor;

    let data = vec![0x08, 0x96, 0x01]; // field 1: 150
    let len = data.len();
    let reader = Cursor::new(data);
    let fields: Vec<_> = reader
        .read_protobuf_fields(len)
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_number.as_u32(), 1);
    match &fields[0].value {
        FieldValue::Varint(varint) => {
            assert_eq!(varint.to_uint64(), 150);
        }
        _ => panic!("Expected Varint field"),
    }
}

#[test]
fn test_read_ext_read_multiple_fields() {
    use super::ReadExtProtobuf;
    use ::std::io::Cursor;

    let data = vec![
        0x08, 0x96, 0x01, // field 1: 150
        0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
    ];
    let len = data.len();
    let reader = Cursor::new(data);
    let fields: Vec<_> = reader
        .read_protobuf_fields(len)
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 2);

    // Check first field
    assert_eq!(fields[0].field_number.as_u32(), 1);
    match &fields[0].value {
        FieldValue::Varint(varint) => {
            assert_eq!(varint.to_uint64(), 150);
        }
        _ => panic!("Expected Varint field"),
    }

    // Check second field
    assert_eq!(fields[1].field_number.as_u32(), 2);
    match &fields[1].value {
        FieldValue::Len(data) => {
            assert_eq!(&data[..], b"Hel");
        }
        _ => panic!("Expected Len field"),
    }
}

#[test]
fn test_read_ext_read_i32_field() {
    use super::ReadExtProtobuf;
    use ::std::io::Cursor;

    let data = vec![0x15, 0x78, 0x56, 0x34, 0x12]; // field 2: 0x12345678 (Fixed32)
    let len = data.len();
    let reader = Cursor::new(data);
    let fields: Vec<_> = reader
        .read_protobuf_fields(len)
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_number.as_u32(), 2);
    match &fields[0].value {
        FieldValue::I32(bytes) => {
            assert_eq!(*bytes, [0x78, 0x56, 0x34, 0x12]);
        }
        _ => panic!("Expected I32 field"),
    }
}

#[test]
fn test_read_ext_read_len_field() {
    use super::ReadExtProtobuf;
    use ::std::io::Cursor;

    let data = vec![0x12, 0x03, 0x48, 0x65, 0x6c]; // field 2: "Hel"
    let len = data.len();
    let reader = Cursor::new(data);
    let fields: Vec<_> = reader
        .read_protobuf_fields(len)
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_number.as_u32(), 2);
    match &fields[0].value {
        FieldValue::Len(data) => {
            assert_eq!(&data[..], b"Hel");
        }
        _ => panic!("Expected Len field"),
    }
}

#[test]
fn test_read_ext_read_i64_field() {
    use super::ReadExtProtobuf;
    use ::std::io::Cursor;

    let data = vec![0x19, 0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]; // field 3: 0x1234567890ABCDEF (Fixed64)
    let len = data.len();
    let reader = Cursor::new(data);
    let fields: Vec<_> = reader
        .read_protobuf_fields(len)
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_number.as_u32(), 3);
    match &fields[0].value {
        FieldValue::I64(bytes) => {
            assert_eq!(*bytes, [0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]);
        }
        _ => panic!("Expected I64 field"),
    }
}

#[test]
fn test_read_ext_read_empty_stream() {
    use super::ReadExtProtobuf;
    use ::std::io::Cursor;

    let data = vec![];
    let reader = Cursor::new(data);
    let fields: Vec<_> = reader
        .read_protobuf_fields(0)
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 0);
}

#[test]
fn test_read_ext_respects_len_budget() {
    use super::ReadExtProtobuf;
    use ::std::io::{Cursor, Read};

    // Two fields; budget covers only the first.
    let data = vec![
        0x08, 0x96, 0x01, // field 1: 150
        0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
    ];
    let mut reader = Cursor::new(data);
    let fields: Vec<_> = reader
        .by_ref()
        .read_protobuf_fields(3)
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_number.as_u32(), 1);
    // Remaining bytes after the budget are still on the cursor.
    assert_eq!(reader.position(), 3);
    let rest = reader.into_inner();
    assert_eq!(&rest[3..], &[0x12, 0x03, 0x48, 0x65, 0x6c]);
}

#[test]
fn test_read_ext_oversized_budget_is_unexpected_eof() {
    use super::ReadExtProtobuf;
    use crate::ProtobufError;
    use ::std::io::Cursor;

    // One complete field (3 bytes), but the budget claims more than the
    // underlying reader can supply. After the field is consumed, the local
    // budget still has remaining capacity while the cursor is at EOF, so
    // the next poll is UnexpectedEof (not a soft end).
    let data = vec![0x08, 0x96, 0x01]; // field 1: 150
    let result: ::std::result::Result<Vec<_>, _> =
        Cursor::new(data).read_protobuf_fields(100).collect();
    assert!(
        matches!(result, Err(ProtobufError::UnexpectedEof)),
        "got {result:?}"
    );
}

#[test]
fn test_read_ext_truncated_fixed_and_len() {
    use super::ReadExtProtobuf;
    use crate::ProtobufError;
    use ::std::io::Cursor;

    // Mid-field truncation with budget == input.len(): the local remaining
    // count catches the shortfall before the underlying Read EOFs.
    for input in [
        &[0x0D, 0x01, 0x02, 0x03][..],    // truncated i32
        &[0x09, 0, 0, 0, 0, 0, 0, 0][..], // truncated i64
        &[0x0A, 0x05, b'a', b'b'][..],    // truncated len
    ] {
        let error = Cursor::new(input)
            .read_protobuf_fields(input.len())
            .next()
            .unwrap()
            .unwrap_err();
        assert!(
            matches!(error, ProtobufError::UnexpectedEof),
            "input={input:?} got {error:?}"
        );
    }
}

#[test]
fn test_as_ref_ext_algorithm_edges() {
    use super::AsRefExtProtobuf;
    use crate::ProtobufError;

    let multi_byte_tag = &[0x80, 0x01, 0x01][..];
    let fields: Vec<_> = multi_byte_tag
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields[0].field_number.as_u32(), 16);
    match &fields[0].value {
        FieldValue::Varint(varint) => assert_eq!(varint.to_uint64(), 1),
        other => panic!("expected varint, got {other:?}"),
    }

    let empty_len = &[0x0A, 0x00][..];
    let fields: Vec<_> = empty_len
        .read_protobuf_fields()
        .collect::<::std::result::Result<Vec<_>, _>>()
        .unwrap();
    match &fields[0].value {
        FieldValue::Len(data) => assert!(data.is_empty()),
        other => panic!("expected empty len, got {other:?}"),
    }

    let truncated_i32 = [0x0D, 0x01, 0x02, 0x03][..]
        .read_protobuf_fields()
        .next()
        .unwrap();
    assert!(matches!(truncated_i32, Err(ProtobufError::UnexpectedEof)));

    let truncated_i64 = [0x09, 0, 0, 0, 0, 0, 0, 0][..]
        .read_protobuf_fields()
        .next()
        .unwrap();
    assert!(matches!(truncated_i64, Err(ProtobufError::UnexpectedEof)));

    let truncated_len = [0x0A, 0x05, b'a', b'b'][..]
        .read_protobuf_fields()
        .next()
        .unwrap();
    assert!(matches!(truncated_len, Err(ProtobufError::UnexpectedEof)));

    let length_past_u32 = [0x0A, 0x80, 0x80, 0x80, 0x80, 0x10][..]
        .read_protobuf_fields()
        .next()
        .unwrap();
    assert!(matches!(
        length_past_u32,
        Err(ProtobufError::VarintDowncastOutOfRange {
            value: 0x1_0000_0000,
            target_type: "u32",
        })
    ));

    // Wire types 3 and 4 are group tags; field readers return them as values.
    for (byte, expected) in [(0x0Bu8, FieldValue::SGroup), (0x0C, FieldValue::EGroup)] {
        let bytes = [byte];
        let field = bytes[..].read_protobuf_fields().next().unwrap().unwrap();
        assert_eq!(field.field_number.as_u32(), 1);
        assert_eq!(field.value, expected);
    }
}

mod group_body_adapter {
    use super::super::{AsRefExtProtobuf, ReadExtProtobuf};
    use crate::field_number::FieldNumber;
    use crate::{FieldValue, ProtobufError};
    use ::std::io::Cursor;

    fn slice_fields(bytes: &[u8]) -> super::super::FieldIterFromSlice<'_> {
        AsRefExtProtobuf::read_protobuf_fields(bytes)
    }

    fn expect_sgroup(fields: &mut super::super::FieldIterFromSlice<'_>) -> FieldNumber {
        let f = fields.next().unwrap().unwrap();
        assert!(matches!(f.value, FieldValue::SGroup));
        f.field_number
    }

    #[test]
    fn empty_group_yields_nothing_then_continues() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        assert!(fields.group_body(n).next().is_none());
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 1);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
    }

    #[test]
    fn group_body_accessors_expose_field_number_and_inner() {
        let bytes = [0x0B, 0x0C, 0x10, 0x02];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let mut body = fields.group_body(n);
        assert_eq!(body.field_number(), n);
        let _ = body.inner();
        let _ = body.inner_mut();
        assert!(body.next().is_none()); // empty group body
        let after = body.into_inner().next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 2);
    }

    #[test]
    fn group_body_propagates_inner_decode_error() {
        // SGroup then truncated fixed32 tag/payload → Err from inner parse.
        let bytes = [0x0B, 0x0D, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let err = fields.group_body(n).next().unwrap().unwrap_err();
        assert!(matches!(err, ProtobufError::UnexpectedEof));
    }

    #[test]
    fn after_none_further_next_may_read_past_group() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let mut body = fields.group_body(n);
        assert!(body.next().is_none());
        // Not fused: another next pulls the field after the group.
        let after = body.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 1);
    }

    #[test]
    fn fuse_keeps_none_after_group_ends() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let mut body = fields.group_body(n).fuse();
        assert!(body.next().is_none());
        assert!(body.next().is_none());
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 1);
    }

    #[test]
    fn yields_body_fields_and_consumes_end() {
        let bytes = [0x0B, 0x10, 0x96, 0x01, 0x0C, 0x18, 0x07];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let body: Vec<_> = fields.group_body(n).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].field_number.as_u32(), 2);
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
    }

    #[test]
    fn nested_adapter_checks_inner_end_number() {
        let bytes = [
            0x0B, // 1 SGroup
            0x13, // 2 SGroup
            0x18, 0x01, // 3 varint 1
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
            0x20, 0x02, // 4 varint 2
        ];
        let mut fields = slice_fields(&bytes);
        let outer_n = expect_sgroup(&mut fields);
        let mut outer = fields.group_body(outer_n);
        let inner_start = outer.next().unwrap().unwrap();
        assert!(matches!(inner_start.value, FieldValue::SGroup));
        {
            let mut inner = outer.group_body(inner_start.field_number);
            let v = inner.next().unwrap().unwrap();
            assert_eq!(v.field_number.as_u32(), 3);
            assert!(inner.next().is_none());
        }
        assert!(outer.next().is_none());
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 4);
    }

    #[test]
    fn depth_only_allows_same_number_nesting_without_inner_adapter() {
        // SGroup(1) SGroup(1) varint EGroup(1) EGroup(1)
        let bytes = [0x0B, 0x0B, 0x10, 0x01, 0x0C, 0x0C];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let body: Vec<_> = fields.group_body(n).collect::<Result<Vec<_>, _>>().unwrap();
        // Inner SGroup + varint + inner EGroup yielded; outer EGroup consumed.
        assert_eq!(body.len(), 3);
        assert!(matches!(body[0].value, FieldValue::SGroup));
        assert!(matches!(body[1].value, FieldValue::Varint(_)));
        assert!(matches!(body[2].value, FieldValue::EGroup));
        assert!(fields.next().is_none());
    }

    #[test]
    fn mismatched_outer_end_is_error() {
        let bytes = [0x0B, 0x14];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let err = fields.group_body(n).next().unwrap().unwrap_err();
        assert!(
            matches!(
                err,
                ProtobufError::MismatchedEndGroup { expected, got }
                if expected.as_u32() == 1 && got.as_u32() == 2
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn outer_adapter_does_not_verify_mismatched_inner_end_number() {
        // SGroup(1) SGroup(2) EGroup(1): at depth>0 the EGroup is treated as a
        // nested end (depth--) without checking field number 1 vs 2, and is
        // yielded. The outer matching end never appears → UnexpectedEof.
        let bytes = [0x0B, 0x13, 0x0C];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let mut body = fields.group_body(n);
        assert!(matches!(
            body.next().unwrap().unwrap().value,
            FieldValue::SGroup
        ));
        let eg = body.next().unwrap().unwrap();
        assert!(matches!(eg.value, FieldValue::EGroup));
        assert_eq!(eg.field_number.as_u32(), 1);
        assert!(matches!(
            body.next().unwrap().unwrap_err(),
            ProtobufError::UnexpectedEof
        ));
    }

    #[test]
    fn truncated_group_body_is_unexpected_eof() {
        let bytes = [0x0B, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let n = expect_sgroup(&mut fields);
        let err = fields
            .group_body(n)
            .collect::<Result<Vec<_>, _>>()
            .unwrap_err();
        assert!(matches!(err, ProtobufError::UnexpectedEof));
    }

    #[test]
    fn works_on_read_iterator() {
        let bytes = [0x0B, 0x10, 0x01, 0x0C, 0x18, 0x07];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        let body: Vec<_> = fields
            .group_body(start.field_number)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(body.len(), 1);
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
    }
}

mod group_preserve {
    use super::super::{AsRefExtProtobuf, ReadExtProtobuf, until_group_ends};
    use crate::field_number::FieldNumber;
    use crate::{FieldValue, ProtobufError, WriteExtProtobuf};
    use ::std::io::Cursor;

    fn slice_fields(bytes: &[u8]) -> super::super::FieldIterFromSlice<'_> {
        AsRefExtProtobuf::read_protobuf_fields(bytes)
    }

    fn expect_sgroup(fields: &mut super::super::FieldIterFromSlice<'_>) -> FieldNumber {
        let f = fields.next().unwrap().unwrap();
        assert!(matches!(f.value, FieldValue::SGroup));
        f.field_number
    }

    fn message_with_group_then_varint() -> Vec<u8> {
        vec![
            0x0B, // field 1 SGroup
            0x10, 0x96, 0x01, // field 2 varint 150
            0x0C, // field 1 EGroup
            0x18, 0x07, // field 3 varint 7
        ]
    }

    #[test]
    fn take_empty_group_body() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let body = fields.take_until(until_group_ends(field_number)).unwrap();
        assert_eq!(body, &[0x0C]);
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 1);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
    }

    #[test]
    fn take_group_with_nested_scalars() {
        let bytes = message_with_group_then_varint();
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let body = fields.take_until(until_group_ends(field_number)).unwrap();
        assert_eq!(body, &[0x10, 0x96, 0x01, 0x0C]);
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 3);
    }

    #[test]
    fn take_nested_groups() {
        let bytes = [
            0x0B, // 1 SGroup
            0x13, // 2 SGroup
            0x18, 0x01, // 3 varint 1
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
            0x20, 0x02, // 4 varint 2
        ];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let body = fields.take_until(until_group_ends(field_number)).unwrap();
        assert_eq!(body, &[0x13, 0x18, 0x01, 0x14, 0x0C]);
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 4);
    }

    #[test]
    fn take_group_containing_len_and_fixed() {
        let bytes = [
            0x0B, // 1 SGroup
            0x12, 0x03, b'a', b'b', b'c', // field 2 len "abc"
            0x15, 0x01, 0x02, 0x03, 0x04, // field 2 fixed32
            0x09, 0, 0, 0, 0, 0, 0, 0, 0,    // field 1 fixed64
            0x0C, // 1 EGroup
            0x10, 0x05, // field 2 varint 5
        ];
        let expected_body = &bytes[1..bytes.len() - 2];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let body = fields.take_until(until_group_ends(field_number)).unwrap();
        assert_eq!(body, expected_body);
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 2);
    }

    #[test]
    fn take_mismatched_end_group_is_an_error() {
        let bytes = [0x0B, 0x14];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let err = fields
            .take_until(until_group_ends(field_number))
            .unwrap_err();
        assert!(
            matches!(
                err,
                ProtobufError::MismatchedEndGroup { expected, got }
                if expected.as_u32() == 1 && got.as_u32() == 2
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn take_truncated_group_is_unexpected_eof() {
        let bytes = [0x0B, 0x08, 0x01];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        assert!(matches!(
            fields.take_until(until_group_ends(field_number)),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn read_copy_until_preserves_group_body() {
        let bytes = message_with_group_then_varint();
        let len = bytes.len();
        let mut fields = Cursor::new(bytes.clone()).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x10, 0x96, 0x01, 0x0C]);
        assert_eq!(written, out.len());

        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn read_copy_until_empty_group_body() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x0C]);
        assert_eq!(written, 1);

        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 1);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
    }

    #[test]
    fn read_copy_until_nested_groups_and_len() {
        let bytes = [
            0x0B, // 1 SGroup
            0x13, // 2 SGroup
            0x12, 0x02, b'x', b'y', // field 2 len "xy" inside inner group
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
        ];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x13, 0x12, 0x02, b'x', b'y', 0x14, 0x0C]);
        assert_eq!(written, out.len());
        assert!(fields.next().is_none());
    }

    #[test]
    fn read_copy_until_mismatched_end_group() {
        let bytes = [0x0B, 0x14];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        let mut out = Vec::new();
        let err = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap_err();
        assert!(matches!(
            err,
            ProtobufError::MismatchedEndGroup { expected, got }
            if expected.as_u32() == 1 && got.as_u32() == 2
        ));
        // Partial write includes the mismatched end-group tag that failed pred.
        assert_eq!(out, vec![0x14]);
    }

    #[test]
    fn read_copy_until_truncated_group_is_unexpected_eof() {
        let bytes = [0x0B, 0x08, 0x01];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut out = Vec::new();
        let err = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap_err();
        assert!(matches!(err, ProtobufError::UnexpectedEof));
        // Copied the incomplete body's varint field before EOF waiting for EGroup.
        assert_eq!(out, vec![0x08, 0x01]);
    }

    #[test]
    fn round_trip_preserved_group_reparsed() {
        let bytes = [
            0x0B, // 1 SGroup
            0x10, 0x01, // 2 varint 1
            0x12, 0x03, b'a', b'b', b'c', // 2 len
            0x15, 0x11, 0x22, 0x33, 0x44, // 2 fixed32
            0x13, // 2 SGroup
            0x18, 0x02, // 3 varint 2
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
        ];
        let mut fields = slice_fields(&bytes);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));

        let mut preserved = Vec::new();
        preserved.write_protobuf_field(&start).unwrap();
        let body = fields
            .take_until(until_group_ends(start.field_number))
            .unwrap();
        preserved.extend_from_slice(body);
        assert_eq!(preserved.as_slice(), bytes.as_slice());

        let reparsed: Vec<_> = AsRefExtProtobuf::read_protobuf_fields(preserved.as_slice())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(reparsed.len(), 8);
        assert!(matches!(reparsed[0].value, FieldValue::SGroup));
        assert_eq!(reparsed[0].field_number.as_u32(), 1);
        assert!(matches!(&reparsed[1].value, FieldValue::Varint(v) if v.to_uint64() == 1));
        assert!(matches!(reparsed[2].value, FieldValue::Len(b"abc")));
        assert!(matches!(
            &reparsed[3].value,
            FieldValue::I32([0x11, 0x22, 0x33, 0x44])
        ));
        assert!(matches!(reparsed[4].value, FieldValue::SGroup));
        assert_eq!(reparsed[4].field_number.as_u32(), 2);
        assert!(matches!(&reparsed[5].value, FieldValue::Varint(v) if v.to_uint64() == 2));
        assert!(matches!(reparsed[6].value, FieldValue::EGroup));
        assert_eq!(reparsed[6].field_number.as_u32(), 2);
        assert!(matches!(reparsed[7].value, FieldValue::EGroup));
        assert_eq!(reparsed[7].field_number.as_u32(), 1);
    }

    #[test]
    fn read_copy_until_round_trip_matches_slice_take() {
        let bytes = message_with_group_then_varint();
        let group_only = &bytes[..bytes.len() - 2]; // drop trailing field 3

        let mut slice_fields = slice_fields(group_only);
        let start = slice_fields.next().unwrap().unwrap();
        let mut from_slice = Vec::new();
        from_slice.write_protobuf_field(&start).unwrap();
        from_slice.extend_from_slice(
            slice_fields
                .take_until(until_group_ends(start.field_number))
                .unwrap(),
        );

        let len = group_only.len();
        let mut read_fields = Cursor::new(group_only.to_vec()).read_protobuf_fields(len);
        let start = read_fields.next().unwrap().unwrap();
        let mut from_read = Vec::new();
        let start_len = from_read.write_protobuf_field(&start).unwrap();
        let body_written = read_fields
            .copy_until(until_group_ends(start.field_number), &mut from_read)
            .unwrap();

        assert_eq!(from_slice, group_only);
        assert_eq!(from_read, from_slice);
        assert_eq!(start_len + body_written, from_read.len());
    }

    #[test]
    fn read_copy_until_preserves_noncanonical_varints() {
        // Overlong length prefix (1 encoded as 0x81 0x00) must not be
        // re-canonicalized to 0x01 when copying through Read.
        let bytes = [
            0x0B, // 1 SGroup
            0x12, 0x81, 0x00, b'x', // 2 Len, non-canonical length=1, payload 'x'
            0x0C, // 1 EGroup
        ];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, vec![0x12, 0x81, 0x00, b'x', 0x0C]);
        assert_eq!(written, out.len());
    }

    #[test]
    fn read_copy_until_preserves_fixed32_and_fixed64() {
        let bytes = [
            0x0B, // 1 SGroup
            0x15, 0x01, 0x02, 0x03, 0x04, // 2 fixed32
            0x09, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, // 1 fixed64
            0x0C, // 1 EGroup
        ];
        let expected_body = &bytes[1..];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        let mut out = Vec::new();
        let written = fields
            .copy_until(until_group_ends(start.field_number), &mut out)
            .unwrap();
        assert_eq!(out, expected_body);
        assert_eq!(written, out.len());
        assert!(fields.next().is_none());
    }
}

mod group_skip {
    use super::super::slice::SliceReader;
    use super::super::wire::WireCursor;
    use super::super::{AsRefExtProtobuf, ReadExtProtobuf, until_group_ends};
    use crate::field_number::FieldNumber;
    use crate::wire_format::WireType;
    use crate::{FieldValue, ProtobufError};

    fn field(n: u32) -> FieldNumber {
        FieldNumber::try_new(n).unwrap()
    }

    fn slice_fields(bytes: &[u8]) -> super::super::FieldIterFromSlice<'_> {
        AsRefExtProtobuf::read_protobuf_fields(bytes)
    }

    /// Take the next field and assert it is an `SGroup`; return its field number.
    fn expect_sgroup(fields: &mut super::super::FieldIterFromSlice<'_>) -> FieldNumber {
        let field = fields.next().unwrap().unwrap();
        assert!(
            matches!(field.value, FieldValue::SGroup),
            "expected SGroup, got {:?}",
            field.value
        );
        field.field_number
    }

    /// field 1 start-group, nested field 2 varint 150, field 1 end-group, then
    /// field 3 varint 7.
    fn message_with_group_then_varint() -> Vec<u8> {
        vec![
            0x0B, // field 1 SGroup
            0x10, 0x96, 0x01, // field 2 varint 150
            0x0C, // field 1 EGroup
            0x18, 0x07, // field 3 varint 7
        ]
    }

    #[test]
    fn skip_empty_group_then_read_next_field() {
        let bytes = [0x0B, 0x0C, 0x08, 0x01]; // empty group 1, then field 1 = 1
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        fields.skip_until(until_group_ends(field_number)).unwrap();
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 1);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
        assert!(fields.next().is_none());
    }

    #[test]
    fn skip_group_with_nested_fields_on_slice_iter() {
        let bytes = message_with_group_then_varint();
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        assert_eq!(field_number.as_u32(), 1);
        fields.skip_until(until_group_ends(field_number)).unwrap();
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn skip_nested_groups() {
        // outer 1 { inner 2 { field 3 = 1 } } then field 4 = 2
        let bytes = [
            0x0B, // 1 SGroup
            0x13, // 2 SGroup
            0x18, 0x01, // 3 varint 1
            0x14, // 2 EGroup
            0x0C, // 1 EGroup
            0x20, 0x02, // 4 varint 2
        ];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        fields.skip_until(until_group_ends(field_number)).unwrap();
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 4);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 2);
    }

    #[test]
    fn skip_group_containing_len_and_fixed() {
        let bytes = [
            0x0B, // 1 SGroup
            0x12, 0x03, b'a', b'b', b'c', // field 2 len "abc"
            0x15, 0x01, 0x02, 0x03, 0x04, // field 2 fixed32
            0x09, 0, 0, 0, 0, 0, 0, 0, 0,    // field 1 fixed64
            0x0C, // 1 EGroup
            0x10, 0x05, // field 2 varint 5
        ];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        fields.skip_until(until_group_ends(field_number)).unwrap();
        let next = fields.next().unwrap().unwrap();
        assert_eq!(next.field_number.as_u32(), 2);
        let FieldValue::Varint(v) = next.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 5);
    }

    #[test]
    fn read_group_body_field_by_field_until_egroup() {
        let bytes = message_with_group_then_varint();
        let mut fields = slice_fields(&bytes);
        assert_eq!(expect_sgroup(&mut fields).as_u32(), 1);

        let inner = fields.next().unwrap().unwrap();
        assert_eq!(inner.field_number.as_u32(), 2);
        let FieldValue::Varint(v) = inner.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 150);

        let end = fields.next().unwrap().unwrap();
        assert_eq!(end.field_number.as_u32(), 1);
        assert!(matches!(end.value, FieldValue::EGroup));

        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn mismatched_end_group_is_an_error() {
        // start field 1, end field 2
        let bytes = [0x0B, 0x14];
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        let skip_err = fields
            .skip_until(until_group_ends(field_number))
            .unwrap_err();
        assert!(
            matches!(
                skip_err,
                ProtobufError::MismatchedEndGroup { expected, got }
                if expected.as_u32() == 1 && got.as_u32() == 2
            ),
            "got {skip_err:?}"
        );
    }

    #[test]
    fn truncated_group_is_unexpected_eof() {
        let bytes = [0x0B, 0x08, 0x01]; // start group, varint field, no end
        let mut fields = slice_fields(&bytes);
        let field_number = expect_sgroup(&mut fields);
        assert!(matches!(
            fields.skip_until(until_group_ends(field_number)),
            Err(ProtobufError::UnexpectedEof)
        ));
    }

    #[test]
    fn skip_via_read_ext_after_start_tag() {
        use ::std::io::{Cursor, Read};

        let bytes = message_with_group_then_varint();
        let mut reader = Cursor::new(bytes);
        assert_eq!(
            reader
                .by_ref()
                .read_protobuf_fields(1)
                .next()
                .unwrap()
                .unwrap()
                .value,
            FieldValue::SGroup
        );
        let remaining = reader.get_ref().len() - reader.position() as usize;
        let mut fields = reader.read_protobuf_fields(remaining);
        fields.skip_until(until_group_ends(field(1))).unwrap();
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
    }

    #[test]
    fn skip_field_returns_tag_including_groups() {
        let bytes = message_with_group_then_varint();
        let mut slice = bytes.as_slice();
        let mut cursor = SliceReader { slice: &mut slice };

        let start = cursor.skip_field().unwrap();
        assert_eq!(start.field_number.as_u32(), 1);
        assert_eq!(start.wire_type, WireType::SGroup);

        cursor
            .skip_until(until_group_ends(start.field_number))
            .unwrap();

        let after = cursor.skip_field().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        assert_eq!(after.wire_type, WireType::Varint);
        assert_eq!(cursor.remaining(), 0);
    }

    #[test]
    fn skip_field_on_slice_reader_skips_len_payload() {
        // field 1 SGroup, field 2 len "ab", field 1 EGroup
        let bytes = [0x0B, 0x12, 0x02, b'a', b'b', 0x0C];
        let mut slice = bytes.as_slice();
        let mut cursor = SliceReader { slice: &mut slice };

        let start = cursor.skip_field().unwrap();
        assert_eq!(start.wire_type, WireType::SGroup);
        cursor
            .skip_until(until_group_ends(start.field_number))
            .unwrap();
        assert_eq!(cursor.remaining(), 0);
        assert!(slice.is_empty());
    }

    #[test]
    fn slice_reader_read_varint_with_raw_returns_input_subslice() {
        use crate::wire_format::MAX_VARINT_BYTES;

        let bytes = [0x96u8, 0x01, 0x08];
        let mut slice = bytes.as_slice();
        let mut cursor = SliceReader { slice: &mut slice };
        let mut buf = [0u8; MAX_VARINT_BYTES];
        let (varint, raw) = cursor.read_varint_with_raw(&mut buf).unwrap();
        assert_eq!(varint.to_uint64(), 150);
        assert_eq!(raw, &[0x96, 0x01]);
        assert!(::std::ptr::eq(raw.as_ptr(), bytes.as_ptr()));
        assert_eq!(slice, &[0x08]);
    }

    #[test]
    fn slice_reader_copy_field_copies_fixed_and_len() {
        // field 2 fixed32, field 3 fixed64, field 4 len "ab"
        let bytes = [
            0x15, 0x01, 0x02, 0x03, 0x04, // fixed32
            0x19, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, // fixed64
            0x22, 0x02, b'a', b'b', // len
        ];
        let mut slice = bytes.as_slice();
        let mut cursor = SliceReader { slice: &mut slice };
        let mut out = Vec::new();
        let (tag, n) = cursor.copy_field(&mut out).unwrap();
        assert_eq!(tag.wire_type, WireType::Int32);
        assert_eq!(n, 5);
        let (tag, n) = cursor.copy_field(&mut out).unwrap();
        assert_eq!(tag.wire_type, WireType::Int64);
        assert_eq!(n, 9);
        let (tag, n) = cursor.copy_field(&mut out).unwrap();
        assert_eq!(tag.wire_type, WireType::Len);
        assert_eq!(n, 4);
        assert_eq!(out, bytes);
        assert!(slice.is_empty());
    }

    #[test]
    fn skip_via_field_iter_from_read() {
        use ::std::io::Cursor;

        let bytes = message_with_group_then_varint();
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        fields
            .skip_until(until_group_ends(start.field_number))
            .unwrap();
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn skip_group_with_len_payload_on_read_iter() {
        use ::std::io::Cursor;

        // Exercises ReadCursor::skip_bytes via Len inside skip_until.
        // field 1 SGroup, field 2 len "ab", field 1 EGroup, field 3 varint 7
        let bytes = [0x0B, 0x12, 0x02, b'a', b'b', 0x0C, 0x18, 0x07];
        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        fields
            .skip_until(until_group_ends(start.field_number))
            .unwrap();
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }

    #[test]
    fn skip_large_len_payload_on_read_iter() {
        use crate::varint::Varint;
        use ::std::io::Cursor;

        // Len payload longer than ReadCursor's 256-byte skip buffer so the
        // multi-chunk skip_bytes loop runs.
        const PAYLOAD_LEN: usize = 300;
        let mut bytes = vec![0x0B]; // field 1 SGroup
        bytes.push(0x12); // field 2 Len
        let (encoded, count) = Varint::from_uint64(PAYLOAD_LEN as u64).encode();
        bytes.extend_from_slice(&encoded[..count]);
        bytes.extend(::std::iter::repeat_n(0u8, PAYLOAD_LEN));
        bytes.push(0x0C); // field 1 EGroup
        bytes.extend_from_slice(&[0x18, 0x07]); // field 3 varint 7

        let len = bytes.len();
        let mut fields = Cursor::new(bytes).read_protobuf_fields(len);
        let start = fields.next().unwrap().unwrap();
        assert!(matches!(start.value, FieldValue::SGroup));
        fields
            .skip_until(until_group_ends(start.field_number))
            .unwrap();
        let after = fields.next().unwrap().unwrap();
        assert_eq!(after.field_number.as_u32(), 3);
        let FieldValue::Varint(v) = after.value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 7);
    }
}

mod len_payload {
    use super::super::{Limited, ReadExtProtobuf};
    use crate::varint::Varint;
    use crate::{Field, FieldNumber, FieldValue, ProtobufError};
    use ::std::io::{Cursor, Read};

    const LARGE_LEN: usize = 8200;

    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    fn read_bytes_payload<R: Read>(
        _n: FieldNumber,
        body: &mut Limited<'_, R>,
    ) -> crate::Result<Bytes> {
        let mut data = vec![0; body.remaining()];
        body.read_exact(&mut data)?;
        Ok(Bytes(data))
    }

    fn len_message(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0x0A];
        let (encoded, count) = Varint::from_uint64(payload.len() as u64).encode();
        out.extend_from_slice(&encoded[..count]);
        out.extend_from_slice(payload);
        out
    }

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|index| index as u8).collect()
    }

    fn assert_bytes(field_bytes: &[u8], expected: &[u8]) {
        let owned = field_bytes.to_vec();
        let len = owned.len();
        let from_read = Cursor::new(owned)
            .read_protobuf_fields_with(len, read_bytes_payload)
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(Bytes(data)) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);
    }

    #[test]
    fn closure_fills_empty_small_and_large_payloads() {
        assert_bytes(&len_message(b""), b"");
        assert_bytes(&len_message(b"Hel"), b"Hel");
        let big = payload(LARGE_LEN);
        assert_bytes(&len_message(&big), &big);
    }

    #[test]
    fn short_payload_is_an_error_on_read() {
        let input = [0x0A, 0x05, b'a', b'b'];
        // Length prefix claims 5 bytes but the message budget only has 2 left.
        let error = Cursor::new(input)
            .read_protobuf_fields_with(input.len(), read_bytes_payload)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    fn assert_box(message: &[u8], expected: &[u8]) {
        let from_read = Cursor::new(message.to_vec())
            .read_protobuf_fields_with(message.len(), |_n, body: &mut Limited<'_, _>| {
                let mut buf = vec![0u8; body.remaining()];
                body.read_exact(&mut buf)?;
                Ok(buf.into_boxed_slice())
            })
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], expected);
    }

    #[test]
    fn box_closure_reads_payload_on_read() {
        assert_box(&len_message(b""), b"");
        assert_box(&len_message(b"Hel"), b"Hel");
        let big = payload(LARGE_LEN);
        assert_box(&len_message(&big), &big);
    }

    #[derive(Debug)]
    struct Tagged {
        id: u64,
        data: Vec<u8>,
    }

    fn two_len_fields() -> Vec<u8> {
        let mut message = len_message(b"Hel");
        message.extend_from_slice(&len_message(b"lo!"));
        message
    }

    fn assert_tagged(fields: &[Field<Tagged>], id: u64) {
        assert_eq!(fields.len(), 2);
        let FieldValue::Len(first) = &fields[0].value else {
            panic!("expected len");
        };
        let FieldValue::Len(second) = &fields[1].value else {
            panic!("expected len");
        };
        assert_eq!(first.data, b"Hel");
        assert_eq!(second.data, b"lo!");
        assert_eq!(first.id, id);
        assert_eq!(second.id, id);
    }

    #[test]
    fn closure_captures_state_for_each_length_field() {
        let message = two_len_fields();
        let id = 7u64;
        let len = message.len();

        let from_read = Cursor::new(message)
            .read_protobuf_fields_with(len, |_n, body: &mut Limited<'_, _>| {
                let mut data = vec![0; body.remaining()];
                body.read_exact(&mut data)?;
                Ok(Tagged { id, data })
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_tagged(&from_read, id);
    }

    #[test]
    fn closure_runs_only_for_length_fields() {
        let mixed = [0x08, 0x96, 0x01, 0x12, 0x03, b'H', b'e', b'l'];
        let mut calls = 0usize;
        let fields = Cursor::new(mixed)
            .read_protobuf_fields_with(mixed.len(), |_n, body: &mut Limited<'_, _>| {
                calls += 1;
                read_bytes_payload(_n, body)
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(calls, 1);
        assert!(matches!(fields[0].value, FieldValue::Varint(_)));
        assert_eq!(fields[0].field_number.as_u32(), 1);
        let FieldValue::Len(Bytes(data)) = &fields[1].value else {
            panic!("expected len");
        };
        assert_eq!(data, b"Hel");
        assert_eq!(fields[1].field_number.as_u32(), 2);
    }

    #[test]
    fn closure_error_fails_the_field() {
        let input = [0x0A, 0x03, b'H', b'e', b'l'];
        let error = Cursor::new(input)
            .read_protobuf_fields_with(
                input.len(),
                |_n, _body: &mut Limited<'_, _>| -> crate::Result<Vec<u8>> {
                    Err(ProtobufError::UnexpectedEof)
                },
            )
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    #[derive(Debug)]
    struct Spare {
        data: Vec<u8>,
    }

    fn assert_spare(fields: &[Field<Spare>]) {
        assert_eq!(fields.len(), 2);
        let FieldValue::Len(buf) = &fields[0].value else {
            panic!("expected len");
        };
        assert_eq!(&buf.data[..3], b"Hel");
        assert_eq!(&buf.data[3..], &[0xAB; 4]);
        let FieldValue::Varint(varint) = &fields[1].value else {
            panic!("expected varint");
        };
        assert_eq!(varint.to_uint64(), 1);
        assert_eq!(fields[1].field_number.as_u32(), 2);
    }

    #[test]
    fn longer_buffer_keeps_the_next_field_and_the_suffix() {
        let message = [0x0A, 0x03, b'H', b'e', b'l', 0x10, 0x01];

        let from_read = Cursor::new(message)
            .read_protobuf_fields_with(message.len(), |_n, body: &mut Limited<'_, _>| {
                let len = body.remaining();
                let mut data = vec![0xAB; len + 4];
                body.read_exact(&mut data[..len])?;
                Ok(Spare { data })
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_spare(&from_read);
    }

    #[test]
    fn unread_body_bytes_are_skipped() {
        // Handler reads only the first byte; the rest of the Len body must be
        // drained so the following varint field is still visible.
        let message = [0x0A, 0x03, b'H', b'e', b'l', 0x10, 0x01];
        let fields = Cursor::new(message)
            .read_protobuf_fields_with(message.len(), |_n, body: &mut Limited<'_, _>| {
                let mut one = [0u8; 1];
                body.read_exact(&mut one)?;
                Ok(one[0])
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(fields.len(), 2);
        assert!(matches!(fields[0].value, FieldValue::Len(b'H')));
        let FieldValue::Varint(v) = &fields[1].value else {
            panic!("expected varint");
        };
        assert_eq!(v.to_uint64(), 1);
    }

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

    #[test]
    fn nested_message_via_on_len() {
        // field 1 string "Hi"; field 2 message { id=7, name="ok" }
        let wire: &[u8] = &[
            0x0A, 0x02, b'H', b'i', //
            0x12, 0x06, //
            0x08, 0x07, //
            0x1A, 0x02, b'o', b'k',
        ];

        fn decode_len<R: Read>(
            field_number: FieldNumber,
            body: &mut Limited<'_, R>,
        ) -> crate::Result<LenValue> {
            match field_number.as_u32() {
                2 => {
                    let mut child = Child::default();
                    let mut raw = |inner_n: FieldNumber, inner: &mut Limited<'_, R>| {
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

        let fields = Cursor::new(wire)
            .read_protobuf_fields_with(wire.len(), decode_len)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(fields.len(), 2);
        assert!(matches!(
            &fields[0].value,
            FieldValue::Len(LenValue::Bytes(b)) if b == b"Hi"
        ));
        assert!(matches!(
            &fields[1].value,
            FieldValue::Len(LenValue::Child(Child { id: 7, name })) if name == b"ok"
        ));
    }
}
