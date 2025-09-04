//! Protocol Buffers descriptor definitions
//!
//! This module contains the data structures and constants defined in descriptor.proto

use crate::define_metadata;
use crate::descriptor::DescriptorEnum;
use std::io::Write;

// Wrapped types for type safety
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Edition(i32);

impl Edition {
    pub const UNKNOWN: Self = Self(0);
    pub const LEGACY: Self = Self(900);
    pub const PROTO2: Self = Self(998);
    pub const PROTO3: Self = Self(999);
    pub const EDITION_2023: Self = Self(1000);
    pub const EDITION_2024: Self = Self(1001);
    pub const EDITION_1_TEST_ONLY: Self = Self(1);
    pub const EDITION_2_TEST_ONLY: Self = Self(2);
    pub const EDITION_99997_TEST_ONLY: Self = Self(99997);
    pub const EDITION_99998_TEST_ONLY: Self = Self(99998);
    pub const EDITION_99999_TEST_ONLY: Self = Self(99999);
    pub const EDITION_MAX: Self = Self(0x7FFFFFFF);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 900 | 998 | 999 | 1000 | 1001)
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self.0, 998 | 999)
    }

    pub fn is_modern(&self) -> bool {
        matches!(self.0, 1000 | 1001)
    }
}

impl DescriptorEnum for Edition {
    fn value(&self) -> i32 {
        self.0
    }

    fn from_value(value: i32) -> Self {
        Self(value)
    }

    fn is_known(&self) -> bool {
        self.is_known()
    }
}

impl std::fmt::Debug for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Edition::UNKNOWN => write!(f, "Edition::UNKNOWN"),
            Edition::LEGACY => write!(f, "Edition::LEGACY"),
            Edition::PROTO2 => write!(f, "Edition::PROTO2"),
            Edition::PROTO3 => write!(f, "Edition::PROTO3"),
            Edition::EDITION_2023 => write!(f, "Edition::EDITION_2023"),
            Edition::EDITION_2024 => write!(f, "Edition::EDITION_2024"),
            Edition::EDITION_1_TEST_ONLY => write!(f, "Edition::EDITION_1_TEST_ONLY"),
            Edition::EDITION_2_TEST_ONLY => write!(f, "Edition::EDITION_2_TEST_ONLY"),
            Edition::EDITION_99997_TEST_ONLY => write!(f, "Edition::EDITION_99997_TEST_ONLY"),
            Edition::EDITION_99998_TEST_ONLY => write!(f, "Edition::EDITION_99998_TEST_ONLY"),
            Edition::EDITION_99999_TEST_ONLY => write!(f, "Edition::EDITION_99999_TEST_ONLY"),
            Edition::EDITION_MAX => write!(f, "Edition::EDITION_MAX"),
            _ => write!(f, "Edition({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldType(i32);

impl FieldType {
    pub const DOUBLE: Self = Self(1);
    pub const FLOAT: Self = Self(2);
    pub const INT64: Self = Self(3);
    pub const UINT64: Self = Self(4);
    pub const INT32: Self = Self(5);
    pub const FIXED64: Self = Self(6);
    pub const FIXED32: Self = Self(7);
    pub const BOOL: Self = Self(8);
    pub const STRING: Self = Self(9);
    pub const GROUP: Self = Self(10);
    pub const MESSAGE: Self = Self(11);
    pub const BYTES: Self = Self(12);
    pub const UINT32: Self = Self(13);
    pub const ENUM: Self = Self(14);
    pub const SFIXED32: Self = Self(15);
    pub const SFIXED64: Self = Self(16);
    pub const SINT32: Self = Self(17);
    pub const SINT64: Self = Self(18);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        (1..=18).contains(&self.0)
    }

    pub fn is_integer(&self) -> bool {
        matches!(self.0, 3 | 4 | 5 | 13 | 17 | 18)
    }

    pub fn is_fixed(&self) -> bool {
        matches!(self.0, 6 | 7 | 15 | 16)
    }
}

impl std::fmt::Debug for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            FieldType::DOUBLE => write!(f, "FieldType::DOUBLE"),
            FieldType::FLOAT => write!(f, "FieldType::FLOAT"),
            FieldType::INT64 => write!(f, "FieldType::INT64"),
            FieldType::UINT64 => write!(f, "FieldType::UINT64"),
            FieldType::INT32 => write!(f, "FieldType::INT32"),
            FieldType::FIXED64 => write!(f, "FieldType::FIXED64"),
            FieldType::FIXED32 => write!(f, "FieldType::FIXED32"),
            FieldType::BOOL => write!(f, "FieldType::BOOL"),
            FieldType::STRING => write!(f, "FieldType::STRING"),
            FieldType::GROUP => write!(f, "FieldType::GROUP"),
            FieldType::MESSAGE => write!(f, "FieldType::MESSAGE"),
            FieldType::BYTES => write!(f, "FieldType::BYTES"),
            FieldType::UINT32 => write!(f, "FieldType::UINT32"),
            FieldType::ENUM => write!(f, "FieldType::ENUM"),
            FieldType::SFIXED32 => write!(f, "FieldType::SFIXED32"),
            FieldType::SFIXED64 => write!(f, "FieldType::SFIXED64"),
            FieldType::SINT32 => write!(f, "FieldType::SINT32"),
            FieldType::SINT64 => write!(f, "FieldType::SINT64"),
            _ => write!(f, "FieldType({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldLabel(i32);

impl FieldLabel {
    pub const OPTIONAL: Self = Self(1);
    pub const REQUIRED: Self = Self(2);
    pub const REPEATED: Self = Self(3);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 1 | 2 | 3)
    }
}

impl std::fmt::Debug for FieldLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            FieldLabel::OPTIONAL => write!(f, "FieldLabel::OPTIONAL"),
            FieldLabel::REQUIRED => write!(f, "FieldLabel::REQUIRED"),
            FieldLabel::REPEATED => write!(f, "FieldLabel::REPEATED"),
            _ => write!(f, "FieldLabel({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptimizeMode(i32);

impl OptimizeMode {
    pub const SPEED: Self = Self(1);
    pub const CODE_SIZE: Self = Self(2);
    pub const LITE_RUNTIME: Self = Self(3);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 1 | 2 | 3)
    }
}

impl std::fmt::Debug for OptimizeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            OptimizeMode::SPEED => write!(f, "OptimizeMode::SPEED"),
            OptimizeMode::CODE_SIZE => write!(f, "OptimizeMode::CODE_SIZE"),
            OptimizeMode::LITE_RUNTIME => write!(f, "OptimizeMode::LITE_RUNTIME"),
            _ => write!(f, "OptimizeMode({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CType(i32);

impl CType {
    pub const STRING: Self = Self(0);
    pub const CORD: Self = Self(1);
    pub const STRING_PIECE: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for CType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            CType::STRING => write!(f, "CType::STRING"),
            CType::CORD => write!(f, "CType::CORD"),
            CType::STRING_PIECE => write!(f, "CType::STRING_PIECE"),
            _ => write!(f, "CType({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct JSType(i32);

impl JSType {
    pub const NORMAL: Self = Self(0);
    pub const STRING: Self = Self(1);
    pub const NUMBER: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for JSType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            JSType::NORMAL => write!(f, "JSType::NORMAL"),
            JSType::STRING => write!(f, "JSType::STRING"),
            JSType::NUMBER => write!(f, "JSType::NUMBER"),
            _ => write!(f, "JSType({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionRetention(i32);

impl OptionRetention {
    pub const UNKNOWN: Self = Self(0);
    pub const RUNTIME: Self = Self(1);
    pub const SOURCE: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for OptionRetention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            OptionRetention::UNKNOWN => write!(f, "OptionRetention::UNKNOWN"),
            OptionRetention::RUNTIME => write!(f, "OptionRetention::RUNTIME"),
            OptionRetention::SOURCE => write!(f, "OptionRetention::SOURCE"),
            _ => write!(f, "OptionRetention({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionTargetType(i32);

impl OptionTargetType {
    pub const UNKNOWN: Self = Self(0);
    pub const FILE: Self = Self(1);
    pub const EXTENSION_RANGE: Self = Self(2);
    pub const MESSAGE: Self = Self(3);
    pub const FIELD: Self = Self(4);
    pub const ONEOF: Self = Self(5);
    pub const ENUM: Self = Self(6);
    pub const ENUM_ENTRY: Self = Self(7);
    pub const SERVICE: Self = Self(8);
    pub const METHOD: Self = Self(9);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        (0..=9).contains(&self.0)
    }
}

impl std::fmt::Debug for OptionTargetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            OptionTargetType::UNKNOWN => write!(f, "OptionTargetType::UNKNOWN"),
            OptionTargetType::FILE => write!(f, "OptionTargetType::FILE"),
            OptionTargetType::EXTENSION_RANGE => write!(f, "OptionTargetType::EXTENSION_RANGE"),
            OptionTargetType::MESSAGE => write!(f, "OptionTargetType::MESSAGE"),
            OptionTargetType::FIELD => write!(f, "OptionTargetType::FIELD"),
            OptionTargetType::ONEOF => write!(f, "OptionTargetType::ONEOF"),
            OptionTargetType::ENUM => write!(f, "OptionTargetType::ENUM"),
            OptionTargetType::ENUM_ENTRY => write!(f, "OptionTargetType::ENUM_ENTRY"),
            OptionTargetType::SERVICE => write!(f, "OptionTargetType::SERVICE"),
            OptionTargetType::METHOD => write!(f, "OptionTargetType::METHOD"),
            _ => write!(f, "OptionTargetType({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolVisibility(i32);

impl SymbolVisibility {
    pub const UNSET: Self = Self(0);
    pub const LOCAL: Self = Self(1);
    pub const EXPORT: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for SymbolVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            SymbolVisibility::UNSET => write!(f, "SymbolVisibility::UNSET"),
            SymbolVisibility::LOCAL => write!(f, "SymbolVisibility::LOCAL"),
            SymbolVisibility::EXPORT => write!(f, "SymbolVisibility::EXPORT"),
            _ => write!(f, "SymbolVisibility({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldPresence(i32);

impl FieldPresence {
    pub const UNKNOWN: Self = Self(0);
    pub const EXPLICIT: Self = Self(1);
    pub const IMPLICIT: Self = Self(2);
    pub const LEGACY_REQUIRED: Self = Self(3);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2 | 3)
    }
}

impl std::fmt::Debug for FieldPresence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            FieldPresence::UNKNOWN => write!(f, "FieldPresence::UNKNOWN"),
            FieldPresence::EXPLICIT => write!(f, "FieldPresence::EXPLICIT"),
            FieldPresence::IMPLICIT => write!(f, "FieldPresence::IMPLICIT"),
            FieldPresence::LEGACY_REQUIRED => write!(f, "FieldPresence::LEGACY_REQUIRED"),
            _ => write!(f, "FieldPresence({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumType(i32);

impl EnumType {
    pub const UNKNOWN: Self = Self(0);
    pub const OPEN: Self = Self(1);
    pub const CLOSED: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for EnumType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            EnumType::UNKNOWN => write!(f, "EnumType::UNKNOWN"),
            EnumType::OPEN => write!(f, "EnumType::OPEN"),
            EnumType::CLOSED => write!(f, "EnumType::CLOSED"),
            _ => write!(f, "EnumType({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepeatedFieldEncoding(i32);

impl RepeatedFieldEncoding {
    pub const UNKNOWN: Self = Self(0);
    pub const PACKED: Self = Self(1);
    pub const EXPANDED: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for RepeatedFieldEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            RepeatedFieldEncoding::UNKNOWN => write!(f, "RepeatedFieldEncoding::UNKNOWN"),
            RepeatedFieldEncoding::PACKED => write!(f, "RepeatedFieldEncoding::PACKED"),
            RepeatedFieldEncoding::EXPANDED => write!(f, "RepeatedFieldEncoding::EXPANDED"),
            _ => write!(f, "RepeatedFieldEncoding({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Utf8Validation(i32);

impl Utf8Validation {
    pub const UNKNOWN: Self = Self(0);
    pub const VERIFY: Self = Self(2);
    pub const NONE: Self = Self(3);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 2 | 3)
    }
}

impl std::fmt::Debug for Utf8Validation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Utf8Validation::UNKNOWN => write!(f, "Utf8Validation::UNKNOWN"),
            Utf8Validation::VERIFY => write!(f, "Utf8Validation::VERIFY"),
            Utf8Validation::NONE => write!(f, "Utf8Validation::NONE"),
            _ => write!(f, "Utf8Validation({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageEncoding(i32);

impl MessageEncoding {
    pub const UNKNOWN: Self = Self(0);
    pub const LENGTH_PREFIXED: Self = Self(1);
    pub const DELIMITED: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for MessageEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            MessageEncoding::UNKNOWN => write!(f, "MessageEncoding::UNKNOWN"),
            MessageEncoding::LENGTH_PREFIXED => write!(f, "MessageEncoding::LENGTH_PREFIXED"),
            MessageEncoding::DELIMITED => write!(f, "MessageEncoding::DELIMITED"),
            _ => write!(f, "MessageEncoding({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct JsonFormat(i32);

impl JsonFormat {
    pub const UNKNOWN: Self = Self(0);
    pub const ALLOW: Self = Self(1);
    pub const LEGACY_BEST_EFFORT: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for JsonFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            JsonFormat::UNKNOWN => write!(f, "JsonFormat::UNKNOWN"),
            JsonFormat::ALLOW => write!(f, "JsonFormat::ALLOW"),
            JsonFormat::LEGACY_BEST_EFFORT => write!(f, "JsonFormat::LEGACY_BEST_EFFORT"),
            _ => write!(f, "JsonFormat({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnforceNamingStyle(i32);

impl EnforceNamingStyle {
    pub const UNKNOWN: Self = Self(0);
    pub const STYLE_2024: Self = Self(1);
    pub const STYLE_LEGACY: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for EnforceNamingStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            EnforceNamingStyle::UNKNOWN => write!(f, "EnforceNamingStyle::UNKNOWN"),
            EnforceNamingStyle::STYLE_2024 => write!(f, "EnforceNamingStyle::STYLE_2024"),
            EnforceNamingStyle::STYLE_LEGACY => write!(f, "EnforceNamingStyle::STYLE_LEGACY"),
            _ => write!(f, "EnforceNamingStyle({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefaultSymbolVisibility(i32);

impl DefaultSymbolVisibility {
    pub const UNKNOWN: Self = Self(0);
    pub const EXPORT_ALL: Self = Self(1);
    pub const EXPORT_TOP_LEVEL: Self = Self(2);
    pub const LOCAL_ALL: Self = Self(3);
    pub const STRICT: Self = Self(4);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2 | 3 | 4)
    }
}

impl std::fmt::Debug for DefaultSymbolVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            DefaultSymbolVisibility::UNKNOWN => write!(f, "DefaultSymbolVisibility::UNKNOWN"),
            DefaultSymbolVisibility::EXPORT_ALL => write!(f, "DefaultSymbolVisibility::EXPORT_ALL"),
            DefaultSymbolVisibility::EXPORT_TOP_LEVEL => {
                write!(f, "DefaultSymbolVisibility::EXPORT_TOP_LEVEL")
            }
            DefaultSymbolVisibility::LOCAL_ALL => write!(f, "DefaultSymbolVisibility::LOCAL_ALL"),
            DefaultSymbolVisibility::STRICT => write!(f, "DefaultSymbolVisibility::STRICT"),
            _ => write!(f, "DefaultSymbolVisibility({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdempotencyLevel(i32);

impl IdempotencyLevel {
    pub const UNKNOWN: Self = Self(0);
    pub const NO_SIDE_EFFECTS: Self = Self(1);
    pub const IDEMPOTENT: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for IdempotencyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            IdempotencyLevel::UNKNOWN => write!(f, "IdempotencyLevel::UNKNOWN"),
            IdempotencyLevel::NO_SIDE_EFFECTS => write!(f, "IdempotencyLevel::NO_SIDE_EFFECTS"),
            IdempotencyLevel::IDEMPOTENT => write!(f, "IdempotencyLevel::IDEMPOTENT"),
            _ => write!(f, "IdempotencyLevel({})", self.0),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Semantic(i32);

impl Semantic {
    /// There is no effect or the effect is indescribable
    pub const NONE: Self = Self(0);
    /// The element is set or otherwise mutated
    pub const SET: Self = Self(1);
    /// An alias to the element is returned
    pub const ALIAS: Self = Self(2);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1 | 2)
    }
}

impl std::fmt::Debug for Semantic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Semantic::NONE => write!(f, "Semantic::NONE"),
            Semantic::SET => write!(f, "Semantic::SET"),
            Semantic::ALIAS => write!(f, "Semantic::ALIAS"),
            _ => write!(f, "Semantic({})", self.0),
        }
    }
}

// Main message structures (outlines only)
/// TODO: Implement FileDescriptorSet message
/// Contains a set of file descriptors
pub struct FileDescriptorSet;

/// TODO: Implement FileDescriptorProto message
/// Contains the complete definition of a .proto file
pub struct FileDescriptorProto;

/// TODO: Implement DescriptorProto message
/// Describes a message type
pub struct DescriptorProto;

/// TODO: Implement ExtensionRangeOptions message
/// Options for extension ranges
pub struct ExtensionRangeOptions;

/// TODO: Implement FieldDescriptorProto message
/// Describes a field within a message
pub struct FieldDescriptorProto;

/// TODO: Implement OneofDescriptorProto message
/// Describes a oneof field group
pub struct OneofDescriptorProto;

/// TODO: Implement EnumDescriptorProto message
/// Describes an enum type
pub struct EnumDescriptorProto;

/// TODO: Implement EnumValueDescriptorProto message
/// Describes a value within an enum
pub struct EnumValueDescriptorProto;

/// TODO: Implement ServiceDescriptorProto message
/// Describes a service
pub struct ServiceDescriptorProto;

/// TODO: Implement MethodDescriptorProto message
/// Describes a method within a service
pub struct MethodDescriptorProto;

/// TODO: Implement FileOptions message
/// Options for .proto files
pub struct FileOptions;

/// TODO: Implement MessageOptions message
/// Options for message types
pub struct MessageOptions;

/// TODO: Implement FieldOptions message
/// Options for fields
pub struct FieldOptions;

/// TODO: Implement OneofOptions message
/// Options for oneof fields
pub struct OneofOptions;

/// TODO: Implement EnumOptions message
/// Options for enum types
pub struct EnumOptions;

/// TODO: Implement EnumValueOptions message
/// Options for enum values
pub struct EnumValueOptions;

/// TODO: Implement ServiceOptions message
/// Options for services
pub struct ServiceOptions;

/// TODO: Implement MethodOptions message
/// Options for methods
pub struct MethodOptions;

/// TODO: Implement FeatureSet message
/// Defines a set of protobuf features
pub struct FeatureSet;

/// TODO: Implement FeatureSetDefaults message
/// Default feature sets for different editions
pub struct FeatureSetDefaults;

/// TODO: Implement SourceCodeInfo message
/// Contains information about the original source code
pub struct SourceCodeInfo;

/// TODO: Implement GeneratedCodeInfo message
/// Contains information about generated code
pub struct GeneratedCodeInfo;

// Nested message structures (inlined from small submodules)
/// TODO: Implement ExtensionRange message
/// Extension range within a message
pub struct ExtensionRange;

/// TODO: Implement ReservedRange message
/// Reserved range within a message
pub struct ReservedRange;

/// TODO: Implement EditionDefault message
/// Edition default value for a field
pub struct EditionDefault;

/// TODO: Implement FeatureSupport message
/// Feature support information for a field
pub struct FeatureSupport;

/// TODO: Implement EnumReservedRange message
/// Range of reserved numeric values in an enum
pub struct EnumReservedRange;

/// Part of an uninterpreted option name
/// Each string represents a segment in a dot-separated name
///
/// Source definition from descriptor.proto:
/// message NamePart {
///   required string name_part = 1;
///   required bool is_extension = 2;
/// }
#[derive(Default)]
pub struct NamePart {
    name_part: String,  // required
    is_extension: bool, // required
}

impl NamePart {
    pub fn new(name_part: String, is_extension: bool) -> Self {
        Self {
            name_part,
            is_extension,
        }
    }

    // Getters only (required fields don't need defaults)
    pub fn name_part(&self) -> &str {
        &self.name_part
    }
    pub fn is_extension(&self) -> bool {
        self.is_extension
    }
}

// Implement HasMetadata trait for NamePart using macro
impl crate::descriptor::HasMetadata for NamePart {
    define_metadata! {
        required name_part: String = 1;
        required is_extension: bool = 2;
    }
}

// DescriptorMessage is automatically implemented via HasMetadata trait

/// TODO: Implement FeatureSetEditionDefault message
/// Default feature set for a specific edition
pub struct FeatureSetEditionDefault;

/// TODO: Implement Location message
/// Location information in source code
pub struct Location;

/// TODO: Implement Annotation message
/// Annotation connecting generated code to source
pub struct Annotation;

/// UninterpretedOption message
/// Represents an option the parser does not recognize
///
/// Source definition from descriptor.proto:
/// message UninterpretedOption {
///   repeated NamePart name = 2;
///   optional string identifier_value = 3;
///   optional uint64 positive_int_value = 4;
///   optional int64 negative_int_value = 5;
///   optional double double_value = 6;
///   optional bytes string_value = 7;
///   optional string aggregate_value = 8;
/// }
#[derive(Default)]
pub struct UninterpretedOption {
    name: Vec<NamePart>,                                           // repeated
    identifier_value: Option<String>,                              // optional
    positive_int_value: Option<crate::descriptor::ProtobufUInt64>, // optional
    negative_int_value: Option<crate::descriptor::ProtobufInt64>,  // optional
    double_value: Option<f64>,                                     // optional
    string_value: Option<Vec<u8>>,                                 // optional (bytes)
    aggregate_value: Option<String>,                               // optional
}

impl UninterpretedOption {
    // Default implementation provides empty values
    // pub fn new() -> Self { Self::default() } // This is now redundant

    // Repeated field methods (read-only)
    pub fn name(&self) -> &[NamePart] {
        &self.name
    }

    // Optional field getters with defaults
    pub fn identifier_value(&self) -> &str {
        self.identifier_value.as_deref().unwrap_or_default()
    }
    pub fn positive_int_value(&self) -> crate::descriptor::ProtobufUInt64 {
        self.positive_int_value
            .unwrap_or_else(|| crate::descriptor::ProtobufUInt64::new(0))
    }
    pub fn negative_int_value(&self) -> crate::descriptor::ProtobufInt64 {
        self.negative_int_value
            .unwrap_or_else(|| crate::descriptor::ProtobufInt64::new(0))
    }
    pub fn double_value(&self) -> f64 {
        self.double_value.unwrap_or_default()
    }
    pub fn string_value(&self) -> &[u8] {
        self.string_value.as_deref().unwrap_or_default()
    }
    pub fn aggregate_value(&self) -> &str {
        self.aggregate_value.as_deref().unwrap_or_default()
    }

    // Has methods for optional fields
    pub fn has_identifier_value(&self) -> bool {
        self.identifier_value.is_some()
    }
    pub fn has_positive_int_value(&self) -> bool {
        self.positive_int_value.is_some()
    }
    pub fn has_negative_int_value(&self) -> bool {
        self.negative_int_value.is_some()
    }
    pub fn has_double_value(&self) -> bool {
        self.double_value.is_some()
    }
    pub fn has_string_value(&self) -> bool {
        self.string_value.is_some()
    }
    pub fn has_aggregate_value(&self) -> bool {
        self.aggregate_value.is_some()
    }
}

// Implement HasMetadata trait for UninterpretedOption using macro
impl crate::descriptor::HasMetadata for UninterpretedOption {
    define_metadata! {
        repeated name: Vec<NamePart> = 2;
        optional identifier_value: String = 3;
        optional positive_int_value: ProtobufUInt64 = 4;
        optional negative_int_value: ProtobufInt64 = 5;
        optional double_value: f64 = 6;
        optional string_value: Vec<u8> = 7;
        optional aggregate_value: String = 8;
    }
}

// DescriptorMessage is automatically implemented via HasMetadata trait

// Remaining modules that are larger or have more complex structures
pub mod extension_range_options;
pub mod feature_set;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::HasMetadata;

    #[test]
    fn test_name_part() {
        let name_part = NamePart::new("test_name".to_string(), true);
        assert_eq!(name_part.name_part(), "test_name");
        assert_eq!(name_part.is_extension(), true);

        let metadata = NamePart::metadata();
        assert_eq!(metadata.fields.len(), 2);
        assert_eq!(metadata.fields[0].number, 1);
        assert_eq!(metadata.fields[1].number, 2);
    }

    #[test]
    fn test_uninterpreted_option() {
        let option = UninterpretedOption::default();

        // Test default values
        assert_eq!(option.name().len(), 0);
        assert_eq!(option.identifier_value(), "");
        assert_eq!(option.positive_int_value().value(), 0);
        assert_eq!(option.negative_int_value().value(), 0);
        assert_eq!(option.double_value(), 0.0);
        assert_eq!(option.string_value().len(), 0);
        assert_eq!(option.aggregate_value(), "");

        // Test has methods
        assert!(!option.has_identifier_value());
        assert!(!option.has_positive_int_value());
        assert!(!option.has_negative_int_value());
        assert!(!option.has_double_value());
        assert!(!option.has_string_value());
        assert!(!option.has_aggregate_value());

        // Test metadata
        let metadata = UninterpretedOption::metadata();
        assert_eq!(metadata.fields.len(), 7);
        assert_eq!(metadata.fields[0].number, 2); // name field
        assert_eq!(metadata.fields[1].number, 3); // identifier_value field
    }
}
