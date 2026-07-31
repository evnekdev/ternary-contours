//! Backend-independent numerical primitives for regular two-dimensional ternary grids.
//!
//! `ternary-contours` owns the regular lattice, scalar-field validation, and
//! directed cubic-alpha interpolation model used by `plotters-ternary`. It has
//! no dependency on Plotters, drawing backends, screen coordinates, clipping, or
//! contour path extraction.
//!
//! The regular lattice stores finite scalar samples at `i + j + k = n` in a
//! documented row-major order. With the `cubic-alpha` feature, shared directed
//! edge intervals are constructed through `spline1d`; local triangle fields use
//! the alpha form `y0*(1-t)+y1*t+(1-t)*t*(alpha0+alpha1*t)`.
//!
//! Current scope is deliberately limited to regular two-dimensional ternary
//! grids. Arbitrary-dimensional grids, Kuhn simplices, manifold/path extraction,
//! viewport clipping, rendering, and Plotters integration are excluded.

mod error;
pub mod field;
pub mod grid;
pub mod interpolation;

pub use error::FieldError;
pub use field::{CubicBuildDiagnostics, CubicGridField};
pub use grid::{GridTriangle, GridVertexId, LatticeCoordinate, RegularTernaryScalarField};
pub use interpolation::{
    AlphaInterval, BinaryExtrapolation, CubicAlphaBuildOptions, CubicAlphaMethod,
    CubicAlphaTriangle, CubicBoundaryPolicy, DirectedAlphaInterval, InterpolationError,
    PairEvaluation, evaluate_pair,
};
