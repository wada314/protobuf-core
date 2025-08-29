//! ExtensionRangeOptions nested message definitions

/// Declaration of an extension within a range
pub struct Declaration;

/// Verification state of an extension range
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct VerificationState(i32);

impl VerificationState {
    /// All extensions of the range must be declared
    pub const DECLARATION: Self = Self(0);
    /// Extensions are not verified
    pub const UNVERIFIED: Self = Self(1);

    pub fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn is_known(&self) -> bool {
        matches!(self.0, 0 | 1)
    }
}

impl std::fmt::Debug for VerificationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            VerificationState::DECLARATION => write!(f, "VerificationState::DECLARATION"),
            VerificationState::UNVERIFIED => write!(f, "VerificationState::UNVERIFIED"),
            _ => write!(f, "VerificationState({})", self.0),
        }
    }
}
