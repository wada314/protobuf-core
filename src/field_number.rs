use std::convert::TryFrom;
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
    pub fn try_new(value: u32) -> Result<Self, FieldNumberError> {
        if value < Self::MIN.0 || value > Self::MAX.0 {
            return Err(FieldNumberError::OutOfRange(value));
        }
        Ok(Self(value))
    }

    /// Checks if the field number can be encoded as a single-byte tag.
    /// Field numbers 1-15 result in tags that fit in 1 byte.
    pub fn is_tag_single_byte(&self) -> bool {
        self.0 <= Self::MAX_SINGLE_BYTE_TAG
    }

    /// Checks if the field number is in a reserved range.
    /// Protobuf reserves field numbers 19000-19999.
    pub fn is_reserved(&self) -> bool {
        self.0 >= Self::RESERVED_RANGE_START && self.0 <= Self::RESERVED_RANGE_END
    }

    /// Checks if the field number is in a specific range.
    pub fn is_in_range(&self, min: u32, max: u32) -> bool {
        self.0 >= min && self.0 <= max
    }

    /// Returns the minimum number of bytes needed to encode the tag (field_number + wire_type) as a varint.
    ///
    /// The tag is encoded as: (field_number << 3) | wire_type
    /// Since wire_type is 0-7, the maximum tag value is (field_number * 8) + 7
    pub fn encoded_size(&self) -> usize {
        let max_tag_value = (self.0 << 3) | 7; // field_number << 3 + max wire_type (7)

        match max_tag_value {
            0..=0x7F => 1,
            0x80..=0x3FFF => 2,
            0x4000..=0x1FFFFF => 3,
            0x200000..=0xFFFFFFF => 4,
            _ => 5,
        }
    }

    /// Returns the field number as a u32.
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    /// Returns the field number as a usize.
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<u32> for FieldNumber {
    type Error = FieldNumberError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<FieldNumber> for u32 {
    fn from(field_number: FieldNumber) -> Self {
        field_number.0
    }
}

impl From<FieldNumber> for usize {
    fn from(field_number: FieldNumber) -> Self {
        field_number.0 as usize
    }
}

/// Error type for field number validation.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum FieldNumberError {
    #[error("Field number {0} is out of range (must be between 1 and 536870911)")]
    OutOfRange(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_number_creation() {
        assert!(FieldNumber::try_new(1).is_ok());
        assert!(FieldNumber::try_new(16).is_ok());
        assert!(FieldNumber::try_new(536_870_911).is_ok());

        assert!(FieldNumber::try_new(0).is_err());
        assert!(FieldNumber::try_new(536_870_912).is_err());
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
        assert_eq!(FieldNumber::try_from(1), Ok(FieldNumber(1)));
        assert_eq!(
            FieldNumber::try_from(0),
            Err(FieldNumberError::OutOfRange(0))
        );
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
    fn test_encoded_size() {
        // Field 1: (1 << 3) | 7 = 8 | 7 = 15 (1 byte)
        assert_eq!(FieldNumber::try_new(1).unwrap().encoded_size(), 1);

        // Field 16: (16 << 3) | 7 = 128 | 7 = 135 (2 bytes)
        assert_eq!(FieldNumber::try_new(16).unwrap().encoded_size(), 2);

        // Field 100: (100 << 3) | 7 = 800 | 7 = 807 (2 bytes)
        assert_eq!(FieldNumber::try_new(100).unwrap().encoded_size(), 2);

        // Field 10000: (10000 << 3) | 7 = 80000 | 7 = 80007 (3 bytes)
        assert_eq!(FieldNumber::try_new(10000).unwrap().encoded_size(), 3);

        // Field 1000000: (1000000 << 3) | 7 = 8000000 | 7 = 8000007 (4 bytes)
        assert_eq!(FieldNumber::try_new(1000000).unwrap().encoded_size(), 4);

        // Field 100000000: (100000000 << 3) | 7 = 800000000 | 7 = 800000007 (5 bytes)
        assert_eq!(FieldNumber::try_new(100000000).unwrap().encoded_size(), 5);
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
}
