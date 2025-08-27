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

    /// Creates a new field number, validating the range.
    pub fn new(value: u32) -> Result<Self, FieldNumberError> {
        if value < Self::MIN.0 || value > Self::MAX.0 {
            return Err(FieldNumberError::OutOfRange(value));
        }
        Ok(Self(value))
    }

    /// Returns the underlying u32 value.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Checks if the field number is in the range [1, 16].
    /// This is useful for determining encoded size.
    pub fn is_small(self) -> bool {
        self.0 <= 16
    }

    /// Checks if the field number is in a reserved range.
    /// Protobuf reserves field numbers 19000-19999.
    pub fn is_reserved(self) -> bool {
        self.0 >= 19000 && self.0 <= 19999
    }

    /// Checks if the field number is in a specific range.
    pub fn is_in_range(self, min: u32, max: u32) -> bool {
        self.0 >= min && self.0 <= max
    }

    /// Returns the minimum number of bytes needed to encode the tag (field_number + wire_type) as a varint.
    ///
    /// The tag is encoded as: (field_number << 3) | wire_type
    /// Since wire_type is 0-7, the maximum tag value is (field_number * 8) + 7
    pub fn tag_encoded_size(self) -> usize {
        let max_tag_value = (self.0 << 3) | 7; // field_number << 3 + max wire_type (7)

        if max_tag_value <= 0x7F {
            1
        } else if max_tag_value <= 0x3FFF {
            2
        } else if max_tag_value <= 0x1FFFFF {
            3
        } else if max_tag_value <= 0xFFFFFFF {
            4
        } else {
            5
        }
    }

    /// Returns the encoded size of a tag with this field number and the given wire type.
    pub fn tag_size_with_wire_type(self, wire_type: crate::wire_format::WireType) -> usize {
        let tag_value = (self.0 << 3) | (wire_type as u32);

        if tag_value <= 0x7F {
            1
        } else if tag_value <= 0x3FFF {
            2
        } else if tag_value <= 0x1FFFFF {
            3
        } else if tag_value <= 0xFFFFFFF {
            4
        } else {
            5
        }
    }

    /// Returns the minimum number of bytes needed to encode this field number as a varint.
    ///
    /// Note: This is rarely useful in practice since field numbers are always encoded as part of a tag.
    pub fn encoded_size(self) -> usize {
        if self.0 <= 0x7F {
            1
        } else if self.0 <= 0x3FFF {
            2
        } else if self.0 <= 0x1FFFFF {
            3
        } else if self.0 <= 0xFFFFFFF {
            4
        } else {
            5
        }
    }

    /// Checks if this field number is commonly used (1-16).
    /// These field numbers are most efficient to encode.
    pub fn is_common(self) -> bool {
        self.0 <= 16
    }
}

impl TryFrom<u32> for FieldNumber {
    type Error = FieldNumberError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<FieldNumber> for u32 {
    fn from(field_number: FieldNumber) -> Self {
        field_number.get()
    }
}

impl From<FieldNumber> for usize {
    fn from(field_number: FieldNumber) -> Self {
        field_number.get() as usize
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
        assert!(FieldNumber::new(1).is_ok());
        assert!(FieldNumber::new(16).is_ok());
        assert!(FieldNumber::new(536_870_911).is_ok());

        assert!(FieldNumber::new(0).is_err());
        assert!(FieldNumber::new(536_870_912).is_err());
    }

    #[test]
    fn test_field_number_conversion() {
        let field = FieldNumber::new(42).unwrap();
        assert_eq!(u32::from(field), 42);
        assert_eq!(usize::from(field), 42);
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
        let small_field = FieldNumber::new(16).unwrap();
        let large_field = FieldNumber::new(1000).unwrap();
        let reserved_field = FieldNumber::new(19500).unwrap();

        assert!(small_field.is_small());
        assert!(!large_field.is_small());

        assert!(small_field.is_common());
        assert!(!large_field.is_common());

        assert!(reserved_field.is_reserved());
        assert!(!small_field.is_reserved());

        assert!(small_field.is_in_range(1, 20));
        assert!(!small_field.is_in_range(20, 30));
    }

    #[test]
    fn test_tag_encoded_size() {
        // Field 1: (1 << 3) | 7 = 8 | 7 = 15 (1 byte)
        assert_eq!(FieldNumber::new(1).unwrap().tag_encoded_size(), 1);

        // Field 16: (16 << 3) | 7 = 128 | 7 = 135 (2 bytes)
        assert_eq!(FieldNumber::new(16).unwrap().tag_encoded_size(), 2);

        // Field 100: (100 << 3) | 7 = 800 | 7 = 807 (2 bytes)
        assert_eq!(FieldNumber::new(100).unwrap().tag_encoded_size(), 2);

        // Field 10000: (10000 << 3) | 7 = 80000 | 7 = 80007 (3 bytes)
        assert_eq!(FieldNumber::new(10000).unwrap().tag_encoded_size(), 3);

        // Field 1000000: (1000000 << 3) | 7 = 8000000 | 7 = 8000007 (4 bytes)
        assert_eq!(FieldNumber::new(1000000).unwrap().tag_encoded_size(), 4);

        // Field 100000000: (100000000 << 3) | 7 = 800000000 | 7 = 800000007 (5 bytes)
        assert_eq!(FieldNumber::new(100000000).unwrap().tag_encoded_size(), 5);
    }

    #[test]
    fn test_tag_size_with_wire_type() {
        use crate::wire_format::WireType;

        let field_1 = FieldNumber::new(1).unwrap();
        let field_16 = FieldNumber::new(16).unwrap();

        // Field 1 with Varint (0): (1 << 3) | 0 = 8 (1 byte)
        assert_eq!(field_1.tag_size_with_wire_type(WireType::Varint), 1);

        // Field 1 with I64 (1): (1 << 3) | 1 = 9 (1 byte)
        assert_eq!(field_1.tag_size_with_wire_type(WireType::I64), 1);

        // Field 16 with Varint (0): (16 << 3) | 0 = 128 (2 bytes)
        assert_eq!(field_16.tag_size_with_wire_type(WireType::Varint), 2);

        // Field 16 with Len (2): (16 << 3) | 2 = 130 (2 bytes)
        assert_eq!(field_16.tag_size_with_wire_type(WireType::Len), 2);
    }

    #[test]
    fn test_encoded_size() {
        // These are the sizes for field numbers alone (without wire type)
        assert_eq!(FieldNumber::new(1).unwrap().encoded_size(), 1);
        assert_eq!(FieldNumber::new(16).unwrap().encoded_size(), 1);
        assert_eq!(FieldNumber::new(100).unwrap().encoded_size(), 1); // 100 = 0x64 (7 bits)
        assert_eq!(FieldNumber::new(10000).unwrap().encoded_size(), 2); // 10000 = 0x2710 (14 bits)
        assert_eq!(FieldNumber::new(1000000).unwrap().encoded_size(), 3); // 1000000 = 0xF4240 (20 bits)
        assert_eq!(FieldNumber::new(100000000).unwrap().encoded_size(), 4); // 100000000 = 0x5F5E100 (27 bits)
    }
}
