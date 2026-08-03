//! Backend-independent ternary fields and isoline construction.
//!
//! `ternary-contours` owns regular-lattice indexing, scalar-field validation,
//! direct regular-grid point location, prepared linear and cubic-alpha field
//! evaluation, topology extraction, deterministic path assembly, optional
//! arc-length regularization, and implicit-level projection. It has no
//! dependency on Plotters, drawing backends, screen coordinates, or viewport
//! clipping.
//!
//! The regular lattice stores finite samples at `i + j + k = n` in the ordering
//! documented by [`RegularTernaryScalarField`]. With `cubic-alpha`, shared
//! directed edge intervals use
//! `y0*(1-t) + y1*t + (1-t)*t*(alpha0 + alpha1*t)`.
//!
//! [`InterpolatedTernaryField`] prepares a field model once for repeated point
//! queries. Its [`FieldSample`] gradients use independent semantic `(a, b)`
//! coordinates with `c = 1-a-b`. Locations on shared grid edges are assigned to
//! one canonical owner, so gradients are deterministic without averaging.
//!
//! Regular grids provide linear and optional cubic-alpha pointwise evaluation,
//! isolines, and linear filled bands. With `irregular-delaunay`, immutable
//! irregular 2-D ternary meshes provide point location and prepared linear
//! fields inside their convex hull. `irregular-cubic-alpha` additionally builds
//! a cached self-consistent edge-alpha field with synchronous Jacobi sweeps;
//! its virtual locations and one interval per canonical mesh edge are prepared
//! once. Irregular linear isolines are available with `irregular-delaunay`;
//! `irregular-cubic-alpha` additionally enables adaptive cubic isolines over a
//! converged prepared field. Irregular filled bands remain intentionally excluded.
//! The maintained numerical validation method and its limits are recorded in
//! [`docs/numerical-validation.md`](../docs/numerical-validation.md).
//! [`PreparedStablePhaseEnsemble`] resamples heterogeneous regular and optional
//! irregular height/secondary sources onto one virtual regular sampling grid.
//! It caches exact affine upper-envelope phase polygons, then produces
//! phase-labelled height or secondary contours with canonical univariant and
//! invariant junctions. Partial evaluator domains use explicit defined/undefined
//! results without extrapolation. The prepared ensemble also constructs a
//! level-free graph of binary/interior invariant nodes and boundary-connected
//! stable univariants, with optional shared path regularization using T_p-T_q=0.
//! Optional verification and global refinement estimate
//! source-to-sampling-grid error; results are exact for the final piecewise-linear
//! sampling-grid representation, not certified for the original source interpolants.
//!
//! [metrics] adds deterministic shared gradient, local quadratic sampled-field,
//! derived-evaluator, edge-jump, and final-contour analysis. Delaunay geometry
//! quality remains specific to [`IrregularTernaryMesh`] behind `irregular-delaunay`;
//! the corrected capability split is documented in
//! [`docs/irregular-mesh-metrics.md`](../docs/irregular-mesh-metrics.md).

pub mod contour;
mod error;
pub mod evaluation;
pub mod field;
pub mod grid;
pub mod interpolation;
#[cfg(feature = "irregular-delaunay")]
pub mod irregular;
pub mod metrics;
mod path;
mod simplex;
pub mod stable;
/// Deterministic analytical fields used by examples and regression tests.
pub mod synthetic;

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
#[cfg(feature = "irregular-delaunay")]
pub use contour::{
    IrregularAdaptiveContourOptions, IrregularContourDiagnostics, IrregularContourError,
    IrregularContourGeometryOptions, IrregularContourInterpolation,
    IrregularContourLevelDiagnostics, IrregularContourOptions, IrregularContourSet,
    IrregularCubicContourSourceDiagnostics,
};
pub use error::{FieldError, GridEvaluationError};
pub use evaluation::{
    FieldEvaluationError, FieldInterpolation, FieldSample, InterpolatedTernaryField,
};
pub use field::{CubicBuildDiagnostics, CubicGridField};
pub use grid::{
    GridTriangle, GridVertexId, LatticeCoordinate, LocatedTriangle, POINT_LOCATION_TOLERANCE,
    PointBoundaryLocation, PointLocationError, RegularGridEdgeId, RegularSamplingTopology,
    RegularTernaryGrid, RegularTernaryScalarField, TriangleEdgeRef,
};
pub use interpolation::{
    BinaryExtrapolation, CubicAlphaBuildOptions, CubicAlphaMethod, CubicBoundaryPolicy,
};
#[cfg(feature = "cubic-alpha")]
pub use metrics::RegularCubicEdgeContinuityMetrics;
pub use metrics::{
    ContourLevelResponseMetrics, ContourPathResponseMetrics, DerivedFieldQuantity,
    DerivedFieldSample, DerivedRegularTernaryField, DistributionError, DistributionQuantiles,
    DistributionSummary, GradientJump, Histogram, HistogramBinning, HistogramError,
    LocalQuadraticError, LocalQuadraticEstimate, LocalQuadraticOptions, MetricWeighting,
    RegularFieldMetrics, RegularGradientJump, RegularTriangleFieldMetrics,
    RegularTriangleOrientation, ScalarFieldDistributionMetrics, TernaryGradient,
};
#[cfg(feature = "irregular-delaunay")]
pub use metrics::{
    DerivedIrregularTernaryField, IrregularEdgeFieldMetrics, IrregularEdgeGeometryMetrics,
    IrregularFieldMetrics, IrregularGradientJump, IrregularMeshMetrics, IrregularMeshSummary,
    IrregularTriangleFieldMetrics, IrregularTriangleGeometryMetrics,
    IrregularVertexGeometryMetrics, TriangleFieldAlignmentMetrics,
};
#[cfg(feature = "irregular-cubic-alpha")]
pub use metrics::{
    IrregularAlphaEdgeMetrics, IrregularAlphaResponseMetrics, IrregularCubicEdgeContinuityMetrics,
};
pub use path::PathRegularizationOptions;
pub use stable::{
    BinaryBoundary, BinaryBoundaryTrace, BinaryBoundaryTraceDiagnostics, BinaryInvariantNode,
    BinaryStableRegion, InteriorInvariantNode, PreparedStablePhaseEnsemble,
    StableBoundaryDiagnostics, StableBoundaryError, StableBoundaryNetwork, StableBoundaryOptions,
    StableContourDiagnostics, StableContourError, StableContourJunction, StableContourJunctionKind,
    StableContourLevel, StableContourPath, StableContourQuantity, StableContourSet,
    StableGridOptions, StableGridVerification, StableInvariantNode, StableInvariantNodeId,
    StableJunctionId, StablePhaseEvaluation, StablePhaseEvaluator, StablePhaseId, StablePhasePair,
    StablePhaseSource, StablePhaseUndefinedReason, StableScalarSource, StableSourceEvaluationError,
    StableUnivariantEndId, StableUnivariantId, StableUnivariantPath,
    StableUnivariantRegularizationDiagnostics, StableVerificationPassDiagnostics,
};

/// Compatibility alias retained for the stable Plotters adapter.
pub type StableUmbrellaOptions = StableGridOptions;
/// Compatibility alias retained for the stable Plotters adapter.
pub type StableUmbrellaVerification = StableGridVerification;

#[cfg(feature = "irregular-delaunay")]
pub use irregular::{
    IRREGULAR_VERTEX_TOLERANCE, InterpolatedIrregularTernaryField, IrregularAlphaConvergence,
    IrregularAlphaSweepOptions, IrregularCubicAlphaBuildError, IrregularCubicAlphaDiagnostics,
    IrregularCubicAlphaOptions, IrregularEdgeId, IrregularFieldError,
    IrregularFieldEvaluationError, IrregularFieldInterpolation, IrregularFieldSample,
    IrregularMeshEdge, IrregularMeshError, IrregularMeshTriangle, IrregularPointBoundaryLocation,
    IrregularPointLocationError, IrregularTernaryMesh, IrregularTernaryScalarField,
    IrregularTriangleId, IrregularVertexId, IrregularVirtualStencilFailure,
    IrregularVirtualStencilSide, LocatedIrregularTriangle, PreparedIrregularTernaryField,
};

pub use synthetic::LiquidusFieldSpec;
