//! Human-editable TCT input, stable liquidus projection, and static rendering.
//!
//! This crate is deliberately a repository tool (`publish = false`). The parser
//! model remains independent from the numerical library, while the conversion
//! module adapts it to `ternary-contours` only at calculation time.

pub mod model;
pub mod parser;
pub mod projection;
pub mod render;

pub use model::*;
pub use parser::{TctError, parse_path, parse_str};
pub use projection::{LiquidusProjection, ProjectionOptions, calculate_projection, parse_level_spec};
pub use render::{OutputFormat, RenderOptions, render_to_path};
