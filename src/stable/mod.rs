//! Stable phase contours on a common virtual regular sampling grid.

mod boundary;
mod clip;
mod continuous;
mod contour_signature;
mod diagnostics;
mod error;
mod options;
mod partition;
mod paths;
mod prepare;
mod sample;
mod segments;
mod signature;
mod source;
mod verify;

#[cfg(test)]
mod tests;

pub use boundary::{
    BinaryBoundary, BinaryBoundaryTrace, BinaryBoundaryTraceDiagnostics, BinaryInvariantNode,
    BinaryStableRegion, BinaryTransitionUnavailableReason, InteriorInvariantNode,
    PartialBinaryTransition, StableBoundaryDiagnostics, StableBoundaryError, StableBoundaryNetwork,
    StableBoundaryOptions, StableInvariantNode, StableInvariantNodeId, StableInvariantVerification,
    StablePathGeometryState, StablePhasePair, StableTruncatedUnivariantPath, StableUnivariantEndId,
    StableUnivariantId, StableUnivariantPath, StableUnivariantRegularizationDiagnostics,
    StableUnivariantRegularizationFailure, UnivariantTermination,
};
pub use contour_signature::{
    StableContourComparison, StableContourHalfEdgeSignature, StableContourJunctionSignature,
    StableContourLevelSignature, StableContourPathSignature, StableContourSignature,
    compare_stable_contours, stable_contour_signature,
};
pub use diagnostics::{StableContourDiagnostics, StableVerificationPassDiagnostics};
pub use error::{StableContourError, StableSourceEvaluationError};
pub use options::{StableGridOptions, StableGridVerification};
pub use prepare::PreparedStablePhaseEnsemble;
pub use signature::{
    StableEdgeSignature, StableNodeGeometrySignature, StableNodeKindSignature, StableNodeSignature,
    StableTopologyComparison, StableTopologyComparisonMode, StableTopologySignature,
    StableTruncatedBranchSignature, assert_same_stable_topology, compare_stable_topology,
    stable_topology_signature,
};
pub use source::{
    StableContourQuantity, StablePhaseEvaluation, StablePhaseEvaluator, StablePhaseId,
    StablePhaseSource, StablePhaseUndefinedReason, StableScalarSource,
};

use crate::TernaryCoordinate;

/// Dense identifier for a junction within one stable contour level.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableJunctionId(pub usize);

/// Dense identifier for a phase-labelled contour half-edge within one level.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableContourHalfEdgeId(pub usize);

/// Stable-phase contour geometry for all requested levels.
#[derive(Clone, Debug, PartialEq)]
pub struct StableContourSet {
    /// Scalar quantity traced by this result.
    pub quantity: StableContourQuantity,
    /// Levels sorted into deterministic ascending order.
    pub levels: Vec<StableContourLevel>,
    /// Preparation, verification, partition, and extraction diagnostics.
    pub diagnostics: StableContourDiagnostics,
}

/// Phase-labelled paths and junctions for one scalar level.
#[derive(Clone, Debug, PartialEq)]
pub struct StableContourLevel {
    /// Requested finite scalar value.
    pub value: f64,
    /// Deterministically ordered paths, never joined across phase IDs.
    pub paths: Vec<StableContourPath>,
    /// Canonical stable-boundary contacts used by path endpoints.
    pub junctions: Vec<StableContourJunction>,
    /// Explicit incidence records. A regular transfer has one half-edge for
    /// each of its two phases; phase ownership is never inferred from a
    /// coincident pair of endpoint coordinates.
    pub half_edges: Vec<StableContourHalfEdge>,
}

/// One path owned by a single stable phase.
#[derive(Clone, Debug, PartialEq)]
pub struct StableContourPath {
    /// Stable phase that owns every segment in this path.
    pub phase: StablePhaseId,
    /// Ordered semantic A/B/C coordinates, without a duplicate closing point.
    pub points: Vec<TernaryCoordinate>,
    /// Whether the final point connects back to the first.
    pub closed: bool,
    /// Junction at the first point, when the path starts on a stable boundary.
    pub start_junction: Option<StableJunctionId>,
    /// Junction at the last point, when the path ends on a stable boundary.
    pub end_junction: Option<StableJunctionId>,
    /// The selected geometry provenance. Contour regularization is deliberately
    /// post-topology; a recoverable failure must retain the raw route.
    pub geometry_state: StableContourPathGeometryState,
}

/// One canonical stable-boundary event at a contour level.
#[derive(Clone, Debug, PartialEq)]
pub struct StableContourJunction {
    /// Dense identifier matching path endpoint references.
    pub id: StableJunctionId,
    /// Canonical semantic composition.
    pub point: TernaryCoordinate,
    /// Sorted stable phases tied in height at this point. An ordinary transfer
    /// has exactly two; a level coincident with an existing ternary invariant
    /// is represented by the dedicated `InvariantLevelCoincidence` variant.
    pub phases: Vec<StablePhaseId>,
    /// Thermodynamic/topological classification.
    pub kind: StableContourJunctionKind,
    /// Level-free accepted stable-boundary branch used to isolate this event.
    pub branch: Option<StableUnivariantId>,
    /// Existing ternary invariant for an invariant-level coincidence.
    pub invariant: Option<StableInvariantNodeId>,
    /// Continuous source-evaluation evidence. `None` is retained only for the
    /// compatibility sampled-contour entry point.
    pub verification: Option<StableContourJunctionVerification>,
}

/// Classification of a stable contour endpoint.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum StableContourJunctionKind {
    /// Compatibility classification for an affine sampled two-phase endpoint.
    /// New stable-boundary-backed calculations emit `RegularTransfer`.
    Univariant,
    /// Compatibility classification for an affine sampled multi-phase endpoint.
    /// New calculations use `InvariantLevelCoincidence` only for a canonical
    /// existing ternary invariant at its exact height.
    Invariant,
    /// Compatibility classification for an affine secondary contact.
    StableBoundaryContact,
    /// A continuously verified A-to-B phase transfer.
    RegularTransfer,
    /// A requested height level coincides with one existing ternary invariant.
    InvariantLevelCoincidence,
    /// A secondary phase contour reaches a boundary, but the other phase does
    /// not have the requested secondary value; no phase switch is fabricated.
    OneSidedSecondaryContact,
    /// A non-transverse height/contour contact.
    TangentBoundaryContact,
    /// A contour or stable-boundary branch ended at unavailable source data.
    DomainTruncated,
    /// A zero-gradient, coincident, or unresolved local event.
    Degenerate,
}

/// Geometry provenance for a stable contour route.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StableContourPathGeometryState {
    Raw,
    Regularized,
    RawFallback,
}

/// A phase-labelled incidence of a path endpoint at one junction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableContourHalfEdge {
    pub id: StableContourHalfEdgeId,
    pub phase: StablePhaseId,
    pub path_index: usize,
    pub at_start: bool,
    pub junction: StableJunctionId,
}

/// Continuous numerical evidence retained with a stable contour transfer.
#[derive(Clone, Debug, PartialEq)]
pub struct StableContourJunctionVerification {
    pub height_values: Vec<(StablePhaseId, f64)>,
    pub quantity_values: Vec<(StablePhaseId, f64)>,
    pub equality_residual: f64,
    pub level_residuals: Vec<(StablePhaseId, f64)>,
    pub stability_margin: f64,
    pub sampling_triangle: Option<usize>,
    pub branch: Option<StableUnivariantId>,
    pub solver_iterations: usize,
}
