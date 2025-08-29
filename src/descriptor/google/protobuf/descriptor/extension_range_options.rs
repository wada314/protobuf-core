//! ExtensionRangeOptions nested message definitions

/// Declaration of an extension within a range
pub struct Declaration;

/// Verification state of an extension range
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationState {
    /// All extensions of the range must be declared
    Declaration = 0,
    /// Extensions are not verified
    Unverified = 1,
}
