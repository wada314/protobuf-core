//! Field reading utilities for Protocol Buffers
//!
//! This module provides low-level utilities for reading raw protobuf fields from various sources.
//! Streaming sources (`std::io::Read` and byte iterators) build length-delimited
//! payloads with [`LenBuf`](crate::LenBuf). Slice sources borrow them as `&[u8]`.

use super::len_buf::LenBuf;
use crate::field::{Field, FieldValue};
use crate::tag::Tag;
use crate::varint::{DecodeOutcome, Varint};
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};
use ::std::convert::AsRef;
use ::std::io::Read;

// ============================================================================
// Common parsing infrastructure
// ============================================================================

/// Trait for reading field values from different data sources
///
/// This trait abstracts the reading operations needed to parse field values.
/// Different implementations handle different data source types (Iterator, Read, slice, etc.)
pub(crate) trait FieldValueReader<L> {
    /// Read a tag (field number + wire type)
    ///
    /// Default implementation uses `read_varint` and converts the varint to a tag.
    fn read_tag(&mut self) -> Result<Option<Tag>> {
        match self.read_varint()? {
            Some(varint) => Tag::from_encoded(varint).map(Some),
            None => Ok(None),
        }
    }

    /// Read a varint value (returns None when the stream is empty)
    fn read_varint(&mut self) -> Result<Option<Varint>>;

    /// Read exactly N bytes (for fixed-size wire types like Int32, Int64, Fixed32, Fixed64)
    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]>;

    /// Read a length-delimited byte sequence
    ///
    /// The length parameter is the number of bytes to read after the length varint has been parsed.
    fn read_length_delimited(&mut self, length: usize) -> Result<L>;
}

/// Parse a complete field (tag + value) from a field value reader
///
/// This is the common parsing logic for reading a complete field.
/// `L` is the length-delimited payload. Streaming readers use [`LenBuf`](crate::LenBuf).
/// The slice reader uses `&'a [u8]`.
pub(crate) fn parse_field<L, R>(reader: &mut R) -> Result<Option<Field<L>>>
where
    R: FieldValueReader<L>,
{
    // Read tag
    let Some(tag) = reader.read_tag()? else {
        return Ok(None);
    };

    // Parse value based on wire type
    let value = match tag.wire_type {
        WireType::Varint => {
            let varint = reader.read_varint()?.ok_or(ProtobufError::UnexpectedEof)?;
            FieldValue::Varint(varint)
        }
        WireType::Int32 => {
            let bytes = reader.read_fixed::<4>()?;
            FieldValue::I32(bytes)
        }
        WireType::Int64 => {
            let bytes = reader.read_fixed::<8>()?;
            FieldValue::I64(bytes)
        }
        WireType::Len => {
            // Read length prefix (varint)
            let length_varint = reader.read_varint()?.ok_or(ProtobufError::UnexpectedEof)?;
            let length = length_varint.try_to_uint32()? as usize;
            let data = reader.read_length_delimited(length)?;
            FieldValue::Len(data)
        }
        _ => {
            return Err(ProtobufError::InvalidWireType {
                value: tag.wire_type as u8,
            });
        }
    };

    Ok(Some(Field {
        field_number: tag.field_number,
        value,
    }))
}

// ============================================================================
// IteratorExtProtobuf
// ============================================================================

/// Helper struct implementing FieldValueReader for Iterator<Item = u8>
struct IteratorReader<'a, I, A>(&'a mut I, &'a A);

impl<'a, I, L> FieldValueReader<L> for IteratorReader<'a, I, L::Alloc>
where
    I: Iterator<Item = u8>,
    L: LenBuf,
{
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        use crate::varint::IteratorExtVarint;
        match self.0.read_varint_partial()? {
            DecodeOutcome::Complete(v) => Ok(Some(v)),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(_) => Err(ProtobufError::UnexpectedEof),
        }
    }

    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        for byte in bytes.iter_mut() {
            *byte = self.0.next().ok_or(ProtobufError::UnexpectedEof)?;
        }
        Ok(bytes)
    }

    fn read_length_delimited(&mut self, length: usize) -> Result<L> {
        let mut payload = L::with_len(length, self.1.clone());
        for byte in payload.as_mut() {
            *byte = self.0.next().ok_or(ProtobufError::UnexpectedEof)?;
        }
        Ok(payload)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a byte iterator.
///
/// `L` is the length-delimited payload ([`LenBuf`](crate::LenBuf)). It defaults to `Vec<u8>`.
/// `alloc` is cloned into each length-delimited field.
pub struct ProtobufFieldIteratorFromBytes<I, L = Vec<u8>>
where
    L: LenBuf,
{
    iter: I,
    alloc: L::Alloc,
}

impl<I, L> Iterator for ProtobufFieldIteratorFromBytes<I, L>
where
    I: Iterator<Item = u8>,
    L: LenBuf,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = IteratorReader(&mut self.iter, &self.alloc);
        match parse_field(&mut reader) {
            Ok(Some(field)) => Some(Ok(field)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Extension trait for reading raw Protocol Buffer fields from byte iterators.
///
/// This trait provides convenient methods to read fields directly from
/// any iterator that yields bytes. Length-delimited payloads are `L: LenBuf`
/// (for example `Vec<u8>` or `Box<[u8]>`).
///
/// # Example
/// ```
/// use ::protobuf_core::IteratorExtProtobuf;
///
/// let bytes = vec![0x08, 0x96, 0x01]; // field 1: 150
/// let iter = bytes.into_iter();
/// let fields: Vec<_> = iter.protobuf_fields::<Vec<u8>>().collect::<::std::result::Result<Vec<_>, _>>().unwrap();
/// assert_eq!(fields[0].field_number.as_u32(), 1);
/// ```
pub trait IteratorExtProtobuf {
    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// Returns an iterator that yields `Result<Field<L>>`.
    /// `L` is the length-delimited payload. Each field is parsed sequentially.
    /// Length-delimited fields are allocated with `L::Alloc::default()`.
    /// Pass an allocator instance to [`protobuf_fields_in`](IteratorExtProtobuf::protobuf_fields_in).
    fn protobuf_fields<L>(self) -> ProtobufFieldIteratorFromBytes<Self, L>
    where
        Self: Sized,
        L: LenBuf,
        L::Alloc: Default,
    {
        self.protobuf_fields_in(L::Alloc::default())
    }

    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// `alloc` is cloned into each length-delimited field.
    fn protobuf_fields_in<L>(self, alloc: L::Alloc) -> ProtobufFieldIteratorFromBytes<Self, L>
    where
        Self: Sized,
        L: LenBuf;
}

impl<I> IteratorExtProtobuf for I
where
    I: Iterator<Item = u8>,
{
    fn protobuf_fields_in<L>(self, alloc: L::Alloc) -> ProtobufFieldIteratorFromBytes<Self, L>
    where
        Self: Sized,
        L: LenBuf,
    {
        ProtobufFieldIteratorFromBytes { iter: self, alloc }
    }
}

// ============================================================================
// TryIteratorExtProtobuf
// ============================================================================

/// Helper struct implementing FieldValueReader for Iterator<Item = Result<u8, E>>
struct TryIteratorReader<'a, I, A>(&'a mut I, &'a A);

impl<'a, I, E, L> FieldValueReader<L> for TryIteratorReader<'a, I, L::Alloc>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
    L: LenBuf,
{
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        use crate::varint::TryIteratorExtVarint;
        match self.0.read_varint_partial()? {
            DecodeOutcome::Complete(v) => Ok(Some(v)),
            DecodeOutcome::Empty => Ok(None),
            DecodeOutcome::Incomplete(_) => Err(ProtobufError::UnexpectedEof),
        }
    }

    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        for byte in bytes.iter_mut() {
            *byte = self
                .0
                .next()
                .ok_or(ProtobufError::UnexpectedEof)?
                .map_err(Into::into)?;
        }
        Ok(bytes)
    }

    fn read_length_delimited(&mut self, length: usize) -> Result<L> {
        let mut payload = L::with_len(length, self.1.clone());
        for byte in payload.as_mut() {
            *byte = self
                .0
                .next()
                .ok_or(ProtobufError::UnexpectedEof)?
                .map_err(Into::into)?;
        }
        Ok(payload)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a byte iterator that yields `Result<u8, E>`.
///
/// `L` is the length-delimited payload ([`LenBuf`](crate::LenBuf)). It defaults to `Vec<u8>`.
/// `alloc` is cloned into each length-delimited field.
pub struct ProtobufFieldIteratorFromTryBytes<I, L = Vec<u8>>
where
    L: LenBuf,
{
    iter: I,
    alloc: L::Alloc,
}

impl<I, E, L> Iterator for ProtobufFieldIteratorFromTryBytes<I, L>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
    L: LenBuf,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = TryIteratorReader(&mut self.iter, &self.alloc);
        match parse_field(&mut reader) {
            Ok(Some(field)) => Some(Ok(field)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Extension trait for reading raw Protocol Buffer fields from byte iterators that yield `Result<u8, E>`.
///
/// This trait provides convenient methods to read fields directly from
/// any iterator that yields `Result<u8, E>`, allowing proper error propagation
/// from I/O operations. Length-delimited payloads are `L: LenBuf`
/// (for example `Vec<u8>` or `Box<[u8]>`).
///
/// # Example
/// ```
/// use ::std::io::{Cursor, Read};
/// use ::protobuf_core::TryIteratorExtProtobuf;
///
/// let data = vec![0x08, 0x96, 0x01]; // field 1: 150
/// let mut reader = Cursor::new(data);
/// let iter = reader.bytes(); // Iterator<Item = Result<u8, io::Error>>
/// let fields: Vec<_> = iter.protobuf_fields::<Vec<u8>>().collect::<Result<Vec<_>, _>>().unwrap();
/// assert_eq!(fields[0].field_number.as_u32(), 1);
/// ```
pub trait TryIteratorExtProtobuf {
    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// Returns an iterator that yields `Result<Field<L>>`.
    /// `L` is the length-delimited payload.
    /// I/O errors from the underlying iterator are propagated.
    /// Length-delimited fields are allocated with `L::Alloc::default()`.
    /// Pass an allocator instance to [`protobuf_fields_in`](TryIteratorExtProtobuf::protobuf_fields_in).
    fn protobuf_fields<L>(self) -> ProtobufFieldIteratorFromTryBytes<Self, L>
    where
        Self: Sized,
        L: LenBuf,
        L::Alloc: Default,
    {
        self.protobuf_fields_in(L::Alloc::default())
    }

    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// `alloc` is cloned into each length-delimited field.
    fn protobuf_fields_in<L>(self, alloc: L::Alloc) -> ProtobufFieldIteratorFromTryBytes<Self, L>
    where
        Self: Sized,
        L: LenBuf;
}

impl<I, E> TryIteratorExtProtobuf for I
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn protobuf_fields_in<L>(self, alloc: L::Alloc) -> ProtobufFieldIteratorFromTryBytes<Self, L>
    where
        Self: Sized,
        L: LenBuf,
    {
        ProtobufFieldIteratorFromTryBytes { iter: self, alloc }
    }
}

// ============================================================================
// ReadExtProtobuf
// ============================================================================

/// Helper struct implementing FieldValueReader for Read
struct ReadReader<'a, R, A>(&'a mut R, &'a A);

impl<'a, R, L> FieldValueReader<L> for ReadReader<'a, R, L::Alloc>
where
    R: Read,
    L: LenBuf,
{
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        use crate::varint::ReadExtVarint;
        self.0.read_varint()
    }

    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        self.0.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn read_length_delimited(&mut self, length: usize) -> Result<L> {
        let mut payload = L::with_len(length, self.1.clone());
        self.0.read_exact(payload.as_mut())?;
        Ok(payload)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a reader.
///
/// `L` is the length-delimited payload ([`LenBuf`](crate::LenBuf)). It defaults to `Vec<u8>`.
/// `alloc` is cloned into each length-delimited field.
pub struct ProtobufFieldIterator<R, L = Vec<u8>>
where
    L: LenBuf,
{
    reader: R,
    alloc: L::Alloc,
}

impl<R, L> Iterator for ProtobufFieldIterator<R, L>
where
    R: Read,
    L: LenBuf,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = ReadReader(&mut self.reader, &self.alloc);
        match parse_field(&mut reader) {
            Ok(Some(field)) => Some(Ok(field)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Extension trait for reading raw Protocol Buffer fields from `Read` types
///
/// This trait provides low-level utilities for reading field-by-field from a byte stream.
/// Length-delimited payloads are `L: LenBuf` (for example `Vec<u8>` or `Box<[u8]>`).
/// It does not provide semantic interpretation - that is the caller's responsibility.
///
/// # Example
/// ```
/// use ::protobuf_core::{ReadExtProtobuf, Field, FieldValue};
///
/// fn main() -> Result<(), Box<dyn ::std::error::Error>> {
///     let reader = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
///
///     for field in reader.read_protobuf_fields::<Vec<u8>>() {
///         let field = field?;
///         match field.value {
///             FieldValue::Varint(varint) => {
///                 println!("Field {}: {}", field.field_number.as_u32(), varint.to_uint64());
///             },
///             FieldValue::Len(data) => {
///                 println!("Field {}: {:?}", field.field_number.as_u32(), data);
///             },
///             _ => {}
///         }
///     }
///     Ok(())
/// }
/// ```
pub trait ReadExtProtobuf {
    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// This consumes the reader and returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// `L` is the length-delimited payload.
    /// Length-delimited fields are allocated with `L::Alloc::default()`.
    /// Pass an allocator instance to [`read_protobuf_fields_in`](ReadExtProtobuf::read_protobuf_fields_in).
    fn read_protobuf_fields<L>(self) -> ProtobufFieldIterator<Self, L>
    where
        Self: Sized,
        L: LenBuf,
        L::Alloc: Default,
    {
        self.read_protobuf_fields_in(L::Alloc::default())
    }

    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// `alloc` is cloned into each length-delimited field.
    fn read_protobuf_fields_in<L>(self, alloc: L::Alloc) -> ProtobufFieldIterator<Self, L>
    where
        Self: Sized,
        L: LenBuf;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_fields_in<L>(self, alloc: L::Alloc) -> ProtobufFieldIterator<Self, L>
    where
        Self: Sized,
        L: LenBuf,
    {
        ProtobufFieldIterator {
            reader: self,
            alloc,
        }
    }
}

// ============================================================================
// AsRefExtProtobuf
// ============================================================================

/// Helper struct implementing FieldValueReader for slice-based readers
pub(crate) struct SliceReader<'a, 'b> {
    pub(crate) slice: &'b mut &'a [u8],
}

impl<'a, 'b> FieldValueReader<&'a [u8]> for SliceReader<'a, 'b> {
    fn read_varint(&mut self) -> Result<Option<Varint>> {
        use crate::varint::ReadExtVarint;
        self.slice.read_varint()
    }

    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let (bytes, remaining) = self
            .slice
            .split_first_chunk::<N>()
            .ok_or(ProtobufError::UnexpectedEof)?;
        *self.slice = remaining;
        Ok(*bytes)
    }

    fn read_length_delimited(&mut self, length: usize) -> Result<&'a [u8]> {
        let (value_slice, remaining) = self
            .slice
            .split_at_checked(length)
            .ok_or(ProtobufError::UnexpectedEof)?;
        *self.slice = remaining;
        Ok(value_slice)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a slice
pub struct ProtobufFieldSliceIterator<'a> {
    slice: &'a [u8],
}

impl<'a> Iterator for ProtobufFieldSliceIterator<'a> {
    type Item = Result<Field<&'a [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = SliceReader {
            slice: &mut self.slice,
        };
        parse_field(&mut reader).transpose()
    }
}

/// Extension trait for reading raw Protocol Buffer fields from slice types
///
/// This trait provides low-level utilities for reading field-by-field from a byte slice.
/// It returns borrowed references (`Field<&'a [u8]>`) suitable for slice sources.
/// It does not provide semantic interpretation - that is the caller's responsibility.
///
/// # Example
/// ```
/// use ::protobuf_core::AsRefExtProtobuf;
///
/// let slice = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
///
/// for field_result in slice.read_protobuf_fields() {
///     let field = field_result?;
///     // Process field...
/// }
/// # Ok::<(), protobuf_core::ProtobufError>(())
/// ```
pub trait AsRefExtProtobuf: AsRef<[u8]> {
    /// Read raw protobuf fields from the slice, returning an iterator
    ///
    /// This returns an iterator that yields fields sequentially.
    /// Each field contains raw bytes that must be interpreted by the caller.
    /// Returns borrowed references (`Field<&'a [u8]>`).
    ///
    /// This method is available for all types implementing `AsRef<[u8]>`.
    fn read_protobuf_fields(&self) -> ProtobufFieldSliceIterator<'_> {
        ProtobufFieldSliceIterator {
            slice: self.as_ref(),
        }
    }
}

impl<T> AsRefExtProtobuf for T where T: AsRef<[u8]> + ?Sized {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
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
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields::<Vec<u8>>()
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
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields::<Vec<u8>>()
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
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields::<Vec<u8>>()
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
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields::<Vec<u8>>()
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
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields::<Vec<u8>>()
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
            .read_protobuf_fields::<Vec<u8>>()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 0);
    }

    // ============================================================================
    // IteratorExtProtobuf tests
    // ============================================================================

    #[test]
    fn test_iterator_ext_read_varint_field() {
        use super::IteratorExtProtobuf;

        let bytes = vec![0x08, 0x96, 0x01]; // field 1: 150
        let iter = bytes.into_iter();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_iterator_ext_read_multiple_fields() {
        use super::IteratorExtProtobuf;

        let bytes = vec![
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ];
        let iter = bytes.into_iter();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_iterator_ext_read_len_field() {
        use super::IteratorExtProtobuf;

        let bytes = vec![0x12, 0x03, 0x48, 0x65, 0x6c]; // field 2: "Hel"
        let iter = bytes.into_iter();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_iterator_ext_read_i32_field() {
        use super::IteratorExtProtobuf;

        let bytes = vec![0x15, 0x78, 0x56, 0x34, 0x12]; // field 2: 0x12345678 (Fixed32)
        let iter = bytes.into_iter();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_iterator_ext_read_i64_field() {
        use super::IteratorExtProtobuf;

        let bytes = vec![0x19, 0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]; // field 3: 0x1234567890ABCDEF (Fixed64)
        let iter = bytes.into_iter();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_iterator_ext_read_empty_stream() {
        use super::IteratorExtProtobuf;

        let bytes = vec![];
        let iter = bytes.into_iter();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 0);
    }

    // ============================================================================
    // TryIteratorExtProtobuf tests
    // ============================================================================

    #[test]
    fn test_try_iterator_ext_read_varint_field() {
        use super::TryIteratorExtProtobuf;
        use ::std::io::{Cursor, Read};

        let data = vec![0x08, 0x96, 0x01]; // field 1: 150
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_try_iterator_ext_read_multiple_fields() {
        use super::TryIteratorExtProtobuf;
        use ::std::io::{Cursor, Read};

        let data = vec![
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ];
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_try_iterator_ext_read_len_field() {
        use super::TryIteratorExtProtobuf;
        use ::std::io::{Cursor, Read};

        let data = vec![0x12, 0x03, 0x48, 0x65, 0x6c]; // field 2: "Hel"
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_try_iterator_ext_read_i32_field() {
        use super::TryIteratorExtProtobuf;
        use ::std::io::{Cursor, Read};

        let data = vec![0x15, 0x78, 0x56, 0x34, 0x12]; // field 2: 0x12345678 (Fixed32)
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_try_iterator_ext_read_i64_field() {
        use super::TryIteratorExtProtobuf;
        use ::std::io::{Cursor, Read};

        let data = vec![0x19, 0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12]; // field 3: 0x1234567890ABCDEF (Fixed64)
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
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
    fn test_try_iterator_ext_read_empty_stream() {
        use super::TryIteratorExtProtobuf;
        use ::std::io::{Cursor, Read};

        let data = vec![];
        let reader = Cursor::new(data);
        let iter = reader.bytes();
        let fields: Vec<_> = iter
            .protobuf_fields::<Vec<u8>>()
            .collect::<::std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(fields.len(), 0);
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

        let truncated_i32 = (&[0x0D, 0x01, 0x02, 0x03][..])
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_i32, Err(ProtobufError::UnexpectedEof)));

        let truncated_i64 = (&[0x09, 0, 0, 0, 0, 0, 0, 0][..])
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_i64, Err(ProtobufError::UnexpectedEof)));

        let truncated_len = (&[0x0A, 0x05, b'a', b'b'][..])
            .read_protobuf_fields()
            .next()
            .unwrap();
        assert!(matches!(truncated_len, Err(ProtobufError::UnexpectedEof)));

        let length_past_u32 = (&[0x0A, 0x80, 0x80, 0x80, 0x80, 0x10][..])
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

        // Wire types 3 and 4 are groups. The reader rejects them.
        for (byte, wire) in [(0x0Bu8, 3u8), (0x0C, 4)] {
            let bytes = [byte];
            let value = match (&bytes[..]).read_protobuf_fields().next().unwrap() {
                Err(ProtobufError::InvalidWireType { value }) => value,
                other => panic!("expected InvalidWireType, got {other:?}"),
            };
            assert_eq!(value, wire);
        }
    }
}
