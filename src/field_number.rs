use ::std::convert::TryFrom;

/// A validated Protocol Buffers field number.
///
/// Field numbers must be in the range [1, 2^29 - 1].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldNumber(u32);

impl FieldNumber {
    /// Minimum allowed field number (1).
    pub const MIN: Self = Self(1);

    /// Maximum allowed field number (2^29 - 1 = 536,870,911).
    pub const MAX: Self = Self(536_870_911);

    /// Maximum field number that can be encoded as a single-byte tag (15).
    /// Field numbers 1-15 result in tags that fit in 1 byte.
    pub const MAX_SINGLE_BYTE_TAG: u32 = 15;

    /// Start of reserved field number range (19000).
    /// Protobuf reserves field numbers 19000-19999.
    pub const RESERVED_RANGE_START: u32 = 19000;

    /// End of reserved field number range (19999).
    /// Protobuf reserves field numbers 19000-19999.
    pub const RESERVED_RANGE_END: u32 = 19999;

    /// Creates a new field number, validating the range.
    ///
    /// On failure, returns the input `value` (a thin-wrapper style error).
    pub const fn try_new(value: u32) -> Result<Self, u32> {
        if value < Self::MIN.0 || value > Self::MAX.0 {
            return Err(value);
        }
        Ok(Self(value))
    }

    /// Checks if the field number can be encoded as a single-byte tag.
    /// Field numbers 1-15 result in tags that fit in 1 byte.
    pub fn is_tag_single_byte(&self) -> bool {
        self.0 <= Self::MAX_SINGLE_BYTE_TAG
    }

    /// Checks if the field number is in a reserved range.
    /// Protobuf reserves field numbers 19000-19999, inclusive.
    pub fn is_reserved(&self) -> bool {
        self.0 >= Self::RESERVED_RANGE_START && self.0 <= Self::RESERVED_RANGE_END
    }

    /// Checks if the field number is in a specific range, inclusive.
    pub fn is_in_range(&self, min: u32, max: u32) -> bool {
        self.0 >= min && self.0 <= max
    }

    /// Returns the field number as a `u32`.
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Returns the field number as a `i32`.
    pub const fn as_i32(self) -> i32 {
        self.0 as i32
    }

    /// Returns the field number as a `usize`.
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<u32> for FieldNumber {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<FieldNumber> for u32 {
    fn from(field_number: FieldNumber) -> Self {
        field_number.as_u32()
    }
}

impl TryFrom<i32> for FieldNumber {
    type Error = i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match u32::try_from(value) {
            Ok(v) => match Self::try_new(v) {
                Ok(n) => Ok(n),
                Err(_) => Err(value),
            },
            Err(_) => Err(value),
        }
    }
}

impl From<FieldNumber> for i32 {
    fn from(field_number: FieldNumber) -> Self {
        field_number.as_i32()
    }
}

impl From<FieldNumber> for usize {
    fn from(field_number: FieldNumber) -> Self {
        field_number.as_usize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_number_creation() {
        assert!(FieldNumber::try_new(1).is_ok());
        assert!(FieldNumber::try_new(16).is_ok());
        assert!(FieldNumber::try_new(536_870_911).is_ok());

        assert_eq!(FieldNumber::try_new(0), Err(0));
        assert_eq!(FieldNumber::try_new(536_870_912), Err(536_870_912));
    }

    #[test]
    fn test_field_number_conversion() {
        let field = FieldNumber::try_new(42).unwrap();
        assert_eq!(u32::from(field), 42);
        assert_eq!(usize::from(field), 42);
        assert_eq!(field.as_u32(), 42);
        assert_eq!(field.as_usize(), 42);
    }

    #[test]
    fn test_field_number_try_from() {
        let result = FieldNumber::try_from(1);
        assert!(result.is_ok());
        let field_number = result.unwrap();
        assert_eq!(field_number.as_u32(), 1);

        assert_eq!(FieldNumber::try_from(0), Err(0));
        assert_eq!(FieldNumber::try_from(-1), Err(-1));
    }

    #[test]
    fn test_helper_methods() {
        let single_byte_field = FieldNumber::try_new(15).unwrap();
        let multi_byte_field = FieldNumber::try_new(16).unwrap();
        let large_field = FieldNumber::try_new(1000).unwrap();
        let reserved_field = FieldNumber::try_new(19500).unwrap();

        assert!(single_byte_field.is_tag_single_byte());
        assert!(!multi_byte_field.is_tag_single_byte());
        assert!(!large_field.is_tag_single_byte());

        assert!(reserved_field.is_reserved());
        assert!(!single_byte_field.is_reserved());

        assert!(single_byte_field.is_in_range(1, 20));
        assert!(!single_byte_field.is_in_range(20, 30));
    }

    #[test]
    fn test_as_u32_and_as_usize() {
        let min_field = FieldNumber::MIN;
        let max_field = FieldNumber::MAX;
        let mid_field = FieldNumber::try_new(1000).unwrap();

        // Test as_u32
        assert_eq!(min_field.as_u32(), 1);
        assert_eq!(max_field.as_u32(), 536_870_911);
        assert_eq!(mid_field.as_u32(), 1000);

        // Test as_usize
        assert_eq!(min_field.as_usize(), 1);
        assert_eq!(max_field.as_usize(), 536_870_911);
        assert_eq!(mid_field.as_usize(), 1000);
    }

    #[test]
    fn test_field_number_range_edges() {
        assert_eq!(FieldNumber::MAX.as_i32(), 536_870_911);
        assert_eq!(i32::from(FieldNumber::MAX), 536_870_911);

        assert_eq!(FieldNumber::try_from(536_870_912i32), Err(536_870_912));
        assert_eq!(FieldNumber::try_from(i32::MIN), Err(i32::MIN));
        assert_eq!(
            FieldNumber::try_from(FieldNumber::MAX.as_u32()).unwrap(),
            FieldNumber::MAX
        );

        let below = FieldNumber::try_new(18_999).unwrap();
        let start = FieldNumber::try_new(19_000).unwrap();
        let end = FieldNumber::try_new(19_999).unwrap();
        let above = FieldNumber::try_new(20_000).unwrap();
        assert!(!below.is_reserved());
        assert!(start.is_reserved());
        assert!(end.is_reserved());
        assert!(!above.is_reserved());
    }
}
