//! Platform-independent foundations for the Rasm Studio drawing and animation engine.
//!
//! This crate deliberately contains no UI, windowing, GPU, or platform dependencies.
//! Rendering and platform adapters must consume these stable mathematical contracts.

pub mod filter;
pub mod time;
pub mod transform;
