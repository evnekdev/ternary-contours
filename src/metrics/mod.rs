//! Deterministic analysis of ternary sampled fields.
//!
//! Triangulation-quality records describe irregular Delaunay meshes only.
//! Gradient, local quadratic curvature, derived prepared-field evaluation,
//! interpolation response, and contour response use common logical-plane
//! definitions for both regular grids and irregular meshes.

mod curvature;
pub(crate) use curvature::fit_local_quadratic;
mod derived;
mod distribution;
mod field;
pub(crate) use field::ScalarFieldDistributionInput;
mod gradient;
#[cfg(feature = "irregular-delaunay")]
mod irregular;
mod regular;
mod response;

pub use curvature::{LocalQuadraticError, LocalQuadraticEstimate, LocalQuadraticOptions};
#[cfg(feature = "irregular-delaunay")]
pub use derived::DerivedIrregularTernaryField;
pub use derived::{DerivedFieldQuantity, DerivedFieldSample, DerivedRegularTernaryField};
pub use distribution::{
    DistributionError, DistributionQuantiles, DistributionSummary, Histogram, HistogramBinning,
    HistogramError,
};
pub use field::{GradientJump, MetricWeighting, ScalarFieldDistributionMetrics};
pub use gradient::TernaryGradient;
#[cfg(feature = "irregular-delaunay")]
pub use irregular::{
    IrregularEdgeFieldMetrics, IrregularEdgeGeometryMetrics, IrregularFieldMetrics,
    IrregularGradientJump, IrregularMeshMetrics, IrregularMeshSummary,
    IrregularTriangleFieldMetrics, IrregularTriangleGeometryMetrics,
    IrregularVertexGeometryMetrics, TriangleFieldAlignmentMetrics,
};
pub use regular::{
    RegularFieldMetrics, RegularGradientJump, RegularTriangleFieldMetrics,
    RegularTriangleOrientation,
};
#[cfg(feature = "irregular-cubic-alpha")]
pub use response::IrregularCubicEdgeContinuityMetrics;
#[cfg(feature = "cubic-alpha")]
pub use response::RegularCubicEdgeContinuityMetrics;
pub use response::{ContourLevelResponseMetrics, ContourPathResponseMetrics};
#[cfg(feature = "irregular-cubic-alpha")]
pub use response::{IrregularAlphaEdgeMetrics, IrregularAlphaResponseMetrics};
