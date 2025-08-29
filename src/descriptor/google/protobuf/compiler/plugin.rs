//! Protocol Buffers compiler plugin definitions
//!
//! This module contains the data structures and constants defined in plugin.proto

/// Supported features for code generators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// No features supported
    FeatureNone = 0,
    /// Supports proto3 optional fields
    FeatureProto3Optional = 1,
    /// Supports editions
    FeatureSupportsEditions = 2,
}

// Main message structures (outlines only)
pub struct Version;
pub struct CodeGeneratorRequest;
pub struct CodeGeneratorResponse;

// Nested message structures
/// Represents a single generated file
pub struct File;
