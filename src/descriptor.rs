//! Protocol Buffers descriptor module
//!
//! This module provides the data structures and constants defined in the official
//! protobuf descriptor.proto and plugin.proto files.

pub mod google {
    pub mod protobuf {
        pub mod descriptor;
        pub mod compiler {
            pub mod plugin;
        }
    }
}
