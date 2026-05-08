//! Core traits and shared types for the tools-mcp workspace.
//!
//! This crate is the dependency floor: only `async-trait` and `serde`.
//! Service-specific code (MySQL, SSH, etc.) lives in higher crates.
