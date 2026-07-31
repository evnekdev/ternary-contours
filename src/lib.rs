//! Backend-independent regular-grid ternary fields and isoline construction.
//!
//! `ternary-contours` owns regular-lattice indexing, scalar-field validation,
//! directed cubic-alpha interpolation, topology extraction, deterministic path
//! assembly, optional arc-length regularization, and implicit-level projection.
//! It has no dependency on Plotters, drawing backends, screen coordinates, or
//! viewport clipping.
//!
//! The regular lattice stores finite samples at `i + j + k = n` in the ordering
//! documented by [`RegularTernaryScalarField`]. With `cubic-alpha`, shared
//! directed edge intervals use
//! `y0*(1-t) + y1*t + (1-t)*t*(alpha0 + alpha1*t)`.
//!
//! Scope is deliberately limited to regular two-dimensional ternary grids.
//! Piecewise-linear isolines and filled bands are available. Irregular
//! triangulations, arbitrary-dimensional grids, Kuhn simplices, viewport
//! clipping, rendering, and cubic-alpha filled bands are intentionally excluded.

pub mod contour;
mod error;
pub mod field;
pub mod grid;
pub mod interpolation;

/// A semantic `(a, b, c)` composition coordinate owned by the numerical core.
///
/// [`ContourSet`] returns finite normalized coordinates. `new` itself is an
/// unchecked low-level constructor used by numerical algorithms; scalar-field
/// and contour APIs validate all public input before producing paths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TernaryCoordinate {
    a: f64,
    b: f64,
    c: f64,
}

impl TernaryCoordinate {
    /// Construct an unchecked semantic A/B/C coordinate.
    pub const fn new(a: f64, b: f64, c: f64) -> Self {
        Self { a, b, c }
    }

    /// Return the semantic components in canonical A/B/C order.
    pub const fn as_array(self) -> [f64; 3] {
        [self.a, self.b, self.c]
    }
}

impl From<[f64; 3]> for TernaryCoordinate {
    fn from([a, b, c]: [f64; 3]) -> Self {
        Self::new(a, b, c)
    }
}

impl From<TernaryCoordinate> for [f64; 3] {
    fn from(value: TernaryCoordinate) -> Self {
        value.as_array()
    }
}

pub use contour::{
    AdaptiveContourOptions, ContourBand, ContourBandOptions, ContourBandSet, ContourError,
    ContourFragment, ContourInterpolation, ContourLevel, ContourOptions, ContourPath,
    ContourRegion, ContourRegularization, ContourSet, CubicAlphaOptions, CubicContourDiagnostics,
};
pub use error::{FieldError, GridEvaluationError};
pub use grid::{GridVertexId, LatticeCoordinate, RegularTernaryGrid, RegularTernaryScalarField};
pub use interpolation::{BinaryExtrapolation, CubicAlphaMethod, CubicBoundaryPolicy};
