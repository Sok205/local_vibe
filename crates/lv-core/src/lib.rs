//! Core types, traits, and configuration shared across all LocalVibe crates.
//!
//! The crate is dependency-light by design — every backend, vector store,
//! parser, and chunker lives behind a trait defined here.

pub mod config;
pub mod error;
pub mod sidecar;
pub mod status;
pub mod traits;
pub mod types;

pub use config::Config;
pub use error::{Result, VibeError};
