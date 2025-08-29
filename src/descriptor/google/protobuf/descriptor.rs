//! Protocol Buffers descriptor definitions
//!
//! This module contains the data structures and constants defined in descriptor.proto

// Edition constants
pub const EDITION_UNKNOWN: i32 = 0;
pub const EDITION_LEGACY: i32 = 900;
pub const EDITION_PROTO2: i32 = 998;
pub const EDITION_PROTO3: i32 = 999;
pub const EDITION_2023: i32 = 1000;
pub const EDITION_2024: i32 = 1001;
pub const EDITION_1_TEST_ONLY: i32 = 1;
pub const EDITION_2_TEST_ONLY: i32 = 2;
pub const EDITION_99997_TEST_ONLY: i32 = 99997;
pub const EDITION_99998_TEST_ONLY: i32 = 99998;
pub const EDITION_99999_TEST_ONLY: i32 = 99999;
pub const EDITION_MAX: i32 = 0x7FFFFFFF;

// Field type constants
pub const TYPE_DOUBLE: i32 = 1;
pub const TYPE_FLOAT: i32 = 2;
pub const TYPE_INT64: i32 = 3;
pub const TYPE_UINT64: i32 = 4;
pub const TYPE_INT32: i32 = 5;
pub const TYPE_FIXED64: i32 = 6;
pub const TYPE_FIXED32: i32 = 7;
pub const TYPE_BOOL: i32 = 8;
pub const TYPE_STRING: i32 = 9;
pub const TYPE_GROUP: i32 = 10;
pub const TYPE_MESSAGE: i32 = 11;
pub const TYPE_BYTES: i32 = 12;
pub const TYPE_UINT32: i32 = 13;
pub const TYPE_ENUM: i32 = 14;
pub const TYPE_SFIXED32: i32 = 15;
pub const TYPE_SFIXED64: i32 = 16;
pub const TYPE_SINT32: i32 = 17;
pub const TYPE_SINT64: i32 = 18;

// Field label constants
pub const LABEL_OPTIONAL: i32 = 1;
pub const LABEL_REQUIRED: i32 = 2;
pub const LABEL_REPEATED: i32 = 3;

// Optimize mode constants
pub const OPTIMIZE_MODE_SPEED: i32 = 1;
pub const OPTIMIZE_MODE_CODE_SIZE: i32 = 2;
pub const OPTIMIZE_MODE_LITE_RUNTIME: i32 = 3;

// CType constants
pub const CTYPE_STRING: i32 = 0;
pub const CTYPE_CORD: i32 = 1;
pub const CTYPE_STRING_PIECE: i32 = 2;

// JSType constants
pub const JSTYPE_NORMAL: i32 = 0;
pub const JSTYPE_STRING: i32 = 1;
pub const JSTYPE_NUMBER: i32 = 2;

// Option retention constants
pub const RETENTION_UNKNOWN: i32 = 0;
pub const RETENTION_RUNTIME: i32 = 1;
pub const RETENTION_SOURCE: i32 = 2;

// Option target type constants
pub const TARGET_TYPE_UNKNOWN: i32 = 0;
pub const TARGET_TYPE_FILE: i32 = 1;
pub const TARGET_TYPE_EXTENSION_RANGE: i32 = 2;
pub const TARGET_TYPE_MESSAGE: i32 = 3;
pub const TARGET_TYPE_FIELD: i32 = 4;
pub const TARGET_TYPE_ONEOF: i32 = 5;
pub const TARGET_TYPE_ENUM: i32 = 6;
pub const TARGET_TYPE_ENUM_ENTRY: i32 = 7;
pub const TARGET_TYPE_SERVICE: i32 = 8;
pub const TARGET_TYPE_METHOD: i32 = 9;

// Symbol visibility constants
pub const VISIBILITY_UNSET: i32 = 0;
pub const VISIBILITY_LOCAL: i32 = 1;
pub const VISIBILITY_EXPORT: i32 = 2;

// Feature constants
pub const FIELD_PRESENCE_UNKNOWN: i32 = 0;
pub const FIELD_PRESENCE_EXPLICIT: i32 = 1;
pub const FIELD_PRESENCE_IMPLICIT: i32 = 2;
pub const FIELD_PRESENCE_LEGACY_REQUIRED: i32 = 3;

pub const ENUM_TYPE_UNKNOWN: i32 = 0;
pub const ENUM_TYPE_OPEN: i32 = 1;
pub const ENUM_TYPE_CLOSED: i32 = 2;

pub const REPEATED_FIELD_ENCODING_UNKNOWN: i32 = 0;
pub const REPEATED_FIELD_ENCODING_PACKED: i32 = 1;
pub const REPEATED_FIELD_ENCODING_EXPANDED: i32 = 2;

pub const UTF8_VALIDATION_UNKNOWN: i32 = 0;
pub const UTF8_VALIDATION_VERIFY: i32 = 2;
pub const UTF8_VALIDATION_NONE: i32 = 3;

pub const MESSAGE_ENCODING_UNKNOWN: i32 = 0;
pub const MESSAGE_ENCODING_LENGTH_PREFIXED: i32 = 1;
pub const MESSAGE_ENCODING_DELIMITED: i32 = 2;

pub const JSON_FORMAT_UNKNOWN: i32 = 0;
pub const JSON_FORMAT_ALLOW: i32 = 1;
pub const JSON_FORMAT_LEGACY_BEST_EFFORT: i32 = 2;

pub const ENFORCE_NAMING_STYLE_UNKNOWN: i32 = 0;
pub const ENFORCE_NAMING_STYLE_2024: i32 = 1;
pub const ENFORCE_NAMING_STYLE_LEGACY: i32 = 2;

pub const DEFAULT_SYMBOL_VISIBILITY_UNKNOWN: i32 = 0;
pub const DEFAULT_SYMBOL_VISIBILITY_EXPORT_ALL: i32 = 1;
pub const DEFAULT_SYMBOL_VISIBILITY_EXPORT_TOP_LEVEL: i32 = 2;
pub const DEFAULT_SYMBOL_VISIBILITY_LOCAL_ALL: i32 = 3;
pub const DEFAULT_SYMBOL_VISIBILITY_STRICT: i32 = 4;

// Idempotency level constants
pub const IDEMPOTENCY_UNKNOWN: i32 = 0;
pub const IDEMPOTENCY_NO_SIDE_EFFECTS: i32 = 1;
pub const IDEMPOTENCY_IDEMPOTENT: i32 = 2;

// Wrapped types for type safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Edition(i32);

impl Edition {
    pub const UNKNOWN: Self = Self(EDITION_UNKNOWN);
    pub const LEGACY: Self = Self(EDITION_LEGACY);
    pub const PROTO2: Self = Self(EDITION_PROTO2);
    pub const PROTO3: Self = Self(EDITION_PROTO3);
    pub const EDITION_2023: Self = Self(EDITION_2023);
    pub const EDITION_2024: Self = Self(EDITION_2024);
    pub const EDITION_1_TEST_ONLY: Self = Self(EDITION_1_TEST_ONLY);
    pub const EDITION_2_TEST_ONLY: Self = Self(EDITION_2_TEST_ONLY);
    pub const EDITION_99997_TEST_ONLY: Self = Self(EDITION_99997_TEST_ONLY);
    pub const EDITION_99998_TEST_ONLY: Self = Self(EDITION_99998_TEST_ONLY);
    pub const EDITION_99999_TEST_ONLY: Self = Self(EDITION_99999_TEST_ONLY);
    pub const EDITION_MAX: Self = Self(EDITION_MAX);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(
            self.0,
            EDITION_UNKNOWN
                | EDITION_LEGACY
                | EDITION_PROTO2
                | EDITION_PROTO3
                | EDITION_2023
                | EDITION_2024
        )
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self.0, EDITION_PROTO2 | EDITION_PROTO3)
    }

    pub fn is_modern(&self) -> bool {
        matches!(self.0, EDITION_2023 | EDITION_2024)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldType(i32);

impl FieldType {
    pub const DOUBLE: Self = Self(TYPE_DOUBLE);
    pub const FLOAT: Self = Self(TYPE_FLOAT);
    pub const INT64: Self = Self(TYPE_INT64);
    pub const UINT64: Self = Self(TYPE_UINT64);
    pub const INT32: Self = Self(TYPE_INT32);
    pub const FIXED64: Self = Self(TYPE_FIXED64);
    pub const FIXED32: Self = Self(TYPE_FIXED32);
    pub const BOOL: Self = Self(TYPE_BOOL);
    pub const STRING: Self = Self(TYPE_STRING);
    pub const GROUP: Self = Self(TYPE_GROUP);
    pub const MESSAGE: Self = Self(TYPE_MESSAGE);
    pub const BYTES: Self = Self(TYPE_BYTES);
    pub const UINT32: Self = Self(TYPE_UINT32);
    pub const ENUM: Self = Self(TYPE_ENUM);
    pub const SFIXED32: Self = Self(TYPE_SFIXED32);
    pub const SFIXED64: Self = Self(TYPE_SFIXED64);
    pub const SINT32: Self = Self(TYPE_SINT32);
    pub const SINT64: Self = Self(TYPE_SINT64);

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
        matches!(
            self.0,
            TYPE_INT32 | TYPE_INT64 | TYPE_UINT32 | TYPE_UINT64 | TYPE_SINT32 | TYPE_SINT64
        )
    }

    pub fn is_fixed(&self) -> bool {
        matches!(
            self.0,
            TYPE_FIXED32 | TYPE_FIXED64 | TYPE_SFIXED32 | TYPE_SFIXED64
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldLabel(i32);

impl FieldLabel {
    pub const OPTIONAL: Self = Self(LABEL_OPTIONAL);
    pub const REQUIRED: Self = Self(LABEL_REQUIRED);
    pub const REPEATED: Self = Self(LABEL_REPEATED);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, LABEL_OPTIONAL | LABEL_REQUIRED | LABEL_REPEATED)
    }
}

// Main message structures (outlines only)
pub struct FileDescriptorSet;
pub struct FileDescriptorProto;
pub struct DescriptorProto;
pub struct ExtensionRangeOptions;
pub struct FieldDescriptorProto;
pub struct OneofDescriptorProto;
pub struct EnumDescriptorProto;
pub struct EnumValueDescriptorProto;
pub struct ServiceDescriptorProto;
pub struct MethodDescriptorProto;
pub struct FileOptions;
pub struct MessageOptions;
pub struct FieldOptions;
pub struct OneofOptions;
pub struct EnumOptions;
pub struct EnumValueOptions;
pub struct ServiceOptions;
pub struct MethodOptions;
pub struct UninterpretedOption;
pub struct FeatureSet;
pub struct FeatureSetDefaults;
pub struct SourceCodeInfo;
pub struct GeneratedCodeInfo;

// Nested message structures (inlined from small submodules)
/// Extension range within a message
pub struct ExtensionRange;

/// Reserved range within a message
pub struct ReservedRange;

/// Edition default value for a field
pub struct EditionDefault;

/// Feature support information for a field
pub struct FeatureSupport;

/// Range of reserved numeric values in an enum
pub struct EnumReservedRange;

/// Part of an uninterpreted option name
pub struct NamePart;

/// Default feature set for a specific edition
pub struct FeatureSetEditionDefault;

/// Location information in source code
pub struct Location;

/// Annotation connecting generated code to source
pub struct Annotation;

/// Semantic effect of an annotation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Semantic {
    /// There is no effect or the effect is indescribable
    None = 0,
    /// The element is set or otherwise mutated
    Set = 1,
    /// An alias to the element is returned
    Alias = 2,
}

// Remaining modules that are larger or have more complex structures
pub mod extension_range_options;
pub mod feature_set;
