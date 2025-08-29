//! FeatureSet nested message definitions

/// Default symbol visibility feature
pub mod visibility_feature {
    /// Default symbol visibility
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    pub struct DefaultSymbolVisibility(i32);

    impl DefaultSymbolVisibility {
        /// Default pre-EDITION_2024, all UNSET visibility are export
        pub const UNKNOWN: Self = Self(0);
        /// Default pre-EDITION_2024, all UNSET visibility are export
        pub const EXPORT_ALL: Self = Self(1);
        /// All top-level symbols default to export, nested default to local
        pub const EXPORT_TOP_LEVEL: Self = Self(2);
        /// All symbols default to local
        pub const LOCAL_ALL: Self = Self(3);
        /// All symbols local by default. Nested types cannot be exported
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
                DefaultSymbolVisibility::EXPORT_ALL => {
                    write!(f, "DefaultSymbolVisibility::EXPORT_ALL")
                }
                DefaultSymbolVisibility::EXPORT_TOP_LEVEL => {
                    write!(f, "DefaultSymbolVisibility::EXPORT_TOP_LEVEL")
                }
                DefaultSymbolVisibility::LOCAL_ALL => {
                    write!(f, "DefaultSymbolVisibility::LOCAL_ALL")
                }
                DefaultSymbolVisibility::STRICT => write!(f, "DefaultSymbolVisibility::STRICT"),
                _ => write!(f, "DefaultSymbolVisibility({})", self.0),
            }
        }
    }
}
