//! Field reading utilities for Protocol Buffers
//!
//! This module provides low-level utilities for reading raw protobuf fields from various sources.
//! Streaming sources (`std::io::Read` and byte iterators) build each length-delimited
//! payload with a closure `FnMut(usize) -> Result<L>`. Slice sources borrow them as `&[u8]`.

use crate::field::{Field, FieldValue};
use crate::tag::Tag;
use crate::varint::{DecodeOutcome, Varint};
use crate::wire_format::WireType;
use crate::{ProtobufError, Result};
use ::std::convert::{AsMut, AsRef};
use ::std::io::Read;

/// `Vec<u8>` payload of `len` initialized bytes.
///
/// [`ReadExtProtobuf::read_protobuf_fields`] and [`protobuf_fields`](IteratorExtProtobuf::protobuf_fields)
/// pass this function. Its type is `fn(usize) -> Result<Vec<u8>>`.
fn vec_payload(len: usize) -> Result<Vec<u8>> {
    Ok(vec![0u8; len])
}

/// Call `make` for one length-delimited payload.
///
/// `make` returns at least `length` initialized bytes. The reader overwrites the
/// first `length` bytes and leaves any suffix as the closure initialized it.
/// A shorter buffer panics.
fn make_payload<L, F>(make: &mut F, length: usize) -> Result<L>
where
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
{
    let mut payload = make(length)?;
    let got = payload.as_mut().len();
    assert!(
        got >= length,
        "length-delimited buffer length {got} is shorter than payload length {length}",
    );
    Ok(payload)
}

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
/// `L` is the length-delimited payload. Streaming readers build it with the
/// caller's closure. The slice reader uses `&'a [u8]`.
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
struct IteratorReader<'a, I, F>(&'a mut I, &'a mut F);

impl<'a, I, F, L> FieldValueReader<L> for IteratorReader<'a, I, F>
where
    I: Iterator<Item = u8>,
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
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
        let mut payload = make_payload(self.1, length)?;
        for byte in &mut payload.as_mut()[..length] {
            *byte = self.0.next().ok_or(ProtobufError::UnexpectedEof)?;
        }
        Ok(payload)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a byte iterator.
///
/// `F` builds each length-delimited payload. It defaults to `fn(usize) -> Result<Vec<u8>>`.
/// `F` is called once per length-delimited field.
pub struct FieldIteratorFromBytes<I, F = fn(usize) -> Result<Vec<u8>>> {
    iter: I,
    make: F,
}

impl<I, F, L> Iterator for FieldIteratorFromBytes<I, F>
where
    I: Iterator<Item = u8>,
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = IteratorReader(&mut self.iter, &mut self.make);
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
/// any iterator that yields bytes. Length-delimited payloads default to `Vec<u8>`.
/// [`protobuf_fields_with`](IteratorExtProtobuf::protobuf_fields_with) supplies a different buffer.
///
/// # Example
/// ```
/// use ::protobuf_core::IteratorExtProtobuf;
///
/// let bytes = vec![0x08, 0x96, 0x01]; // field 1: 150
/// let iter = bytes.into_iter();
/// let fields: Vec<_> = iter.protobuf_fields().collect::<::std::result::Result<Vec<_>, _>>().unwrap();
/// assert_eq!(fields[0].field_number.as_u32(), 1);
/// ```
pub trait IteratorExtProtobuf {
    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// Returns an iterator that yields `Result<Field<Vec<u8>>>`.
    /// Each field is parsed sequentially. Length-delimited payloads are `Vec<u8>`.
    fn protobuf_fields(self) -> FieldIteratorFromBytes<Self>
    where
        Self: Sized,
    {
        self.protobuf_fields_with(vec_payload)
    }

    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// `make` is called once for each length-delimited field. `len` is the payload
    /// length from the wire. Return [`Ok`](Result::Ok) with at least that many
    /// initialized bytes. The reader overwrites the first `len` bytes and leaves
    /// any suffix as `make` initialized it. A buffer shorter than `len` panics.
    /// Return [`Err`](Result::Err) to fail the field. Varint and fixed-width
    /// fields do not call `make`.
    ///
    /// If the payload bytes run out, the buffer is dropped and the reader returns
    /// [`UnexpectedEof`](ProtobufError::UnexpectedEof). Capture an allocator in
    /// `make` when the buffer needs one.
    fn protobuf_fields_with<L, F>(self, make: F) -> FieldIteratorFromBytes<Self, F>
    where
        Self: Sized,
        F: FnMut(usize) -> Result<L>,
        L: AsMut<[u8]>;
}

impl<I> IteratorExtProtobuf for I
where
    I: Iterator<Item = u8>,
{
    fn protobuf_fields_with<L, F>(self, make: F) -> FieldIteratorFromBytes<Self, F>
    where
        Self: Sized,
        F: FnMut(usize) -> Result<L>,
        L: AsMut<[u8]>,
    {
        FieldIteratorFromBytes { iter: self, make }
    }
}

// ============================================================================
// TryIteratorExtProtobuf
// ============================================================================

/// Helper struct implementing FieldValueReader for Iterator<Item = Result<u8, E>>
struct TryIteratorReader<'a, I, F>(&'a mut I, &'a mut F);

impl<'a, I, E, F, L> FieldValueReader<L> for TryIteratorReader<'a, I, F>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
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
        let mut payload = make_payload(self.1, length)?;
        for byte in &mut payload.as_mut()[..length] {
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
/// `F` builds each length-delimited payload. It defaults to `fn(usize) -> Result<Vec<u8>>`.
/// `F` is called once per length-delimited field.
pub struct FieldIteratorFromTryBytes<I, F = fn(usize) -> Result<Vec<u8>>> {
    iter: I,
    make: F,
}

impl<I, E, F, L> Iterator for FieldIteratorFromTryBytes<I, F>
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = TryIteratorReader(&mut self.iter, &mut self.make);
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
/// from I/O operations. Length-delimited payloads default to `Vec<u8>`.
/// [`protobuf_fields_with`](TryIteratorExtProtobuf::protobuf_fields_with) supplies a different buffer.
///
/// # Example
/// ```
/// use ::std::io::{Cursor, Read};
/// use ::protobuf_core::TryIteratorExtProtobuf;
///
/// let data = vec![0x08, 0x96, 0x01]; // field 1: 150
/// let mut reader = Cursor::new(data);
/// let iter = reader.bytes(); // Iterator<Item = Result<u8, io::Error>>
/// let fields: Vec<_> = iter.protobuf_fields().collect::<Result<Vec<_>, _>>().unwrap();
/// assert_eq!(fields[0].field_number.as_u32(), 1);
/// ```
pub trait TryIteratorExtProtobuf {
    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// Returns an iterator that yields `Result<Field<Vec<u8>>>`.
    /// Length-delimited payloads are `Vec<u8>`.
    /// I/O errors from the underlying iterator are propagated.
    fn protobuf_fields(self) -> FieldIteratorFromTryBytes<Self>
    where
        Self: Sized,
    {
        self.protobuf_fields_with(vec_payload)
    }

    /// Convert this iterator into an iterator of protobuf fields.
    ///
    /// `make` is called once for each length-delimited field. `len` is the payload
    /// length from the wire. Return [`Ok`](Result::Ok) with at least that many
    /// initialized bytes. The reader overwrites the first `len` bytes and leaves
    /// any suffix as `make` initialized it. A buffer shorter than `len` panics.
    /// Return [`Err`](Result::Err) to fail the field. Varint and fixed-width
    /// fields do not call `make`.
    ///
    /// If the payload bytes run out, the buffer is dropped and the reader returns
    /// [`UnexpectedEof`](ProtobufError::UnexpectedEof). Capture an allocator in
    /// `make` when the buffer needs one.
    fn protobuf_fields_with<L, F>(self, make: F) -> FieldIteratorFromTryBytes<Self, F>
    where
        Self: Sized,
        F: FnMut(usize) -> Result<L>,
        L: AsMut<[u8]>;
}

impl<I, E> TryIteratorExtProtobuf for I
where
    I: Iterator<Item = ::std::result::Result<u8, E>>,
    E: Into<ProtobufError>,
{
    fn protobuf_fields_with<L, F>(self, make: F) -> FieldIteratorFromTryBytes<Self, F>
    where
        Self: Sized,
        F: FnMut(usize) -> Result<L>,
        L: AsMut<[u8]>,
    {
        FieldIteratorFromTryBytes { iter: self, make }
    }
}

// ============================================================================
// ReadExtProtobuf
// ============================================================================

/// Helper struct implementing FieldValueReader for Read
struct ReadReader<'a, R, F>(&'a mut R, &'a mut F);

impl<'a, R, F, L> FieldValueReader<L> for ReadReader<'a, R, F>
where
    R: Read,
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
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
        let mut payload = make_payload(self.1, length)?;
        self.0.read_exact(&mut payload.as_mut()[..length])?;
        Ok(payload)
    }
}

/// Iterator for reading raw protobuf fields sequentially from a reader.
///
/// `F` builds each length-delimited payload. It defaults to `fn(usize) -> Result<Vec<u8>>`.
/// `F` is called once per length-delimited field.
pub struct FieldIteratorFromRead<R, F = fn(usize) -> Result<Vec<u8>>> {
    reader: R,
    make: F,
}

impl<R, F, L> Iterator for FieldIteratorFromRead<R, F>
where
    R: Read,
    F: FnMut(usize) -> Result<L>,
    L: AsMut<[u8]>,
{
    type Item = Result<Field<L>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = ReadReader(&mut self.reader, &mut self.make);
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
/// Length-delimited payloads default to `Vec<u8>`.
/// [`read_protobuf_fields_with`](ReadExtProtobuf::read_protobuf_fields_with) supplies a different buffer.
/// It does not provide semantic interpretation - that is the caller's responsibility.
///
/// # Example
/// ```
/// use ::protobuf_core::{ReadExtProtobuf, Field, FieldValue};
///
/// fn main() -> Result<(), Box<dyn ::std::error::Error>> {
///     let reader = &[0x08, 0x96, 0x01, 0x12, 0x03, 0x48, 0x65, 0x6c][..];
///
///     for field in reader.read_protobuf_fields() {
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
    /// Length-delimited payloads are `Vec<u8>`.
    fn read_protobuf_fields(self) -> FieldIteratorFromRead<Self>
    where
        Self: Sized,
    {
        self.read_protobuf_fields_with(vec_payload)
    }

    /// Read raw protobuf fields from the reader, returning an iterator.
    ///
    /// `make` is called once for each length-delimited field. `len` is the payload
    /// length from the wire. Return [`Ok`](Result::Ok) with at least that many
    /// initialized bytes. The reader overwrites the first `len` bytes and leaves
    /// any suffix as `make` initialized it. A buffer shorter than `len` panics.
    /// Return [`Err`](Result::Err) to fail the field. Varint and fixed-width
    /// fields do not call `make`.
    ///
    /// If the payload bytes run out, the buffer is dropped and the reader returns
    /// [`UnexpectedEof`](ProtobufError::UnexpectedEof). Capture an allocator in
    /// `make` when the buffer needs one.
    ///
    /// # Examples
    ///
    /// ```
    /// use ::protobuf_core::{FieldValue, ReadExtProtobuf};
    ///
    /// let reader = &[0x0A, 0x03, b'H', b'e', b'l'][..];
    /// let fields: Vec<_> = reader
    ///     .read_protobuf_fields_with(|len| Ok(vec![0u8; len].into_boxed_slice()))
    ///     .collect::<Result<Vec<_>, _>>()
    ///     .unwrap();
    /// let FieldValue::Len(data) = &fields[0].value else {
    ///     panic!("expected a length-delimited field");
    /// };
    /// assert_eq!(&data[..], b"Hel");
    /// ```
    fn read_protobuf_fields_with<L, F>(self, make: F) -> FieldIteratorFromRead<Self, F>
    where
        Self: Sized,
        F: FnMut(usize) -> Result<L>,
        L: AsMut<[u8]>;
}

impl<R> ReadExtProtobuf for R
where
    R: Read,
{
    fn read_protobuf_fields_with<L, F>(self, make: F) -> FieldIteratorFromRead<Self, F>
    where
        Self: Sized,
        F: FnMut(usize) -> Result<L>,
        L: AsMut<[u8]>,
    {
        FieldIteratorFromRead { reader: self, make }
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

/// Iterator for reading raw protobuf fields sequentially from a byte slice.
pub struct FieldIteratorFromSlice<'a> {
    slice: &'a [u8],
}

impl<'a> Iterator for FieldIteratorFromSlice<'a> {
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
    fn read_protobuf_fields(&self) -> FieldIteratorFromSlice<'_> {
        FieldIteratorFromSlice {
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
    fn test_read_ext_read_multiple_fields() {
        use super::ReadExtProtobuf;
        use ::std::io::Cursor;

        let data = vec![
            0x08, 0x96, 0x01, // field 1: 150
            0x12, 0x03, 0x48, 0x65, 0x6c, // field 2: "Hel"
        ];
        let reader = Cursor::new(data);
        let fields: Vec<_> = reader
            .read_protobuf_fields()
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
            .read_protobuf_fields()
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
            .read_protobuf_fields()
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
            .read_protobuf_fields()
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
            .read_protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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
            .protobuf_fields()
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

#[cfg(test)]
mod len_payload {
    use super::{IteratorExtProtobuf, ReadExtProtobuf, TryIteratorExtProtobuf};
    use crate::varint::Varint;
    use crate::{Field, FieldValue, ProtobufError};
    use ::std::convert::AsMut;
    use ::std::io::{Cursor, ErrorKind, Read};

    const LARGE_LEN: usize = 8200;

    #[derive(Debug, PartialEq, Eq)]
    struct Bytes(Vec<u8>);

    impl AsMut<[u8]> for Bytes {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }
    }

    fn bytes_buf(len: usize) -> crate::Result<Bytes> {
        Ok(Bytes(vec![0; len]))
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
        let owned: Vec<u8> = field_bytes.to_vec();

        let from_read = Cursor::new(owned.clone())
            .read_protobuf_fields_with(bytes_buf)
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(Bytes(data)) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);

        let from_iter = owned
            .clone()
            .into_iter()
            .protobuf_fields_with(bytes_buf)
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(Bytes(data)) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(data, expected);

        let from_try = Cursor::new(owned)
            .bytes()
            .protobuf_fields_with(bytes_buf)
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(Bytes(data)) = from_try.value else {
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
    fn short_payload_is_an_error_on_every_streaming_source() {
        let input = [0x0A, 0x05, b'a', b'b'];
        // `Read::read_exact` reports a short buffer as an I/O error.
        let error = Cursor::new(input)
            .read_protobuf_fields_with(bytes_buf)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(
            error,
            ProtobufError::IoError(ref err) if err.kind() == ErrorKind::UnexpectedEof
        ));

        let error = input
            .into_iter()
            .protobuf_fields_with(bytes_buf)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));

        let error = Cursor::new(input)
            .bytes()
            .protobuf_fields_with(bytes_buf)
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    fn assert_box(message: &[u8], expected: &[u8]) {
        let from_read = Cursor::new(message.to_vec())
            .read_protobuf_fields_with(|len| Ok(vec![0u8; len].into_boxed_slice()))
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_read.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], expected);

        let from_iter = message
            .iter()
            .copied()
            .protobuf_fields_with(|len| Ok(vec![0u8; len].into_boxed_slice()))
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_iter.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], expected);

        let from_try = Cursor::new(message.to_vec())
            .bytes()
            .protobuf_fields_with(|len| Ok(vec![0u8; len].into_boxed_slice()))
            .next()
            .unwrap()
            .unwrap();
        let FieldValue::Len(data) = from_try.value else {
            panic!("expected len");
        };
        assert_eq!(&data[..], expected);
    }

    #[test]
    fn box_closure_reads_payload_on_every_streaming_source() {
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

    impl AsMut<[u8]> for Tagged {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.data
        }
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

        let from_read = Cursor::new(message.clone())
            .read_protobuf_fields_with(|len| {
                Ok(Tagged {
                    id,
                    data: vec![0; len],
                })
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_tagged(&from_read, id);

        let from_iter = message
            .clone()
            .into_iter()
            .protobuf_fields_with(|len| {
                Ok(Tagged {
                    id,
                    data: vec![0; len],
                })
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_tagged(&from_iter, id);

        let from_try = Cursor::new(message)
            .bytes()
            .protobuf_fields_with(|len| {
                Ok(Tagged {
                    id,
                    data: vec![0; len],
                })
            })
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_tagged(&from_try, id);
    }

    #[test]
    fn closure_runs_only_for_length_fields() {
        let mixed = [0x08, 0x96, 0x01, 0x12, 0x03, b'H', b'e', b'l'];
        let mut calls = 0usize;
        let fields = Cursor::new(mixed)
            .read_protobuf_fields_with(|len| {
                calls += 1;
                Ok(Bytes(vec![0; len]))
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
            .read_protobuf_fields_with(|_len| -> crate::Result<Vec<u8>> {
                Err(ProtobufError::UnexpectedEof)
            })
            .next()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, ProtobufError::UnexpectedEof));
    }

    #[derive(Debug)]
    struct Spare {
        data: Vec<u8>,
    }

    impl AsMut<[u8]> for Spare {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.data
        }
    }

    fn spare(len: usize) -> crate::Result<Spare> {
        Ok(Spare {
            data: vec![0xAB; len + 4],
        })
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
            .read_protobuf_fields_with(spare)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_spare(&from_read);

        let from_iter = message
            .into_iter()
            .protobuf_fields_with(spare)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_spare(&from_iter);

        let from_try = Cursor::new(message)
            .bytes()
            .protobuf_fields_with(spare)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_spare(&from_try);
    }

    #[test]
    #[should_panic(expected = "shorter than payload length")]
    fn short_buffer_panics_on_read() {
        let input = [0x0A, 0x03, b'H', b'e', b'l'];
        let _ = Cursor::new(input)
            .read_protobuf_fields_with(|len| Ok(Vec::<u8>::with_capacity(len)))
            .next();
    }

    #[test]
    #[should_panic(expected = "shorter than payload length")]
    fn short_buffer_panics_on_byte_iterator() {
        let input = [0x0A, 0x03, b'H', b'e', b'l'];
        let _ = input
            .into_iter()
            .protobuf_fields_with(|len| Ok(Vec::<u8>::with_capacity(len)))
            .next();
    }

    #[test]
    #[should_panic(expected = "shorter than payload length")]
    fn short_buffer_panics_on_try_iterator() {
        let input = [0x0A, 0x03, b'H', b'e', b'l'];
        let _ = Cursor::new(input)
            .bytes()
            .protobuf_fields_with(|len| Ok(Vec::<u8>::with_capacity(len)))
            .next();
    }
}
