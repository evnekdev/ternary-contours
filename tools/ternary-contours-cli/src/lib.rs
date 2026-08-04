//! Human-editable TCT input, stable liquidus projection, and static rendering.
//!
//! This crate is deliberately a repository tool (`publish = false`). The parser
//! model remains independent from the numerical library, while the conversion
//! module adapts it to `ternary-contours` only at calculation time.

pub mod editor;
pub mod model;
pub mod parser;
pub mod projection;
pub mod render;
pub mod serialize;
pub mod table;
pub mod template;
#[cfg(feature = "viewer")]
pub mod viewer;

pub use editor::*;
pub use model::*;
pub use parser::{TctError, parse_path, parse_str};
pub use projection::{
    LiquidusProjection, ProjectionOptions, SourceInterpolation, calculate_projection,
    parse_level_spec,
};
pub use render::{
    OutputFormat, RenderOptions, RenderPathMode, RenderedBitmap, TernaryRenderTransform,
    composition_from_logical, composition_to_logical, render_to_bitmap, render_to_bitmap_with_raw,
    render_to_path, render_to_path_with_raw, rgb_to_rgba,
};
pub use serialize::{
    NumericFormat, SerializeError, TctSerializeOptions, save_tct_atomic, serialize_tct,
};
pub use table::{
    HeaderMode, ParsedCell, ParsedRow, ParsedTable, TableError, TableLocation, parse_tsv_row,
};
pub use template::{
    IrregularTemplateStyle, irregular_template, parse_components, parse_field_specs,
    regular_template_tct,
};
