//! Configuration schema for tracey
//!
//! r[impl config.format.styx]
//! r[impl config.schema]
//!
//! Config lives at `.config/tracey/config.styx` relative to the project root.

// Re-export from tracey-config crate so build.rs can access the types
pub use tracey_config::*;
