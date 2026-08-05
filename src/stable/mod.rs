//! Stable phase contours on a common virtual regular sampling grid.

mod boundary;
mod clip;
mod diagnostics;
mod error;
mod options;
mod partition;
mod paths;
mod prepare;
mod sample;
mod segments;
mod source;
mod verify;

#[cfg(test)]
mod tests;

pub use boundary::{
    BinaryBoundary, BinaryBoundaryTrace, BinaryBoundaryTraceDiagnostics, BinaryInvariantNode,
    BinaryStableRegion, BinaryTransitionUnavailableReason, InteriorInvariantNode,
    PartialBinaryTransition, StableBoundaryDiagnostics, StableBoundaryError, StableBoundaryNetwork,
    StableBoundaryOptions, StableInvariantNode, StableInvariantNodeId, StablePhasePair,
    StableUnivariantEndId, StableUnivariantId, StableUnivariantPath,
    StableUnivariantRegularizationDiagnostics,
};
pub use diagnostics::{StableContourDiagnostics, StableVerificationPassDiagnostics};
pub use error::{StableContourError, StableSourceEvaluationError};
pub use options::{StableGridOptions, StableGridVerification};
pub use prepare::PreparedStablePhaseEnsemble;
pub use source::{
    StableContourQuantity, StablePhaseEvaluation, StablePhaseEvaluator, StablePhaseId,
    StablePhaseSource, StablePhaseUndefinedReason, StableScalarSource,
};

use crate::TernaryCoordinate;

/// Dense identifier for a junction within one stable contour level.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableJunctionId(pub usize);

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
}

/// One canonical stable-boundary event at a contour level.
#[derive(Clone, Debug, PartialEq)]
pub struct StableContourJunction {
    /// Dense identifier matching path endpoint references.
    pub id: StableJunctionId,
    /// Canonical semantic composition.
    pub point: TernaryCoordinate,
    /// Sorted stable phases tied in height at this point.
    pub phases: Vec<StablePhaseId>,
    /// Thermodynamic/topological classification.
    pub kind: StableContourJunctionKind,
}

/// Classification of a stable contour endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StableContourJunctionKind {
    /// Exactly two stable phases share an iso-height endpoint.
    Univariant,
    /// Three or more stable phases share an iso-height endpoint.
    Invariant,
    /// A phase-specific secondary contour reaches a stable height boundary.
    StableBoundaryContact,
}
