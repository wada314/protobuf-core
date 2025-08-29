//! GeneratedCodeInfo nested message definitions

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
