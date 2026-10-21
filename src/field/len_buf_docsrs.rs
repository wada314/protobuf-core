// Included from `len_buf.rs` only when the private `_docsrs` feature is enabled.
// `default fn` lets `min_specialization` keep this impl next to `impl LenBuf for Vec<u8>`.
// Do not paste this keyword into `len_buf.rs`: stable rustc warns about it even inside `#[cfg]`.

/// Length-delimited payload stored in `Vec<u8, A>`.
///
/// Requires the `nightly` Cargo feature and a nightly compiler. Without that
/// feature a custom allocator cannot be used. `A` must implement
/// `std::alloc::Allocator` + `Default`. [`with_capacity`](LenBuf::with_capacity)
/// and the `from_*` methods construct the vector with `A::default()`.
///
/// `Vec<u8>` uses the implementation that does not require `nightly`. On a
/// nightly compiler `Vec<u8>` is `Vec<u8, Global>`, so this implementation
/// also accepts it when the feature is enabled.
#[doc(cfg(feature = "nightly"))]
#[allow(clippy::incompatible_msrv)]
impl<A> LenBuf for Vec<u8, A>
where
    A: Allocator + Default,
{
    default fn with_capacity(capacity: usize) -> Self {
        Vec::with_capacity_in(capacity, A::default())
    }

    default fn reserve(&mut self, additional: usize) {
        Vec::reserve(self, additional);
    }

    default fn append_slice(&mut self, data: &[u8]) {
        self.extend_from_slice(data);
    }

    default fn from_read<R>(reader: &mut R, len: usize) -> Result<Self>
    where
        R: Read,
    {
        // `read_exact` needs an initialized buffer. `A` is the vector's allocator,
        // so this cannot go through the `vec!` macro (that uses `Global`).
        let mut data = Vec::with_capacity_in(len, A::default());
        data.resize(len, 0);
        reader.read_exact(&mut data)?;
        Ok(data)
    }

    default fn from_iter<I>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = u8>,
    {
        let mut data = Vec::with_capacity_in(len, A::default());
        for _ in 0..len {
            data.push(iter.next().ok_or(ProtobufError::UnexpectedEof)?);
        }
        Ok(data)
    }

    default fn from_try_iter<I, E>(iter: &mut I, len: usize) -> Result<Self>
    where
        I: Iterator<Item = ::std::result::Result<u8, E>>,
        E: Into<ProtobufError>,
    {
        let mut data = Vec::with_capacity_in(len, A::default());
        for _ in 0..len {
            data.push(
                iter.next()
                    .ok_or(ProtobufError::UnexpectedEof)?
                    .map_err(Into::into)?,
            );
        }
        Ok(data)
    }
}
