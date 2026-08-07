use core::fmt;

#[cfg(feature = "irregular-delaunay")]
use crate::IrregularFieldEvaluationError;
use crate::{FieldError, FieldEvaluationError, GridVertexId, TernaryCoordinate};

use super::{StableContourQuantity, StableJunctionId, StablePhaseId};

/// Context-preserving source evaluator failure.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StableSourceEvaluationError {
    /// Regular source preparation or evaluation failed.
    Regular(FieldEvaluationError),
    /// Irregular source preparation or evaluation failed.
    #[cfg(feature = "irregular-delaunay")]
    Irregular(IrregularFieldEvaluationError),
}

impl fmt::Display for StableSourceEvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regular(error) => write!(formatter, "regular source: {error}"),
            #[cfg(feature = "irregular-delaunay")]
            Self::Irregular(error) => write!(formatter, "irregular source: {error}"),
        }
    }
}

impl std::error::Error for StableSourceEvaluationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Regular(error) => Some(error),
            #[cfg(feature = "irregular-delaunay")]
            Self::Irregular(error) => Some(error),
        }
    }
}

/// Context for a genuine post-canonicalization physical contour retrace.
///
/// This payload is boxed in [`StableContourError`] so detailed tracing does
/// not inflate every stable-contour result type.
#[derive(Clone, Debug, PartialEq)]
pub struct NonForwardPathAssemblyContext {
    pub level: f64,
    pub phase: StablePhaseId,
    /// Previous, current, and proposed-next physical graph points. These
    /// identify an actual post-canonicalization retrace rather than an input
    /// producer orientation.
    pub point: TernaryCoordinate,
    pub previous: Option<TernaryCoordinate>,
    pub next: Option<TernaryCoordinate>,
    pub triangle: Option<usize>,
    pub previous_junction: Option<StableJunctionId>,
    pub current_junction: Option<StableJunctionId>,
    pub next_junction: Option<StableJunctionId>,
}

/// Context for incompatible records that occupy one physical contour edge.
///
/// This payload is boxed in [`StableContourError`] so preserving source-edge
/// diagnostics does not inflate every stable-contour result type.
#[derive(Clone, Debug, PartialEq)]
pub struct IncompatiblePhysicalContourEdgeContext {
    pub level: f64,
    pub phase: StablePhaseId,
    pub triangle: usize,
    pub existing_triangle: usize,
    pub start: TernaryCoordinate,
    pub end: TernaryCoordinate,
    pub existing_start: TernaryCoordinate,
    pub existing_end: TernaryCoordinate,
    pub start_junction: Option<StableJunctionId>,
    pub end_junction: Option<StableJunctionId>,
    pub existing_start_junction: Option<StableJunctionId>,
    pub existing_end_junction: Option<StableJunctionId>,
}

/// Failure while preparing or extracting stable phase contours.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StableContourError {
    EmptyPhaseEnsemble,
    NoPhaseDefined {
        composition: [f64; 3],
    },
    DuplicatePhaseId {
        phase: StablePhaseId,
    },
    MissingSecondaryScalar {
        phase: StablePhaseId,
    },
    MismatchedPhaseTopology {
        phase: StablePhaseId,
    },
    UnsupportedSourceFeature {
        phase: StablePhaseId,
        quantity: StableContourQuantity,
        feature: &'static str,
    },
    IncompleteSourceCoverage {
        phase: StablePhaseId,
        sampling_vertex: GridVertexId,
        composition: [f64; 3],
    },
    SourcePreparation {
        phase: StablePhaseId,
        quantity: StableContourQuantity,
        source: Box<StableSourceEvaluationError>,
    },
    SourceEvaluation {
        phase: StablePhaseId,
        quantity: StableContourQuantity,
        composition: [f64; 3],
        source: Box<StableSourceEvaluationError>,
    },
    NonFiniteSourceEvaluation {
        phase: StablePhaseId,
        quantity: StableContourQuantity,
        composition: [f64; 3],
    },
    InvalidStableGridOptions {
        message: String,
    },
    SamplingSubdivisionOverflow,
    SamplingResolutionInsufficient {
        subdivisions: usize,
        unresolved_triangles: usize,
        worst_triangle: Option<usize>,
        maximum_height_error: f64,
        maximum_secondary_error: f64,
    },
    StablePolygonFailure {
        triangle: usize,
        phase: StablePhaseId,
    },
    /// Four or more liquidus phases were tied at one ordinary fixed-pressure
    /// ternary invariant-level event. Such a node is overdetermined and is
    /// never promoted into contour topology.
    OverdeterminedInvariantLevel {
        level: f64,
        point: TernaryCoordinate,
        phases: Vec<StablePhaseId>,
    },
    AmbiguousPathAssembly {
        level: f64,
        phase: StablePhaseId,
        degree: usize,
    },
    /// A continuously verified A-B transfer has no half-edge for one phase.
    MissingTransferExit {
        level: f64,
        point: TernaryCoordinate,
        phases: [StablePhaseId; 2],
        present_phase: Option<StablePhaseId>,
    },
    /// A continuously verified transfer has an incidence other than one A and
    /// one B half-edge.
    AmbiguousTransferIncidence {
        level: f64,
        point: TernaryCoordinate,
        phases: [StablePhaseId; 2],
        phase_a_degree: usize,
        phase_b_degree: usize,
    },
    /// A half-edge attached to a transfer belongs to neither transfer phase.
    TransferPhaseMismatch {
        level: f64,
        point: TernaryCoordinate,
        expected: [StablePhaseId; 2],
        actual: StablePhaseId,
    },
    NonMonotoneLocalEvents {
        triangle: usize,
        phase: StablePhaseId,
        previous_parameter: f64,
        next_parameter: f64,
    },
    NonForwardPathAssembly {
        context: Box<NonForwardPathAssemblyContext>,
    },
    /// Two phase-local segment records describe the same physical contour
    /// edge, but disagree about its endpoint or transfer semantics. Such
    /// records cannot be canonicalized safely.
    IncompatiblePhysicalContourEdge {
        context: Box<IncompatiblePhysicalContourEdgeContext>,
    },
    DirectedTraversalCycle {
        level: f64,
        phase: StablePhaseId,
        triangle: usize,
    },
    PositiveAreaHeightTie {
        triangle: usize,
        phases: [StablePhaseId; 2],
    },
    CoincidentTargetSegment {
        level: f64,
        triangle: usize,
        phases: Vec<StablePhaseId>,
        start: TernaryCoordinate,
        end: TernaryCoordinate,
    },
    NonFiniteStableGeometry {
        triangle: usize,
    },
    NonFiniteLevel {
        index: usize,
        value: f64,
    },
    DuplicateLevel {
        first: f64,
        second: f64,
    },
    GridConstruction(FieldError),
}

impl StableContourError {
    pub(crate) fn invalid_option(message: impl Into<String>) -> Self {
        Self::InvalidStableGridOptions {
            message: message.into(),
        }
    }
}

impl fmt::Display for StableContourError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPhaseEnsemble => formatter.write_str("stable phase ensemble is empty"),
            Self::NoPhaseDefined { composition } => write!(
                formatter,
                "no stable phase is defined at composition {composition:?}"
            ),
            Self::DuplicatePhaseId { phase } => {
                write!(formatter, "stable phase ID {phase:?} is duplicated")
            }
            Self::MissingSecondaryScalar { phase } => write!(
                formatter,
                "phase {phase:?} has no secondary scalar in secondary-contour mode"
            ),
            Self::MismatchedPhaseTopology { phase } => write!(
                formatter,
                "phase {phase:?} height and secondary sources do not share exact topology"
            ),
            Self::UnsupportedSourceFeature {
                phase,
                quantity,
                feature,
            } => write!(
                formatter,
                "phase {phase:?} {quantity:?} source requires unavailable feature `{feature}`"
            ),
            Self::IncompleteSourceCoverage {
                phase,
                sampling_vertex,
                composition,
            } => write!(
                formatter,
                "phase {phase:?} does not cover sampling-grid vertex {sampling_vertex:?} at {composition:?}"
            ),
            Self::SourcePreparation {
                phase,
                quantity,
                source,
            } => write!(
                formatter,
                "failed to prepare phase {phase:?} {quantity:?} source: {source}"
            ),
            Self::SourceEvaluation {
                phase,
                quantity,
                composition,
                source,
            } => write!(
                formatter,
                "failed to evaluate phase {phase:?} {quantity:?} source at {composition:?}: {source}"
            ),
            Self::NonFiniteSourceEvaluation {
                phase,
                quantity,
                composition,
            } => write!(
                formatter,
                "phase {phase:?} {quantity:?} source produced a non-finite value at {composition:?}"
            ),
            Self::InvalidStableGridOptions { message } => {
                write!(formatter, "invalid stable sampling-grid options: {message}")
            }
            Self::SamplingSubdivisionOverflow => {
                formatter.write_str("sampling-grid subdivision refinement overflowed")
            }
            Self::SamplingResolutionInsufficient {
                subdivisions,
                unresolved_triangles,
                worst_triangle,
                maximum_height_error,
                maximum_secondary_error,
            } => write!(
                formatter,
                "sampling-grid resolution {subdivisions} left {unresolved_triangles} unresolved triangles (worst {worst_triangle:?}, height error {maximum_height_error}, secondary error {maximum_secondary_error})"
            ),
            Self::StablePolygonFailure { triangle, phase } => write!(
                formatter,
                "stable polygon clipping failed in sampling-grid triangle {triangle} for phase {phase:?}"
            ),
            Self::OverdeterminedInvariantLevel {
                level,
                point,
                phases,
            } => write!(
                formatter,
                "overdetermined ternary invariant-level event at level {level} near {:?}: {phases:?}",
                point.as_array()
            ),
            Self::AmbiguousPathAssembly {
                level,
                phase,
                degree,
            } => write!(
                formatter,
                "stable path assembly is ambiguous at level {level} for phase {phase:?} (degree {degree})"
            ),
            Self::MissingTransferExit {
                level,
                point,
                phases,
                present_phase,
            } => write!(
                formatter,
                "stable contour transfer {:?} at level {level} near {:?} is missing an exit (present {present_phase:?})",
                phases,
                point.as_array()
            ),
            Self::AmbiguousTransferIncidence {
                level,
                point,
                phases,
                phase_a_degree,
                phase_b_degree,
            } => write!(
                formatter,
                "stable contour transfer {:?} at level {level} near {:?} has ambiguous incidence ({phase_a_degree}, {phase_b_degree})",
                phases,
                point.as_array()
            ),
            Self::TransferPhaseMismatch {
                level,
                point,
                expected,
                actual,
            } => write!(
                formatter,
                "stable contour transfer {:?} at level {level} near {:?} received phase {actual:?}",
                expected,
                point.as_array()
            ),
            Self::NonMonotoneLocalEvents {
                triangle,
                phase,
                previous_parameter,
                next_parameter,
            } => write!(
                formatter,
                "local contour events do not make forward progress in triangle {triangle} for phase {phase:?}: {next_parameter} follows {previous_parameter}"
            ),
            Self::NonForwardPathAssembly { context } => write!(
                formatter,
                "stable path continuation retraces after physical-edge canonicalization at level {} for phase {:?} near {:?}; previous={:?}, next={:?}, triangle={:?}, junctions={:?}/{:?}/{:?}",
                context.level,
                context.phase,
                context.point.as_array(),
                context.previous.map(TernaryCoordinate::as_array),
                context.next.map(TernaryCoordinate::as_array),
                context.triangle,
                context.previous_junction,
                context.current_junction,
                context.next_junction,
            ),
            Self::IncompatiblePhysicalContourEdge { context } => write!(
                formatter,
                "phase-local contour records describe incompatible coincident physical edges at level {} for phase {:?} (triangles {} and {})",
                context.level, context.phase, context.existing_triangle, context.triangle,
            ),
            Self::DirectedTraversalCycle {
                level,
                phase,
                triangle,
            } => write!(
                formatter,
                "stable path assembly revisited a directed state at level {level} for phase {phase:?} from triangle {triangle}"
            ),
            Self::PositiveAreaHeightTie { triangle, phases } => write!(
                formatter,
                "phases {:?} have an unresolved positive-area stable height tie in triangle {triangle}",
                phases
            ),
            Self::CoincidentTargetSegment {
                level,
                triangle,
                phases,
                start,
                end,
            } => write!(
                formatter,
                "target level {level} is coincident with a stable boundary in triangle {triangle} for phases {phases:?}, from {:?} to {:?}",
                start.as_array(),
                end.as_array()
            ),
            Self::NonFiniteStableGeometry { triangle } => write!(
                formatter,
                "stable geometry became non-finite in sampling-grid triangle {triangle}"
            ),
            Self::NonFiniteLevel { index, value } => {
                write!(
                    formatter,
                    "stable contour level {index} is non-finite: {value}"
                )
            }
            Self::DuplicateLevel { first, second } => write!(
                formatter,
                "stable contour levels {first} and {second} coincide within value tolerance"
            ),
            Self::GridConstruction(error) => {
                write!(formatter, "sampling grid construction failed: {error}")
            }
        }
    }
}

impl std::error::Error for StableContourError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SourcePreparation { source, .. } | Self::SourceEvaluation { source, .. } => {
                Some(source)
            }
            Self::GridConstruction(error) => Some(error),
            _ => None,
        }
    }
}

impl From<FieldError> for StableContourError {
    fn from(error: FieldError) -> Self {
        Self::GridConstruction(error)
    }
}
