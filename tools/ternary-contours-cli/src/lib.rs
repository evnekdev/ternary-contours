//! Human-editable TCT input, stable liquidus projection, and static rendering.
//!
//! This crate is deliberately a repository tool (`publish = false`). The parser
//! model remains independent from the numerical library, while the conversion
//! module adapts it to `ternary-contours` only at calculation time.

pub mod model;
pub mod parser;
pub mod projection;
pub mod render;
#[cfg(feature = "viewer")]
pub mod viewer;

pub use model::*;
pub use parser::{TctError, parse_path, parse_str};
pub use projection::{
    LiquidusProjection, ProjectionOptions, calculate_projection, parse_level_spec,
};
pub use render::{
    OutputFormat, RenderOptions, RenderPathMode, RenderedBitmap, render_to_bitmap,
    render_to_bitmap_with_raw, render_to_path, render_to_path_with_raw, rgb_to_rgba,
};
