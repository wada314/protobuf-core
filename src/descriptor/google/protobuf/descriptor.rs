//! Protocol Buffers descriptor definitions
//!
//! This module contains the data structures and constants defined in descriptor.proto

/// The full set of known editions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edition {
    /// A placeholder for an unknown edition value
    EditionUnknown = 0,
    /// A placeholder edition for specifying default behaviors before a feature was first introduced
    EditionLegacy = 900,
    /// Legacy syntax "editions" - these pre-date editions but behave much like distinct editions
    EditionProto2 = 998,
    /// Legacy syntax "editions" - these pre-date editions but behave much like distinct editions
    EditionProto3 = 999,
    /// Editions that have been released
    Edition2023 = 1000,
    /// Editions that have been released
    Edition2024 = 1001,
    /// Placeholder editions for testing feature resolution
    Edition1TestOnly = 1,
    /// Placeholder editions for testing feature resolution
    Edition2TestOnly = 2,
    /// Placeholder editions for testing feature resolution
    Edition99997TestOnly = 99997,
    /// Placeholder editions for testing feature resolution
    Edition99998TestOnly = 99998,
    /// Placeholder editions for testing feature resolution
    Edition99999TestOnly = 99999,
    /// Placeholder for specifying unbounded edition support
    EditionMax = 0x7FFFFFFF,
}

/// Field types in protobuf
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldType {
    /// 0 is reserved for errors
    TypeDouble = 1,
    TypeFloat = 2,
    /// Not ZigZag encoded. Negative numbers take 10 bytes
    TypeInt64 = 3,
    TypeUint64 = 4,
    /// Not ZigZag encoded. Negative numbers take 10 bytes
    TypeInt32 = 5,
    TypeFixed64 = 6,
    TypeFixed32 = 7,
    TypeBool = 8,
    TypeString = 9,
    /// Group type is deprecated and not supported after google.protobuf
    TypeGroup = 10,
    TypeMessage = 11,
    /// New in version 2
    TypeBytes = 12,
    TypeUint32 = 13,
    TypeEnum = 14,
    TypeSfixed32 = 15,
    TypeSfixed64 = 16,
    /// Uses ZigZag encoding
    TypeSint32 = 17,
    /// Uses ZigZag encoding
    TypeSint64 = 18,
}

/// Field labels in protobuf
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldLabel {
    /// 0 is reserved for errors
    LabelOptional = 1,
    LabelRequired = 2,
    LabelRepeated = 3,
}

/// Optimize mode for generated classes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptimizeMode {
    /// Generate complete code for parsing, serialization, etc.
    Speed = 1,
    /// Use ReflectionOps to implement these methods
    CodeSize = 2,
    /// Generate code using MessageLite and the lite runtime
    LiteRuntime = 3,
}

/// CType for field options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CType {
    /// Default mode
    String = 0,
    /// Store data in a Cord instead of a string
    Cord = 1,
    StringPiece = 2,
}

/// JSType for field options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JSType {
    /// Use the default type
    JsNormal = 0,
    /// Use JavaScript strings
    JsString = 1,
    /// Use JavaScript numbers
    JsNumber = 2,
}

/// Option retention level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionRetention {
    RetentionUnknown = 0,
    RetentionRuntime = 1,
    RetentionSource = 2,
}

/// Option target type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionTargetType {
    TargetTypeUnknown = 0,
    TargetTypeFile = 1,
    TargetTypeExtensionRange = 2,
    TargetTypeMessage = 3,
    TargetTypeField = 4,
    TargetTypeOneof = 5,
    TargetTypeEnum = 6,
    TargetTypeEnumEntry = 7,
    TargetTypeService = 8,
    TargetTypeMethod = 9,
}

/// Symbol visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolVisibility {
    VisibilityUnset = 0,
    VisibilityLocal = 1,
    VisibilityExport = 2,
}

/// Field presence feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldPresence {
    FieldPresenceUnknown = 0,
    Explicit = 1,
    Implicit = 2,
    LegacyRequired = 3,
}

/// Enum type feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnumType {
    EnumTypeUnknown = 0,
    Open = 1,
    Closed = 2,
}

/// Repeated field encoding feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RepeatedFieldEncoding {
    RepeatedFieldEncodingUnknown = 0,
    Packed = 1,
    Expanded = 2,
}

/// UTF8 validation feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Utf8Validation {
    Utf8ValidationUnknown = 0,
    Verify = 2,
    None = 3,
}

/// Message encoding feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageEncoding {
    MessageEncodingUnknown = 0,
    LengthPrefixed = 1,
    Delimited = 2,
}

/// JSON format feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonFormat {
    JsonFormatUnknown = 0,
    Allow = 1,
    LegacyBestEffort = 2,
}

/// Enforce naming style feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnforceNamingStyle {
    EnforceNamingStyleUnknown = 0,
    Style2024 = 1,
    StyleLegacy = 2,
}

/// Default symbol visibility feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefaultSymbolVisibility {
    DefaultSymbolVisibilityUnknown = 0,
    ExportAll = 1,
    ExportTopLevel = 2,
    LocalAll = 3,
    Strict = 4,
}

/// Idempotency level for methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdempotencyLevel {
    IdempotencyUnknown = 0,
    NoSideEffects = 1,
    Idempotent = 2,
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
