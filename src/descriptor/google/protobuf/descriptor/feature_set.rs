//! FeatureSet nested message definitions

/// Default symbol visibility feature
pub mod visibility_feature {
    /// Default symbol visibility
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum DefaultSymbolVisibility {
        /// Default pre-EDITION_2024, all UNSET visibility are export
        DefaultSymbolVisibilityUnknown = 0,
        /// Default pre-EDITION_2024, all UNSET visibility are export
        ExportAll = 1,
        /// All top-level symbols default to export, nested default to local
        ExportTopLevel = 2,
        /// All symbols default to local
        LocalAll = 3,
        /// All symbols local by default. Nested types cannot be exported
        Strict = 4,
    }
}
