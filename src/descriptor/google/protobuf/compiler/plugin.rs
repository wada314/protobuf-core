//! Protocol Buffers compiler plugin definitions
//!
//! This module contains the data structures and constants defined in plugin.proto

/// Supported features for code generators
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Feature(i32);

impl Feature {
    /// No features supported
    pub const NONE: Self = Self(0);
    /// Supports proto3 optional fields
    pub const PROTO3_OPTIONAL: Self = Self(1);
    /// Supports editions
    pub const SUPPORTS_EDITIONS: Self = Self(2);

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

impl std::fmt::Debug for Feature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Feature::NONE => write!(f, "Feature::NONE"),
            Feature::PROTO3_OPTIONAL => write!(f, "Feature::PROTO3_OPTIONAL"),
            Feature::SUPPORTS_EDITIONS => write!(f, "Feature::SUPPORTS_EDITIONS"),
            _ => write!(f, "Feature({})", self.0),
        }
    }
}

// Main message structures (outlines only)
pub struct Version;
pub struct CodeGeneratorRequest;
pub struct CodeGeneratorResponse;

// Nested message structures
/// Represents a single generated file
pub struct File;
