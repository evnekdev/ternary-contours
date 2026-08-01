use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::{FieldError, PathRegularizationOptions, RegularSamplingTopology, TernaryCoordinate};

use super::{
    StableContourError, StablePhaseEvaluation, StablePhaseId,
    sample::{PreparedSourceLayer, evaluate_layer_at_point},
    source::ScalarRole,
};

/// Canonically oriented outer binary boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BinaryBoundary {
    Ab,
    Bc,
    Ca,
}

impl BinaryBoundary {
    pub const ALL: [Self; 3] = [Self::Ab, Self::Bc, Self::Ca];

    /// Convert a canonical boundary parameter into semantic A/B/C composition.
    pub fn composition(self, parameter: f64) -> Result<TernaryCoordinate, StableBoundaryError> {
        if !parameter.is_finite() || !(0.0..=1.0).contains(&parameter) {
            return Err(StableBoundaryError::InvalidBoundaryParameter {
                boundary: self,
                parameter,
            });
        }
        Ok(self.composition_unchecked(parameter))
    }

    /// Recover the canonical parameter of a point on this boundary.
    pub fn parameter(self, point: TernaryCoordinate) -> Result<f64, StableBoundaryError> {
        let [a, b, c] = point.as_array();
        let tolerance = 1.0e-10;
        let parameter = match self {
            Self::Ab if c.abs() <= tolerance => b,
            Self::Bc if a.abs() <= tolerance => c,
            Self::Ca if b.abs() <= tolerance => a,
            _ => {
                return Err(StableBoundaryError::PointNotOnBoundary {
                    boundary: self,
                    point,
                });
            }
        };
        if parameter < -tolerance || parameter > 1.0 + tolerance {
            return Err(StableBoundaryError::PointNotOnBoundary {
                boundary: self,
                point,
            });
        }
        Ok(parameter.clamp(0.0, 1.0))
    }

    pub(crate) fn composition_unchecked(self, parameter: f64) -> TernaryCoordinate {
        match self {
            Self::Ab => TernaryCoordinate::new(1.0 - parameter, parameter, 0.0),
            Self::Bc => TernaryCoordinate::new(0.0, 1.0 - parameter, parameter),
            Self::Ca => TernaryCoordinate::new(parameter, 0.0, 1.0 - parameter),
        }
    }
}

/// Canonical unordered pair of distinct phase identifiers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StablePhasePair {
    pub first: StablePhaseId,
    pub second: StablePhaseId,
}

impl StablePhasePair {
    /// Construct a phase-ID-order-independent pair.
    pub const fn new(first: StablePhaseId, second: StablePhaseId) -> Self {
        if first.0 <= second.0 {
            Self { first, second }
        } else {
            Self {
                first: second,
                second: first,
            }
        }
    }

    pub const fn contains(self, phase: StablePhaseId) -> bool {
        self.first.0 == phase.0 || self.second.0 == phase.0
    }
}

/// Dense identifier for one invariant node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableInvariantNodeId(pub usize);

/// Dense identifier for one complete stable univariant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableUnivariantId(pub usize);

/// Dense identifier for one pending univariant half-edge.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableUnivariantEndId(pub usize);

/// One stable region in canonical boundary order.
#[derive(Clone, Debug, PartialEq)]
pub struct BinaryStableRegion {
    pub stable_phases: Vec<StablePhaseId>,
    pub parameter_range: [f64; 2],
}

/// Stable outer-boundary invariant.
#[derive(Clone, Debug, PartialEq)]
pub struct BinaryInvariantNode {
    pub id: StableInvariantNodeId,
    pub boundary: BinaryBoundary,
    pub boundary_parameter: f64,
    pub point: TernaryCoordinate,
    pub temperature: f64,
    pub phases: Vec<StablePhaseId>,
    pub left_stable_phase: StablePhaseId,
    pub right_stable_phase: StablePhaseId,
}

/// Stable inner invariant.
#[derive(Clone, Debug, PartialEq)]
pub struct InteriorInvariantNode {
    pub id: StableInvariantNodeId,
    pub point: TernaryCoordinate,
    pub temperature: f64,
    pub phases: Vec<StablePhaseId>,
}

/// Node in the level-free stable boundary graph.
#[derive(Clone, Debug, PartialEq)]
pub enum StableInvariantNode {
    Binary(BinaryInvariantNode),
    Interior(InteriorInvariantNode),
}

impl StableInvariantNode {
    pub const fn id(&self) -> StableInvariantNodeId {
        match self {
            Self::Binary(node) => node.id,
            Self::Interior(node) => node.id,
        }
    }

    pub const fn point(&self) -> TernaryCoordinate {
        match self {
            Self::Binary(node) => node.point,
            Self::Interior(node) => node.point,
        }
    }

    pub const fn temperature(&self) -> f64 {
        match self {
            Self::Binary(node) => node.temperature,
            Self::Interior(node) => node.temperature,
        }
    }

    pub fn phases(&self) -> &[StablePhaseId] {
        match self {
            Self::Binary(node) => &node.phases,
            Self::Interior(node) => &node.phases,
        }
    }
}

/// Summary of optional post-topology univariant regularization.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StableUnivariantRegularizationDiagnostics {
    pub raw_point_count: usize,
    pub final_point_count: usize,
    pub accepted_projections: usize,
    pub projection_iterations: usize,
    pub backtracked_projections: usize,
    pub rejected_unstable_projections: usize,
    pub rejected_undefined_projections: usize,
    pub sampling_triangle_relocations: usize,
    pub maximum_pair_residual: f64,
    pub raw_logical_length: f64,
    pub final_logical_length: f64,
    pub spacing_cv_before: f64,
    pub spacing_cv_after: f64,
}
/// One complete phase-pair path between invariant nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct StableUnivariantPath {
    pub id: StableUnivariantId,
    pub phases: StablePhasePair,
    pub start: StableInvariantNodeId,
    pub end: StableInvariantNodeId,
    pub points: Vec<TernaryCoordinate>,
    pub temperatures: Vec<f64>,
    /// Present when optional post-topology regularization was requested.
    pub regularization: Option<StableUnivariantRegularizationDiagnostics>,
}

/// Ordered discovery result for one outer edge.
#[derive(Clone, Debug, PartialEq)]
pub struct BinaryBoundaryTrace {
    pub boundary: BinaryBoundary,
    pub regions: Vec<BinaryStableRegion>,
    pub invariants: Vec<BinaryInvariantNode>,
    pub diagnostics: BinaryBoundaryTraceDiagnostics,
}

/// Deterministic binary-discovery counters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BinaryBoundaryTraceDiagnostics {
    pub samples_evaluated: usize,
    pub full_phase_sweeps: usize,
    pub pair_only_evaluations: usize,
    pub cached_evaluations_reused: usize,
    pub intervals_refined: usize,
    pub intermediate_phases_inserted: usize,
    pub metastable_pairwise_roots_rejected: usize,
    pub invariants_emitted: usize,
    pub higher_order_invariants: usize,
}

/// Stable-boundary construction controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StableBoundaryOptions {
    pub binary_initial_subdivisions: usize,
    pub binary_maximum_depth: u8,
    pub binary_parameter_tolerance: f64,
    pub temperature_tolerance: f64,
    pub stability_tolerance: f64,
    pub geometry_tolerance: f64,
    pub local_competitor_margin: f64,
    pub local_maximum_subdivision_depth: u8,
    pub minimum_segment_parameter_width: f64,
    pub maximum_trace_steps: usize,
    /// Optional quantity-independent cleanup, redistribution, and projection.
    pub regularization: Option<PathRegularizationOptions>,
}

impl Default for StableBoundaryOptions {
    fn default() -> Self {
        Self {
            binary_initial_subdivisions: 16,
            binary_maximum_depth: 6,
            binary_parameter_tolerance: 1.0e-10,
            temperature_tolerance: 1.0e-9,
            stability_tolerance: 1.0e-9,
            geometry_tolerance: 1.0e-9,
            local_competitor_margin: 1.0e-7,
            local_maximum_subdivision_depth: 8,
            minimum_segment_parameter_width: 1.0e-10,
            maximum_trace_steps: 100_000,
            regularization: None,
        }
    }
}

impl StableBoundaryOptions {
    pub fn validate(self) -> Result<(), StableBoundaryError> {
        if self.binary_initial_subdivisions == 0
            || !self.binary_initial_subdivisions.is_power_of_two()
        {
            return Err(StableBoundaryError::InvalidOptions {
                message: "binary_initial_subdivisions must be a positive power of two".into(),
            });
        }
        if self.binary_maximum_depth > 32 || self.local_maximum_subdivision_depth > 32 {
            return Err(StableBoundaryError::InvalidOptions {
                message: "subdivision depths must not exceed 32".into(),
            });
        }
        if self.maximum_trace_steps == 0 {
            return Err(StableBoundaryError::InvalidOptions {
                message: "maximum_trace_steps must be positive".into(),
            });
        }
        for (name, value, positive) in [
            (
                "binary_parameter_tolerance",
                self.binary_parameter_tolerance,
                true,
            ),
            ("temperature_tolerance", self.temperature_tolerance, true),
            ("stability_tolerance", self.stability_tolerance, false),
            ("geometry_tolerance", self.geometry_tolerance, true),
            (
                "local_competitor_margin",
                self.local_competitor_margin,
                false,
            ),
            (
                "minimum_segment_parameter_width",
                self.minimum_segment_parameter_width,
                true,
            ),
        ] {
            if !value.is_finite() || value < 0.0 || (positive && value == 0.0) {
                return Err(StableBoundaryError::InvalidOptions {
                    message: format!(
                        "{name} must be finite and {}",
                        if positive { "positive" } else { "nonnegative" }
                    ),
                });
            }
        }
        if let Some(regularization) = self.regularization {
            crate::path::validate_regularization(regularization).map_err(|error| {
                StableBoundaryError::InvalidOptions {
                    message: format!("invalid path regularization: {error:?}"),
                }
            })?;
        }
        self.binary_initial_subdivisions
            .checked_shl(self.binary_maximum_depth.into())
            .ok_or_else(|| StableBoundaryError::InvalidOptions {
                message: "binary discovery subdivision count overflowed".into(),
            })?;
        Ok(())
    }
}

/// Deterministic graph construction diagnostics.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StableBoundaryDiagnostics {
    pub binary_boundaries_scanned: usize,
    pub binary_samples_evaluated: usize,
    pub binary_full_phase_sweeps: usize,
    pub binary_pair_only_evaluations: usize,
    pub binary_intervals_refined: usize,
    pub binary_intermediate_phases_inserted: usize,
    pub binary_metastable_pairwise_roots_rejected: usize,
    pub binary_invariants_emitted: usize,
    pub binary_higher_order_invariants: usize,
    pub sampling_vertices: usize,
    pub sampling_edges: usize,
    pub sampling_triangles: usize,
    pub pending_ends_initially_created: usize,
    pub pending_ends_created_at_interior_invariants: usize,
    pub pending_ends_consumed: usize,
    pub univariant_traces_started: usize,
    pub completed_univariants: usize,
    pub sampling_edge_crossings: usize,
    pub sampling_vertex_crossings: usize,
    pub reused_canonical_edge_hits: usize,
    pub local_competitor_refinements: usize,
    pub interior_invariant_candidates: usize,
    pub interior_invariants_accepted: usize,
    pub known_invariants_revisited: usize,
    pub metastable_invariant_candidates_rejected: usize,
    pub directed_traversal_rejections: usize,
    pub maximum_trace_length: usize,
}

/// Boundary-connected stable invariant and univariant graph.
#[derive(Clone, Debug, PartialEq)]
pub struct StableBoundaryNetwork {
    pub nodes: Vec<StableInvariantNode>,
    pub univariants: Vec<StableUnivariantPath>,
    pub binary_traces: Vec<BinaryBoundaryTrace>,
    pub diagnostics: StableBoundaryDiagnostics,
    incidence: Vec<Vec<StableUnivariantId>>,
}

impl StableBoundaryNetwork {
    pub fn incident_univariants(
        &self,
        node: StableInvariantNodeId,
    ) -> Result<&[StableUnivariantId], StableBoundaryError> {
        self.incidence.get(node.0).map(Vec::as_slice).ok_or(
            StableBoundaryError::MalformedGraphConnectivity {
                message: "invariant node ID is outside the network".into(),
            },
        )
    }
}

/// Typed stable-boundary discovery or traversal failure.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StableBoundaryError {
    InvalidOptions {
        message: String,
    },
    InvalidBoundaryParameter {
        boundary: BinaryBoundary,
        parameter: f64,
    },
    PointNotOnBoundary {
        boundary: BinaryBoundary,
        point: TernaryCoordinate,
    },
    NoPhaseDefined {
        boundary: Option<BinaryBoundary>,
        parameter: Option<f64>,
        point: TernaryCoordinate,
    },
    BinaryDiscoveryResolutionExhausted {
        boundary: BinaryBoundary,
        bracket: [f64; 2],
    },
    NoOverlappingStablePhaseDomains {
        boundary: BinaryBoundary,
        left_phase: StablePhaseId,
        right_phase: StablePhaseId,
        bracket: [f64; 2],
    },
    NonDeterministicRepeatedEvaluation {
        phase: StablePhaseId,
        point: TernaryCoordinate,
    },
    InvalidRootBracket {
        boundary: BinaryBoundary,
        phases: StablePhasePair,
        bracket: [f64; 2],
    },
    PairwiseRootRefinementExhausted {
        boundary: BinaryBoundary,
        phases: StablePhasePair,
        bracket: [f64; 2],
    },
    MetastableTransitionUnresolved {
        boundary: BinaryBoundary,
        phases: StablePhasePair,
        bracket: [f64; 2],
    },
    InvalidRegularGridTopology {
        message: String,
    },
    InconsistentCanonicalEdgeHit {
        edge: usize,
    },
    RepeatedDirectedEdgeTraversal {
        edge: usize,
        triangle: usize,
    },
    RepeatedTriangleTransition {
        triangle: usize,
    },
    TraceStepLimitExceeded {
        start: StableUnivariantEndId,
    },
    UnivariantLeftStablePairRegion {
        phases: StablePhasePair,
        point: TernaryCoordinate,
    },
    InvariantSolveOutsideTriangle {
        triangle: usize,
    },
    InvariantRefinementExhausted {
        triangle: usize,
    },
    IncompatibleDuplicateInvariantCandidate {
        point: TernaryCoordinate,
    },
    NoMatchingPendingEnd {
        node: StableInvariantNodeId,
        phases: StablePhasePair,
    },
    NoMatchingBinaryNode {
        boundary: BinaryBoundary,
        parameter: f64,
        phases: StablePhasePair,
    },
    UnresolvedPendingEnds {
        count: usize,
    },
    MalformedGraphConnectivity {
        message: String,
    },
    RegularizationUndefinedPhase {
        phase: StablePhaseId,
        point: TernaryCoordinate,
    },
    RegularizationUnstableProjection {
        phases: StablePhasePair,
        point: TernaryCoordinate,
    },
    RegularizationZeroGradient {
        phases: StablePhasePair,
        residual: f64,
    },
    RegularizationNonConvergence {
        phases: StablePhasePair,
        residual: f64,
        iterations: usize,
    },
    RegularizationBranchSwitch {
        phases: StablePhasePair,
        point: TernaryCoordinate,
    },
    StablePreparation {
        source: Box<StableContourError>,
    },
}

impl fmt::Display for StableBoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions { message } => {
                write!(formatter, "invalid stable-boundary options: {message}")
            }
            Self::InvalidBoundaryParameter {
                boundary,
                parameter,
            } => write!(
                formatter,
                "parameter {parameter:?} is invalid for boundary {boundary:?}"
            ),
            Self::PointNotOnBoundary { boundary, point } => write!(
                formatter,
                "point {:?} is not on boundary {boundary:?}",
                point.as_array()
            ),
            Self::NoPhaseDefined {
                boundary,
                parameter,
                point,
            } => write!(
                formatter,
                "no phase is defined at {:?} (boundary {boundary:?}, parameter {parameter:?})",
                point.as_array()
            ),
            Self::BinaryDiscoveryResolutionExhausted { boundary, bracket } => write!(
                formatter,
                "binary discovery resolution exhausted on {boundary:?} in {bracket:?}"
            ),
            Self::NoOverlappingStablePhaseDomains {
                boundary,
                left_phase,
                right_phase,
                bracket,
            } => write!(
                formatter,
                "stable phases {left_phase:?} and {right_phase:?} have no resolved overlap on {boundary:?} in {bracket:?}"
            ),
            Self::NonDeterministicRepeatedEvaluation { phase, point } => write!(
                formatter,
                "phase {phase:?} evaluated non-deterministically at {:?}",
                point.as_array()
            ),
            Self::InvalidRootBracket {
                boundary,
                phases,
                bracket,
            } => write!(
                formatter,
                "invalid pairwise root bracket for {phases:?} on {boundary:?}: {bracket:?}"
            ),
            Self::PairwiseRootRefinementExhausted {
                boundary,
                phases,
                bracket,
            } => write!(
                formatter,
                "pairwise root refinement exhausted for {phases:?} on {boundary:?}: {bracket:?}"
            ),
            Self::MetastableTransitionUnresolved {
                boundary,
                phases,
                bracket,
            } => write!(
                formatter,
                "metastable transition for {phases:?} on {boundary:?} was unresolved in {bracket:?}"
            ),
            Self::InvalidRegularGridTopology { message } => {
                write!(
                    formatter,
                    "invalid regular sampling-grid topology: {message}"
                )
            }
            Self::InconsistentCanonicalEdgeHit { edge } => {
                write!(
                    formatter,
                    "canonical sampling-edge hit disagrees on edge {edge}"
                )
            }
            Self::RepeatedDirectedEdgeTraversal { edge, triangle } => write!(
                formatter,
                "directed edge traversal repeated on edge {edge} from triangle {triangle}"
            ),
            Self::RepeatedTriangleTransition { triangle } => {
                write!(
                    formatter,
                    "triangle transition repeated in triangle {triangle}"
                )
            }
            Self::TraceStepLimitExceeded { start } => {
                write!(
                    formatter,
                    "trace from pending end {start:?} exceeded the step limit"
                )
            }
            Self::UnivariantLeftStablePairRegion { phases, point } => write!(
                formatter,
                "univariant {phases:?} left its stable pair region at {:?}",
                point.as_array()
            ),
            Self::InvariantSolveOutsideTriangle { triangle } => {
                write!(
                    formatter,
                    "invariant solve left sampling triangle {triangle}"
                )
            }
            Self::InvariantRefinementExhausted { triangle } => {
                write!(
                    formatter,
                    "invariant refinement exhausted in triangle {triangle}"
                )
            }
            Self::IncompatibleDuplicateInvariantCandidate { point } => write!(
                formatter,
                "incompatible invariant candidates coincide near {:?}",
                point.as_array()
            ),
            Self::NoMatchingPendingEnd { node, phases } => write!(
                formatter,
                "known node {node:?} has no compatible pending end for {phases:?}"
            ),
            Self::NoMatchingBinaryNode {
                boundary,
                parameter,
                phases,
            } => write!(
                formatter,
                "trace hit {boundary:?} at {parameter} without a matching binary node for {phases:?}"
            ),
            Self::UnresolvedPendingEnds { count } => {
                write!(
                    formatter,
                    "{count} pending stable-univariant ends remain unresolved"
                )
            }
            Self::MalformedGraphConnectivity { message } => {
                write!(formatter, "malformed stable-boundary graph: {message}")
            }
            Self::RegularizationUndefinedPhase { phase, point } => write!(
                formatter,
                "phase {phase:?} became undefined while regularizing at {:?}",
                point.as_array()
            ),
            Self::RegularizationUnstableProjection { phases, point } => write!(
                formatter,
                "regularized pair {phases:?} left the stable envelope at {:?}",
                point.as_array()
            ),
            Self::RegularizationZeroGradient { phases, residual } => write!(
                formatter,
                "regularized pair {phases:?} has zero difference gradient at residual {residual:?}"
            ),
            Self::RegularizationNonConvergence {
                phases,
                residual,
                iterations,
            } => write!(
                formatter,
                "regularized pair {phases:?} did not converge after {iterations} iterations; residual={residual:?}"
            ),
            Self::RegularizationBranchSwitch { phases, point } => write!(
                formatter,
                "regularized pair {phases:?} attempted to leave its local branch near {:?}",
                point.as_array()
            ),
            Self::StablePreparation { source } => {
                write!(formatter, "stable preparation failed: {source}")
            }
        }
    }
}

impl std::error::Error for StableBoundaryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StablePreparation { source } => Some(source),
            _ => None,
        }
    }
}

impl From<StableContourError> for StableBoundaryError {
    fn from(source: StableContourError) -> Self {
        match source {
            StableContourError::NoPhaseDefined { composition } => Self::NoPhaseDefined {
                boundary: None,
                parameter: None,
                point: TernaryCoordinate::from(composition),
            },
            source => Self::StablePreparation {
                source: Box::new(source),
            },
        }
    }
}

impl From<FieldError> for StableBoundaryError {
    fn from(source: FieldError) -> Self {
        Self::InvalidRegularGridTopology {
            message: source.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct BinaryBoundarySample {
    parameter: f64,
    point: TernaryCoordinate,
    defined_values: Vec<(StablePhaseId, f64)>,
    stable_phases: Vec<StablePhaseId>,
    stable_temperature: f64,
    stability_margin: Option<f64>,
}

struct BinaryScanner<'a> {
    boundary: BinaryBoundary,
    layers: &'a [PreparedSourceLayer<'a>],
    phase_ids: &'a [StablePhaseId],
    options: StableBoundaryOptions,
    samples: BTreeMap<u64, BinaryBoundarySample>,
    phase_evaluations: BTreeMap<(u64, StablePhaseId), StablePhaseEvaluation>,
    diagnostics: BinaryBoundaryTraceDiagnostics,
}

impl<'a> BinaryScanner<'a> {
    fn new(
        boundary: BinaryBoundary,
        layers: &'a [PreparedSourceLayer<'a>],
        phase_ids: &'a [StablePhaseId],
        options: StableBoundaryOptions,
    ) -> Self {
        Self {
            boundary,
            layers,
            phase_ids,
            options,
            samples: BTreeMap::new(),
            phase_evaluations: BTreeMap::new(),
            diagnostics: BinaryBoundaryTraceDiagnostics::default(),
        }
    }

    fn phase_layer(&self, phase: StablePhaseId) -> Option<&PreparedSourceLayer<'a>> {
        self.layers
            .iter()
            .find(|layer| layer.role == ScalarRole::Height && layer.phase == phase)
    }

    fn evaluate_phase(
        &mut self,
        phase: StablePhaseId,
        parameter: f64,
        pair_only: bool,
    ) -> Result<StablePhaseEvaluation, StableBoundaryError> {
        let key = (parameter.to_bits(), phase);
        if let Some(evaluation) = self.phase_evaluations.get(&key) {
            self.diagnostics.cached_evaluations_reused += 1;
            return Ok(evaluation.clone());
        }
        let point = self.boundary.composition_unchecked(parameter);
        let layer =
            self.phase_layer(phase)
                .ok_or(StableBoundaryError::MalformedGraphConnectivity {
                    message: format!("phase {phase:?} has no prepared height layer"),
                })?;
        let evaluation = evaluate_layer_at_point(layer, point.as_array())?;
        if pair_only {
            self.diagnostics.pair_only_evaluations += 1;
        }
        self.phase_evaluations.insert(key, evaluation.clone());
        Ok(evaluation)
    }

    fn full_sample(&mut self, parameter: f64) -> Result<BinaryBoundarySample, StableBoundaryError> {
        let parameter = parameter.clamp(0.0, 1.0);
        let key = parameter.to_bits();
        if let Some(sample) = self.samples.get(&key) {
            self.diagnostics.cached_evaluations_reused += self.phase_ids.len();
            return Ok(sample.clone());
        }
        let point = self.boundary.composition_unchecked(parameter);
        let mut defined_values = Vec::with_capacity(self.phase_ids.len());
        for &phase in self.phase_ids {
            if let StablePhaseEvaluation::Defined { value } =
                self.evaluate_phase(phase, parameter, false)?
            {
                defined_values.push((phase, value));
            }
        }
        if defined_values.is_empty() {
            return Err(StableBoundaryError::NoPhaseDefined {
                boundary: Some(self.boundary),
                parameter: Some(parameter),
                point,
            });
        }
        defined_values.sort_by_key(|(phase, _)| *phase);
        let stable_temperature = defined_values
            .iter()
            .map(|(_, value)| *value)
            .fold(f64::NEG_INFINITY, f64::max);
        let stable_phases = defined_values
            .iter()
            .filter(|(_, value)| *value >= stable_temperature - self.options.stability_tolerance)
            .map(|(phase, _)| *phase)
            .collect::<Vec<_>>();
        let mut ranked = defined_values
            .iter()
            .map(|(_, value)| *value)
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| right.total_cmp(left));
        let stability_margin = ranked.get(1).map(|second| stable_temperature - *second);
        let sample = BinaryBoundarySample {
            parameter,
            point,
            defined_values,
            stable_phases,
            stable_temperature,
            stability_margin,
        };
        self.diagnostics.samples_evaluated += 1;
        self.diagnostics.full_phase_sweeps += 1;
        self.samples.insert(key, sample.clone());
        Ok(sample)
    }

    fn pair_delta(
        &mut self,
        parameter: f64,
        phases: StablePhasePair,
    ) -> Result<Option<(f64, f64, f64)>, StableBoundaryError> {
        let left = self.evaluate_phase(phases.first, parameter, true)?;
        let right = self.evaluate_phase(phases.second, parameter, true)?;
        match (left, right) {
            (
                StablePhaseEvaluation::Defined { value: first },
                StablePhaseEvaluation::Defined { value: second },
            ) => Ok(Some((first - second, first, second))),
            _ => Ok(None),
        }
    }
}
impl<'a> BinaryScanner<'a> {
    fn scan(mut self, first_node_id: usize) -> Result<BinaryBoundaryTrace, StableBoundaryError> {
        let final_intervals = self
            .options
            .binary_initial_subdivisions
            .checked_shl(self.options.binary_maximum_depth.into())
            .ok_or_else(|| StableBoundaryError::InvalidOptions {
                message: "binary interval count overflowed".into(),
            })?;
        let mut initial_phases = BTreeSet::new();
        for index in 0..=self.options.binary_initial_subdivisions {
            let parameter = index as f64 / self.options.binary_initial_subdivisions as f64;
            initial_phases.extend(self.full_sample(parameter)?.stable_phases);
        }
        let mut samples = Vec::with_capacity(final_intervals.saturating_add(1));
        for index in 0..=final_intervals {
            let parameter = index as f64 / final_intervals as f64;
            samples.push(self.full_sample(parameter)?);
        }
        self.diagnostics.intervals_refined =
            final_intervals.saturating_sub(self.options.binary_initial_subdivisions);
        let final_phases = samples
            .iter()
            .flat_map(|sample| sample.stable_phases.iter().copied())
            .collect::<BTreeSet<_>>();
        self.diagnostics.intermediate_phases_inserted =
            final_phases.difference(&initial_phases).count();

        let regions = stable_regions(&samples);
        let mut invariants = Vec::new();

        for (index, sample) in samples.iter().enumerate() {
            if sample.stable_phases.len() < 2 {
                continue;
            }
            let left = samples[..index]
                .iter()
                .rev()
                .find_map(primary_stable_phase)
                .unwrap_or(sample.stable_phases[0]);
            let right = samples[index + 1..]
                .iter()
                .find_map(primary_stable_phase)
                .unwrap_or(*sample.stable_phases.last().unwrap_or(&left));
            invariants.push(node_from_sample(
                self.boundary,
                sample,
                left,
                right,
                StableInvariantNodeId(0),
            ));
        }

        for pair in samples.windows(2) {
            let Some(left_phase) = primary_stable_phase(&pair[0]) else {
                continue;
            };
            let Some(right_phase) = primary_stable_phase(&pair[1]) else {
                continue;
            };
            if left_phase == right_phase {
                continue;
            }
            if pair[0].stable_phases.len() > 1 || pair[1].stable_phases.len() > 1 {
                continue;
            }
            if let Some(node) =
                self.refine_transition(&pair[0], &pair[1], left_phase, right_phase)?
            {
                invariants.push(node);
            }
        }

        invariants.sort_by(|left, right| {
            left.boundary_parameter
                .total_cmp(&right.boundary_parameter)
                .then_with(|| left.phases.cmp(&right.phases))
        });
        invariants = deduplicate_binary_nodes(
            invariants,
            self.options.binary_parameter_tolerance,
            self.options.temperature_tolerance,
        );
        for (offset, node) in invariants.iter_mut().enumerate() {
            node.id = StableInvariantNodeId(first_node_id + offset);
        }
        self.diagnostics.invariants_emitted = invariants.len();
        self.diagnostics.higher_order_invariants = invariants
            .iter()
            .filter(|node| node.phases.len() >= 3)
            .count();
        Ok(BinaryBoundaryTrace {
            boundary: self.boundary,
            regions,
            invariants,
            diagnostics: self.diagnostics,
        })
    }

    fn refine_transition(
        &mut self,
        left: &BinaryBoundarySample,
        right: &BinaryBoundarySample,
        left_phase: StablePhaseId,
        right_phase: StablePhaseId,
    ) -> Result<Option<BinaryInvariantNode>, StableBoundaryError> {
        let phases = StablePhasePair::new(left_phase, right_phase);
        let mut candidates = Vec::new();
        let extra_depth = self.options.binary_maximum_depth.saturating_add(4).min(12);
        let subdivisions = 1usize << extra_depth;
        for index in 0..=subdivisions {
            let fraction = index as f64 / subdivisions as f64;
            let parameter = left.parameter + fraction * (right.parameter - left.parameter);
            if let Some((delta, first, second)) = self.pair_delta(parameter, phases)? {
                candidates.push((parameter, delta, first, second));
            }
        }
        candidates.sort_by(|left, right| left.0.total_cmp(&right.0));
        let bracket = candidates.windows(2).find_map(|pair| {
            let opposite_sign = pair[0].1 == 0.0
                || pair[1].1 == 0.0
                || pair[0].1.is_sign_positive() != pair[1].1.is_sign_positive();
            opposite_sign.then_some((pair[0], pair[1]))
        });
        let Some((left_pair, right_pair)) = bracket else {
            return Err(StableBoundaryError::NoOverlappingStablePhaseDomains {
                boundary: self.boundary,
                left_phase,
                right_phase,
                bracket: [left.parameter, right.parameter],
            });
        };
        let (parameter, first, second) =
            self.refine_pairwise_root(phases, left_pair, right_pair)?;
        let stable_sample = self.full_sample(parameter)?;
        if !stable_sample.stable_phases.contains(&phases.first)
            || !stable_sample.stable_phases.contains(&phases.second)
        {
            self.diagnostics.metastable_pairwise_roots_rejected += 1;
            return Err(StableBoundaryError::MetastableTransitionUnresolved {
                boundary: self.boundary,
                phases,
                bracket: [left.parameter, right.parameter],
            });
        }
        Ok(Some(BinaryInvariantNode {
            id: StableInvariantNodeId(0),
            boundary: self.boundary,
            boundary_parameter: parameter,
            point: stable_sample.point,
            temperature: 0.5 * (first + second),
            phases: stable_sample.stable_phases,
            left_stable_phase: left_phase,
            right_stable_phase: right_phase,
        }))
    }

    fn refine_pairwise_root(
        &mut self,
        phases: StablePhasePair,
        mut left: (f64, f64, f64, f64),
        mut right: (f64, f64, f64, f64),
    ) -> Result<(f64, f64, f64), StableBoundaryError> {
        if left.1 != 0.0
            && right.1 != 0.0
            && left.1.is_sign_positive() == right.1.is_sign_positive()
        {
            return Err(StableBoundaryError::InvalidRootBracket {
                boundary: self.boundary,
                phases,
                bracket: [left.0, right.0],
            });
        }
        for _ in 0..128 {
            let best = if left.1.abs() <= right.1.abs() {
                left
            } else {
                right
            };
            if (right.0 - left.0).abs() <= self.options.binary_parameter_tolerance
                && best.1.abs() <= self.options.temperature_tolerance
            {
                return Ok((best.0, best.2, best.3));
            }
            let denominator = right.1 - left.1;
            let secant = left.0 - left.1 * (right.0 - left.0) / denominator;
            let guard = 0.05 * (right.0 - left.0);
            let mut proposed = if denominator.is_finite()
                && denominator != 0.0
                && secant.is_finite()
                && secant > left.0 + guard
                && secant < right.0 - guard
            {
                secant
            } else {
                0.5 * (left.0 + right.0)
            };
            let mut evaluated = self.pair_delta(proposed, phases)?;
            if evaluated.is_none() {
                proposed = 0.5 * (left.0 + right.0);
                evaluated = self.pair_delta(proposed, phases)?;
            }
            let Some((delta, first, second)) = evaluated else {
                return Err(StableBoundaryError::NoOverlappingStablePhaseDomains {
                    boundary: self.boundary,
                    left_phase: phases.first,
                    right_phase: phases.second,
                    bracket: [left.0, right.0],
                });
            };
            let current = (proposed, delta, first, second);
            if delta.abs() <= self.options.temperature_tolerance {
                return Ok((proposed, first, second));
            }
            if left.1 == 0.0 {
                return Ok((left.0, left.2, left.3));
            }
            if right.1 == 0.0 {
                return Ok((right.0, right.2, right.3));
            }
            if left.1.is_sign_positive() != delta.is_sign_positive() {
                right = current;
            } else {
                left = current;
            }
        }
        Err(StableBoundaryError::PairwiseRootRefinementExhausted {
            boundary: self.boundary,
            phases,
            bracket: [left.0, right.0],
        })
    }
}

fn primary_stable_phase(sample: &BinaryBoundarySample) -> Option<StablePhaseId> {
    sample.stable_phases.first().copied()
}

fn stable_regions(samples: &[BinaryBoundarySample]) -> Vec<BinaryStableRegion> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut regions = Vec::new();
    let mut start = 0.0;
    let mut phases = samples[0].stable_phases.clone();
    for index in 1..samples.len() {
        if samples[index].stable_phases != phases {
            let boundary = 0.5 * (samples[index - 1].parameter + samples[index].parameter);
            regions.push(BinaryStableRegion {
                stable_phases: phases,
                parameter_range: [start, boundary],
            });
            start = boundary;
            phases = samples[index].stable_phases.clone();
        }
    }
    regions.push(BinaryStableRegion {
        stable_phases: phases,
        parameter_range: [start, 1.0],
    });
    regions
}

fn node_from_sample(
    boundary: BinaryBoundary,
    sample: &BinaryBoundarySample,
    left_stable_phase: StablePhaseId,
    right_stable_phase: StablePhaseId,
    id: StableInvariantNodeId,
) -> BinaryInvariantNode {
    BinaryInvariantNode {
        id,
        boundary,
        boundary_parameter: sample.parameter,
        point: sample.point,
        temperature: sample.stable_temperature,
        phases: sample.stable_phases.clone(),
        left_stable_phase,
        right_stable_phase,
    }
}

fn deduplicate_binary_nodes(
    nodes: Vec<BinaryInvariantNode>,
    parameter_tolerance: f64,
    temperature_tolerance: f64,
) -> Vec<BinaryInvariantNode> {
    let mut unique: Vec<BinaryInvariantNode> = Vec::new();
    for node in nodes {
        if let Some(previous) = unique.last_mut()
            && (node.boundary_parameter - previous.boundary_parameter).abs() <= parameter_tolerance
            && (node.temperature - previous.temperature).abs() <= temperature_tolerance
        {
            let phases = previous
                .phases
                .iter()
                .chain(&node.phases)
                .copied()
                .collect::<BTreeSet<_>>();
            previous.phases = phases.into_iter().collect();
            if node
                .boundary_parameter
                .total_cmp(&previous.boundary_parameter)
                .is_lt()
            {
                previous.boundary_parameter = node.boundary_parameter;
                previous.point = node.point;
                previous.temperature = node.temperature;
            }
        } else {
            unique.push(node);
        }
    }
    unique
}

pub(crate) fn trace_binary_boundaries<'a>(
    layers: &'a [PreparedSourceLayer<'a>],
    phase_ids: &'a [StablePhaseId],
    options: StableBoundaryOptions,
) -> Result<Vec<BinaryBoundaryTrace>, StableBoundaryError> {
    options.validate()?;
    let mut traces = Vec::with_capacity(3);
    let mut next_node_id = 0usize;
    for boundary in BinaryBoundary::ALL {
        let trace = BinaryScanner::new(boundary, layers, phase_ids, options).scan(next_node_id)?;
        next_node_id = next_node_id
            .checked_add(trace.invariants.len())
            .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
                message: "binary invariant node count overflowed".into(),
            })?;
        traces.push(trace);
    }
    Ok(traces)
}

pub(crate) fn aggregate_binary_diagnostics(
    traces: &[BinaryBoundaryTrace],
    diagnostics: &mut StableBoundaryDiagnostics,
) {
    diagnostics.binary_boundaries_scanned = traces.len();
    for trace in traces {
        diagnostics.binary_samples_evaluated += trace.diagnostics.samples_evaluated;
        diagnostics.binary_full_phase_sweeps += trace.diagnostics.full_phase_sweeps;
        diagnostics.binary_pair_only_evaluations += trace.diagnostics.pair_only_evaluations;
        diagnostics.binary_intervals_refined += trace.diagnostics.intervals_refined;
        diagnostics.binary_intermediate_phases_inserted +=
            trace.diagnostics.intermediate_phases_inserted;
        diagnostics.binary_metastable_pairwise_roots_rejected +=
            trace.diagnostics.metastable_pairwise_roots_rejected;
        diagnostics.binary_invariants_emitted += trace.diagnostics.invariants_emitted;
        diagnostics.binary_higher_order_invariants += trace.diagnostics.higher_order_invariants;
    }
}

pub(crate) fn topology_diagnostics(
    topology: &RegularSamplingTopology,
    diagnostics: &mut StableBoundaryDiagnostics,
) -> Result<(), StableBoundaryError> {
    diagnostics.sampling_vertices = topology.grid().vertex_count();
    diagnostics.sampling_edges = topology.edge_count();
    diagnostics.sampling_triangles = topology.grid().triangle_count()?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PointKey([i64; 3]);

#[derive(Clone, Debug, PartialEq)]
enum TraceFeature {
    Edge(crate::RegularGridEdgeId),
    Vertex(crate::GridVertexId),
    StableBoundary(BinaryBoundary, f64),
    Invariant,
    Interior,
}

#[derive(Clone, Debug)]
struct FragmentEndpoint {
    key: PointKey,
    point: TernaryCoordinate,
    temperature: f64,
    tied_phases: Vec<StablePhaseId>,
    feature: TraceFeature,
    node: Option<StableInvariantNodeId>,
}

#[derive(Clone, Debug)]
struct LocalBoundaryFragment {
    pair: StablePhasePair,
    triangle: usize,
    endpoints: [FragmentEndpoint; 2],
}

#[derive(Clone, Copy, Debug)]
struct RegularGridEdgeHit {
    parameter: f64,
    point: TernaryCoordinate,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PendingEndKey {
    phases: StablePhasePair,
    node: StableInvariantNodeId,
    branch: usize,
}

#[derive(Clone, Debug)]
struct PendingStableUnivariantEnd {
    id: StableUnivariantEndId,
    key: PendingEndKey,
    fragment: usize,
    side: usize,
    consumed: bool,
}

struct RegularGridTraceMarks {
    epoch: u32,
    edges: Vec<[u32; 2]>,
    triangles: Vec<u32>,
    vertices: Vec<u32>,
}

impl RegularGridTraceMarks {
    fn new(topology: &RegularSamplingTopology) -> Result<Self, StableBoundaryError> {
        Ok(Self {
            epoch: 0,
            edges: vec![[0; 2]; topology.edge_count()],
            triangles: vec![0; topology.grid().triangle_count()?],
            vertices: vec![0; topology.grid().vertex_count()],
        })
    }

    fn begin(&mut self) {
        if self.epoch == u32::MAX {
            self.edges.fill([0; 2]);
            self.triangles.fill(0);
            self.vertices.fill(0);
            self.epoch = 1;
        } else {
            self.epoch += 1;
            if self.epoch == 0 {
                self.epoch = 1;
            }
        }
    }

    fn mark_triangle(&mut self, triangle: usize) -> Result<(), StableBoundaryError> {
        let Some(mark) = self.triangles.get_mut(triangle) else {
            return Err(StableBoundaryError::InvalidRegularGridTopology {
                message: format!("triangle {triangle} is outside dense trace marks"),
            });
        };
        if *mark == self.epoch {
            return Err(StableBoundaryError::RepeatedTriangleTransition { triangle });
        }
        *mark = self.epoch;
        Ok(())
    }

    fn mark_feature(
        &mut self,
        feature: &TraceFeature,
        triangle: usize,
        topology: &RegularSamplingTopology,
        diagnostics: &mut StableBoundaryDiagnostics,
    ) -> Result<(), StableBoundaryError> {
        match feature {
            TraceFeature::Edge(edge) => {
                let incidents = topology.incident_triangles(*edge)?;
                let slot = if incidents[0] == Some(triangle) {
                    0
                } else if incidents[1] == Some(triangle) {
                    1
                } else {
                    return Err(StableBoundaryError::InvalidRegularGridTopology {
                        message: format!("triangle {triangle} is not incident to edge {}", edge.0),
                    });
                };
                if self.edges[edge.0][slot] == self.epoch {
                    diagnostics.directed_traversal_rejections += 1;
                    return Err(StableBoundaryError::RepeatedDirectedEdgeTraversal {
                        edge: edge.0,
                        triangle,
                    });
                }
                self.edges[edge.0][slot] = self.epoch;
                diagnostics.sampling_edge_crossings += 1;
            }
            TraceFeature::Vertex(vertex) => {
                let Some(mark) = self.vertices.get_mut(vertex.0) else {
                    return Err(StableBoundaryError::InvalidRegularGridTopology {
                        message: format!("vertex {} is outside dense trace marks", vertex.0),
                    });
                };
                if *mark == self.epoch {
                    diagnostics.directed_traversal_rejections += 1;
                    return Err(StableBoundaryError::RepeatedTriangleTransition { triangle });
                }
                *mark = self.epoch;
                diagnostics.sampling_vertex_crossings += 1;
            }
            TraceFeature::StableBoundary(_, _)
            | TraceFeature::Invariant
            | TraceFeature::Interior => {}
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_boundary_fragments(
    cells: &[super::partition::StableSamplingCell],
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
    topology: &RegularSamplingTopology,
    options: StableBoundaryOptions,
    diagnostics: &mut StableBoundaryDiagnostics,
) -> Result<Vec<LocalBoundaryFragment>, StableBoundaryError> {
    let mut fragments = Vec::new();
    let mut seen = BTreeSet::new();
    let mut edge_hits =
        BTreeMap::<(StablePhasePair, crate::RegularGridEdgeId), RegularGridEdgeHit>::new();

    for cell in cells {
        for polygon in &cell.polygons {
            for (&left, &right) in polygon
                .barycentric
                .iter()
                .zip(polygon.barycentric.iter().cycle().skip(1))
                .take(polygon.barycentric.len())
            {
                let midpoint = [
                    0.5 * (left[0] + right[0]),
                    0.5 * (left[1] + right[1]),
                    0.5 * (left[2] + right[2]),
                ];
                let (tied, _) = affine_stable_phases(
                    samples,
                    phase_ids,
                    cell,
                    midpoint,
                    options.stability_tolerance,
                );
                if tied.len() < 2 || !tied.contains(&polygon.phase) {
                    continue;
                }
                for other in tied.iter().copied().filter(|phase| *phase != polygon.phase) {
                    let pair = StablePhasePair::new(polygon.phase, other);
                    let mut endpoints = [
                        canonical_fragment_endpoint(
                            pair,
                            cell,
                            left,
                            samples,
                            phase_ids,
                            topology,
                            options,
                            &mut edge_hits,
                            diagnostics,
                        )?,
                        canonical_fragment_endpoint(
                            pair,
                            cell,
                            right,
                            samples,
                            phase_ids,
                            topology,
                            options,
                            &mut edge_hits,
                            diagnostics,
                        )?,
                    ];
                    if endpoints[0].key == endpoints[1].key {
                        continue;
                    }
                    if endpoints[1].key < endpoints[0].key {
                        endpoints.swap(0, 1);
                    }
                    let key = (cell.triangle.id, pair, endpoints[0].key, endpoints[1].key);
                    if seen.insert(key) {
                        fragments.push(LocalBoundaryFragment {
                            pair,
                            triangle: cell.triangle.id,
                            endpoints,
                        });
                    }
                }
            }
        }
    }
    fragments.sort_by(|left, right| {
        left.pair
            .cmp(&right.pair)
            .then_with(|| left.endpoints[0].key.cmp(&right.endpoints[0].key))
            .then_with(|| left.endpoints[1].key.cmp(&right.endpoints[1].key))
            .then_with(|| left.triangle.cmp(&right.triangle))
    });
    Ok(fragments)
}

fn affine_stable_phases(
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
    cell: &super::partition::StableSamplingCell,
    barycentric: [f64; 3],
    tolerance: f64,
) -> (Vec<StablePhaseId>, f64) {
    let values = phase_ids
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(phase, id)| {
            samples
                .triangle_height_values(phase, cell.triangle.vertices)
                .map(|values| (id, super::sample::dot(values, barycentric)))
        })
        .collect::<Vec<_>>();
    let maximum = values
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::NEG_INFINITY, f64::max);
    let tied = values
        .into_iter()
        .filter(|(_, value)| *value >= maximum - tolerance)
        .map(|(phase, _)| phase)
        .collect();
    (tied, maximum)
}

#[allow(clippy::too_many_arguments)]
fn canonical_fragment_endpoint(
    pair: StablePhasePair,
    cell: &super::partition::StableSamplingCell,
    barycentric: [f64; 3],
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
    topology: &RegularSamplingTopology,
    options: StableBoundaryOptions,
    edge_hits: &mut BTreeMap<(StablePhasePair, crate::RegularGridEdgeId), RegularGridEdgeHit>,
    diagnostics: &mut StableBoundaryDiagnostics,
) -> Result<FragmentEndpoint, StableBoundaryError> {
    let mut point = super::partition::point_from_barycentric(cell, barycentric);
    let (tied_phases, stable_temperature) = affine_stable_phases(
        samples,
        phase_ids,
        cell,
        barycentric,
        options.stability_tolerance,
    );
    let components = point.as_array();
    let outer = if components[2].abs() <= options.geometry_tolerance {
        Some(BinaryBoundary::Ab)
    } else if components[0].abs() <= options.geometry_tolerance {
        Some(BinaryBoundary::Bc)
    } else if components[1].abs() <= options.geometry_tolerance {
        Some(BinaryBoundary::Ca)
    } else {
        None
    };
    let zeroes = barycentric
        .iter()
        .enumerate()
        .filter(|(_, value)| value.abs() <= options.geometry_tolerance)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    let feature = if let Some(boundary) = outer {
        let parameter = boundary.parameter(point)?;
        point = boundary.composition_unchecked(parameter);
        TraceFeature::StableBoundary(boundary, parameter)
    } else if tied_phases.len() >= 3 {
        TraceFeature::Invariant
    } else if zeroes.len() >= 2 {
        let local = barycentric
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .ok_or_else(|| StableBoundaryError::InvalidRegularGridTopology {
                message: "triangle endpoint has no owning vertex".into(),
            })?;
        let vertex = cell.triangle.vertices[local];
        point = TernaryCoordinate::from(samples.grid.composition(vertex)?);
        TraceFeature::Vertex(vertex)
    } else if zeroes.len() == 1 {
        let local_edge = match zeroes[0] {
            0 => 1,
            1 => 2,
            _ => 0,
        };
        let edge = topology.triangle_edges(cell.triangle.id)?[local_edge].edge;
        let vertices = topology.edge_vertices(edge)?;
        let start = samples.grid.composition(vertices[0])?;
        let end = samples.grid.composition(vertices[1])?;
        let raw = point.as_array();
        let direction = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let denominator = direction.iter().map(|value| value * value).sum::<f64>();
        if denominator <= 0.0 || !denominator.is_finite() {
            return Err(StableBoundaryError::InvalidRegularGridTopology {
                message: format!("edge {} has zero geometric length", edge.0),
            });
        }
        let parameter = raw
            .iter()
            .zip(start)
            .zip(direction)
            .map(|((value, start), direction)| (value - start) * direction)
            .sum::<f64>()
            / denominator;
        let parameter = parameter.clamp(0.0, 1.0);
        let canonical = TernaryCoordinate::from([
            start[0] + parameter * direction[0],
            start[1] + parameter * direction[1],
            start[2] + parameter * direction[2],
        ]);
        if let Some(previous) = edge_hits.get(&(pair, edge)) {
            if (previous.parameter - parameter).abs() > options.geometry_tolerance {
                return Err(StableBoundaryError::InconsistentCanonicalEdgeHit { edge: edge.0 });
            }
            point = previous.point;
            diagnostics.reused_canonical_edge_hits += 1;
        } else {
            edge_hits.insert(
                (pair, edge),
                RegularGridEdgeHit {
                    parameter,
                    point: canonical,
                },
            );
            point = canonical;
        }
        TraceFeature::Edge(edge)
    } else {
        TraceFeature::Interior
    };

    let temperature =
        pair_temperature(pair, cell, barycentric, samples, phase_ids).unwrap_or(stable_temperature);
    Ok(FragmentEndpoint {
        key: point_key(point, options.geometry_tolerance),
        point,
        temperature,
        tied_phases,
        feature,
        node: None,
    })
}

fn pair_temperature(
    pair: StablePhasePair,
    cell: &super::partition::StableSamplingCell,
    barycentric: [f64; 3],
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
) -> Option<f64> {
    let first = phase_ids.binary_search(&pair.first).ok()?;
    let second = phase_ids.binary_search(&pair.second).ok()?;
    let first_values = samples.triangle_height_values(first, cell.triangle.vertices)?;
    let second_values = samples.triangle_height_values(second, cell.triangle.vertices)?;
    Some(
        0.5 * (super::sample::dot(first_values, barycentric)
            + super::sample::dot(second_values, barycentric)),
    )
}

fn point_key(point: TernaryCoordinate, tolerance: f64) -> PointKey {
    PointKey(point.as_array().map(|value| {
        let scaled = (value / tolerance).round();
        if scaled <= i64::MIN as f64 {
            i64::MIN
        } else if scaled >= i64::MAX as f64 {
            i64::MAX
        } else {
            scaled as i64
        }
    }))
}

fn match_binary_endpoints(
    fragments: &mut [LocalBoundaryFragment],
    traces: &[BinaryBoundaryTrace],
    nodes: &[StableInvariantNode],
    sampling_subdivisions: usize,
    options: StableBoundaryOptions,
) -> Result<(), StableBoundaryError> {
    for fragment in fragments {
        for endpoint in &mut fragment.endpoints {
            let TraceFeature::StableBoundary(boundary, parameter) = endpoint.feature else {
                continue;
            };
            let candidate = traces
                .iter()
                .filter(|trace| trace.boundary == boundary)
                .flat_map(|trace| trace.invariants.iter())
                .filter(|node| {
                    node.phases.contains(&fragment.pair.first)
                        && node.phases.contains(&fragment.pair.second)
                })
                .min_by(|left, right| {
                    (left.boundary_parameter - parameter)
                        .abs()
                        .total_cmp(&(right.boundary_parameter - parameter).abs())
                        .then_with(|| left.id.cmp(&right.id))
                });
            let Some(candidate) = candidate else {
                return Err(StableBoundaryError::NoMatchingBinaryNode {
                    boundary,
                    parameter,
                    phases: fragment.pair,
                });
            };
            let sampling_resolution = 1.0 / sampling_subdivisions as f64;
            let tolerance = options
                .binary_parameter_tolerance
                .max(options.geometry_tolerance * 4.0)
                .max(sampling_resolution);
            if (candidate.boundary_parameter - parameter).abs() > tolerance {
                return Err(StableBoundaryError::NoMatchingBinaryNode {
                    boundary,
                    parameter,
                    phases: fragment.pair,
                });
            }
            let canonical = nodes.get(candidate.id.0).ok_or_else(|| {
                StableBoundaryError::MalformedGraphConnectivity {
                    message: "binary node ID does not index the canonical node vector".into(),
                }
            })?;
            endpoint.node = Some(candidate.id);
            endpoint.point = canonical.point();
            endpoint.temperature = canonical.temperature();
            endpoint.key = point_key(canonical.point(), options.geometry_tolerance);
        }
    }
    Ok(())
}

fn canonical_nodes(
    traces: &[BinaryBoundaryTrace],
) -> Result<Vec<StableInvariantNode>, StableBoundaryError> {
    let mut nodes = Vec::new();
    for trace in traces {
        for node in &trace.invariants {
            if node.id.0 != nodes.len() {
                return Err(StableBoundaryError::MalformedGraphConnectivity {
                    message: "binary invariant identifiers are not dense".into(),
                });
            }
            nodes.push(StableInvariantNode::Binary(node.clone()));
        }
    }
    Ok(nodes)
}

fn canonicalize_interior_nodes(
    fragments: &mut [LocalBoundaryFragment],
    nodes: &mut Vec<StableInvariantNode>,
    options: StableBoundaryOptions,
    diagnostics: &mut StableBoundaryDiagnostics,
) -> Result<(), StableBoundaryError> {
    for fragment in fragments {
        for endpoint in &mut fragment.endpoints {
            if !matches!(endpoint.feature, TraceFeature::Invariant)
                && endpoint.tied_phases.len() < 3
            {
                continue;
            }
            diagnostics.interior_invariant_candidates += 1;
            endpoint.tied_phases.sort();
            endpoint.tied_phases.dedup();
            let matching = nodes.iter().position(|node| {
                matches!(node, StableInvariantNode::Interior(_))
                    && composition_distance(node.point(), endpoint.point)
                        <= options.geometry_tolerance * 4.0
                    && (node.temperature() - endpoint.temperature).abs()
                        <= options.temperature_tolerance * 4.0
            });
            let id = if let Some(index) = matching {
                let node = nodes.get_mut(index).ok_or_else(|| {
                    StableBoundaryError::MalformedGraphConnectivity {
                        message: "interior invariant index disappeared".into(),
                    }
                })?;
                let StableInvariantNode::Interior(node) = node else {
                    return Err(
                        StableBoundaryError::IncompatibleDuplicateInvariantCandidate {
                            point: endpoint.point,
                        },
                    );
                };
                let phases = node
                    .phases
                    .iter()
                    .chain(&endpoint.tied_phases)
                    .copied()
                    .collect::<BTreeSet<_>>();
                node.phases = phases.into_iter().collect();
                diagnostics.known_invariants_revisited += 1;
                node.id
            } else {
                let id = StableInvariantNodeId(nodes.len());
                nodes.push(StableInvariantNode::Interior(InteriorInvariantNode {
                    id,
                    point: endpoint.point,
                    temperature: endpoint.temperature,
                    phases: endpoint.tied_phases.clone(),
                }));
                diagnostics.interior_invariants_accepted += 1;
                id
            };
            let canonical =
                nodes
                    .get(id.0)
                    .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
                        message: "interior invariant ID does not index the node vector".into(),
                    })?;
            endpoint.node = Some(id);
            endpoint.point = canonical.point();
            endpoint.temperature = canonical.temperature();
            endpoint.key = point_key(canonical.point(), options.geometry_tolerance);
        }
    }
    Ok(())
}

fn composition_distance(left: TernaryCoordinate, right: TernaryCoordinate) -> f64 {
    crate::simplex::logical_distance(left.as_array(), right.as_array())
}

fn build_fragment_adjacency(
    fragments: &[LocalBoundaryFragment],
) -> BTreeMap<(StablePhasePair, PointKey), Vec<(usize, usize)>> {
    let mut adjacency = BTreeMap::<(StablePhasePair, PointKey), Vec<(usize, usize)>>::new();
    for (fragment_index, fragment) in fragments.iter().enumerate() {
        for (side, endpoint) in fragment.endpoints.iter().enumerate() {
            adjacency
                .entry((fragment.pair, endpoint.key))
                .or_default()
                .push((fragment_index, side));
        }
    }
    for entries in adjacency.values_mut() {
        entries.sort_unstable();
    }
    adjacency
}

fn create_pending_ends(
    fragments: &[LocalBoundaryFragment],
    nodes: &[StableInvariantNode],
    diagnostics: &mut StableBoundaryDiagnostics,
) -> Result<(Vec<PendingStableUnivariantEnd>, Vec<Option<usize>>), StableBoundaryError> {
    let mut entries = Vec::<(PendingEndKey, usize, usize)>::new();
    for (fragment_index, fragment) in fragments.iter().enumerate() {
        for (side, endpoint) in fragment.endpoints.iter().enumerate() {
            let Some(node) = endpoint.node else {
                continue;
            };
            let canonical = nodes.get(node.0).ok_or_else(|| {
                StableBoundaryError::MalformedGraphConnectivity {
                    message: "fragment endpoint references an absent node".into(),
                }
            })?;
            if !canonical.phases().contains(&fragment.pair.first)
                || !canonical.phases().contains(&fragment.pair.second)
            {
                return Err(StableBoundaryError::MalformedGraphConnectivity {
                    message: format!(
                        "node {} does not contain both phases of {:?}",
                        node.0, fragment.pair
                    ),
                });
            }
            entries.push((
                PendingEndKey {
                    phases: fragment.pair,
                    node,
                    branch: fragment_index * 2 + side,
                },
                fragment_index,
                side,
            ));
            match canonical {
                StableInvariantNode::Binary(_) => diagnostics.pending_ends_initially_created += 1,
                StableInvariantNode::Interior(_) => {
                    diagnostics.pending_ends_created_at_interior_invariants += 1;
                }
            }
        }
    }
    entries.sort_by_key(|entry| entry.0);
    let mut pending_lookup = vec![None; fragments.len().saturating_mul(2)];
    let mut pending = Vec::with_capacity(entries.len());
    for (index, (key, fragment, side)) in entries.into_iter().enumerate() {
        pending_lookup[fragment * 2 + side] = Some(index);
        pending.push(PendingStableUnivariantEnd {
            id: StableUnivariantEndId(index),
            key,
            fragment,
            side,
            consumed: false,
        });
    }
    Ok((pending, pending_lookup))
}

fn append_path_point(
    points: &mut Vec<TernaryCoordinate>,
    temperatures: &mut Vec<f64>,
    endpoint: &FragmentEndpoint,
    tolerance: f64,
) {
    if points
        .last()
        .is_some_and(|previous| composition_distance(*previous, endpoint.point) <= tolerance)
    {
        if let Some(last) = points.last_mut() {
            *last = endpoint.point;
        }
        if let Some(last) = temperatures.last_mut() {
            *last = endpoint.temperature;
        }
        return;
    }
    points.push(endpoint.point);
    temperatures.push(endpoint.temperature);
}

fn oriented_direction(fragment: &LocalBoundaryFragment, entry_side: usize) -> [f64; 2] {
    let start =
        crate::simplex::logical_from_composition(fragment.endpoints[entry_side].point.as_array());
    let end = crate::simplex::logical_from_composition(
        fragment.endpoints[1 - entry_side].point.as_array(),
    );
    [end[0] - start[0], end[1] - start[1]]
}

fn choose_forward_fragment(
    candidates: &[(usize, usize)],
    fragments: &[LocalBoundaryFragment],
    used: &[bool],
    current_fragment: usize,
    incoming: [f64; 2],
) -> Result<Option<(usize, usize)>, StableBoundaryError> {
    let incoming_norm = incoming[0].hypot(incoming[1]);
    let mut viable = Vec::<(f64, usize, usize)>::new();
    for &(fragment_index, entry_side) in candidates {
        if fragment_index == current_fragment || used[fragment_index] {
            continue;
        }
        let direction = oriented_direction(&fragments[fragment_index], entry_side);
        let norm = direction[0].hypot(direction[1]);
        if !norm.is_finite() || norm <= f64::EPSILON || incoming_norm <= f64::EPSILON {
            continue;
        }
        let cosine =
            (incoming[0] * direction[0] + incoming[1] * direction[1]) / (incoming_norm * norm);
        viable.push((cosine, fragment_index, entry_side));
    }
    viable.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    if viable.len() >= 2 && (viable[0].0 - viable[1].0).abs() <= 1.0e-12 {
        return Err(StableBoundaryError::MalformedGraphConnectivity {
            message: "ambiguous forward continuation at a sampling-grid feature".into(),
        });
    }
    Ok(viable.first().map(|entry| (entry.1, entry.2)))
}

fn consume_pending(
    pending: &mut [PendingStableUnivariantEnd],
    pending_lookup: &[Option<usize>],
    fragment: usize,
    side: usize,
    diagnostics: &mut StableBoundaryDiagnostics,
) -> Result<(), StableBoundaryError> {
    let Some(index) = pending_lookup.get(fragment * 2 + side).copied().flatten() else {
        return Err(StableBoundaryError::MalformedGraphConnectivity {
            message: "node endpoint has no pending-end record".into(),
        });
    };
    let end =
        pending
            .get_mut(index)
            .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
                message: "pending-end lookup points outside its dense array".into(),
            })?;
    if end.consumed {
        return Err(StableBoundaryError::NoMatchingPendingEnd {
            node: end.key.node,
            phases: end.key.phases,
        });
    }
    end.consumed = true;
    diagnostics.pending_ends_consumed += 1;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn trace_one_univariant(
    start_pending: usize,
    pending: &mut [PendingStableUnivariantEnd],
    pending_lookup: &[Option<usize>],
    fragments: &[LocalBoundaryFragment],
    adjacency: &BTreeMap<(StablePhasePair, PointKey), Vec<(usize, usize)>>,
    used: &mut [bool],
    nodes: &[StableInvariantNode],
    topology: &RegularSamplingTopology,
    marks: &mut RegularGridTraceMarks,
    options: StableBoundaryOptions,
    diagnostics: &mut StableBoundaryDiagnostics,
) -> Result<StableUnivariantPath, StableBoundaryError> {
    let start_record = pending.get(start_pending).ok_or_else(|| {
        StableBoundaryError::MalformedGraphConnectivity {
            message: "starting pending end is outside its dense array".into(),
        }
    })?;
    let start_id = start_record.id;
    let phases = start_record.key.phases;
    let start_node = start_record.key.node;
    let mut fragment_index = start_record.fragment;
    let mut entry_side = start_record.side;
    consume_pending(
        pending,
        pending_lookup,
        fragment_index,
        entry_side,
        diagnostics,
    )?;
    diagnostics.univariant_traces_started += 1;
    marks.begin();

    let start =
        nodes
            .get(start_node.0)
            .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
                message: "starting node is absent".into(),
            })?;
    let mut points = vec![start.point()];
    let mut temperatures = vec![start.temperature()];
    let mut terminal = None;

    for step in 0..options.maximum_trace_steps {
        if used[fragment_index] {
            diagnostics.directed_traversal_rejections += 1;
            return Err(StableBoundaryError::RepeatedTriangleTransition {
                triangle: fragments[fragment_index].triangle,
            });
        }
        used[fragment_index] = true;
        let fragment = &fragments[fragment_index];
        if fragment.pair != phases {
            return Err(StableBoundaryError::UnivariantLeftStablePairRegion {
                phases,
                point: fragment.endpoints[entry_side].point,
            });
        }
        marks.mark_triangle(fragment.triangle)?;
        let exit_side = 1 - entry_side;
        let exit = &fragment.endpoints[exit_side];
        append_path_point(
            &mut points,
            &mut temperatures,
            exit,
            options.geometry_tolerance,
        );
        if let Some(node) = exit.node {
            consume_pending(
                pending,
                pending_lookup,
                fragment_index,
                exit_side,
                diagnostics,
            )?;
            terminal = Some(node);
            diagnostics.maximum_trace_length = diagnostics.maximum_trace_length.max(step + 1);
            break;
        }

        let candidates = adjacency
            .get(&(phases, exit.key))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let incoming = oriented_direction(fragment, entry_side);
        let Some((next_fragment, next_entry_side)) =
            choose_forward_fragment(candidates, fragments, used, fragment_index, incoming)?
        else {
            return Err(StableBoundaryError::MalformedGraphConnectivity {
                message: format!(
                    "univariant {:?} terminates away from an invariant node",
                    phases
                ),
            });
        };
        marks.mark_feature(&exit.feature, fragment.triangle, topology, diagnostics)?;
        fragment_index = next_fragment;
        entry_side = next_entry_side;
    }
    let Some(end_node) = terminal else {
        return Err(StableBoundaryError::TraceStepLimitExceeded { start: start_id });
    };

    if end_node == start_node {
        return Err(StableBoundaryError::MalformedGraphConnectivity {
            message: "boundary-seeded trace returned to its starting invariant".into(),
        });
    }
    let end =
        nodes
            .get(end_node.0)
            .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
                message: "terminal node is absent".into(),
            })?;
    if let Some(first) = points.first_mut() {
        *first = start.point();
    }
    if let Some(last) = points.last_mut() {
        *last = end.point();
    }
    if let Some(first) = temperatures.first_mut() {
        *first = start.temperature();
    }
    if let Some(last) = temperatures.last_mut() {
        *last = end.temperature();
    }
    if points.len() < 2
        || points
            .windows(2)
            .any(|pair| composition_distance(pair[0], pair[1]) <= options.geometry_tolerance)
    {
        return Err(StableBoundaryError::MalformedGraphConnectivity {
            message: "univariant contains a duplicate or zero-length segment".into(),
        });
    }
    Ok(StableUnivariantPath {
        id: StableUnivariantId(0),
        phases,
        start: start_node,
        end: end_node,
        points,
        temperatures,
        regularization: None,
    })
}

#[derive(Clone, Copy, Debug)]
enum StableProjectionRejection {
    Undefined(StablePhaseId, TernaryCoordinate),
    Unstable(TernaryCoordinate),
    Branch(TernaryCoordinate),
}

struct StablePairEvaluation {
    sample: crate::path::ImplicitSample,
    temperature: f64,
    triangle: usize,
}

fn height_layer<'a>(
    layers: &'a [PreparedSourceLayer<'a>],
    phase: StablePhaseId,
) -> Result<&'a PreparedSourceLayer<'a>, StableBoundaryError> {
    layers
        .iter()
        .find(|layer| layer.role == ScalarRole::Height && layer.phase == phase)
        .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
            message: format!("phase {phase:?} has no prepared height layer"),
        })
}

fn phase_value_at(
    layers: &[PreparedSourceLayer<'_>],
    phase: StablePhaseId,
    point: TernaryCoordinate,
) -> Result<Option<f64>, StableBoundaryError> {
    let layer = height_layer(layers, phase)?;
    Ok(match evaluate_layer_at_point(layer, point.as_array())? {
        StablePhaseEvaluation::Defined { value } => Some(value),
        StablePhaseEvaluation::Undefined { .. } => None,
    })
}

fn sampled_pair_gradient(
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
    pair: StablePhasePair,
    point: TernaryCoordinate,
) -> Result<([f64; 2], usize), StableBoundaryError> {
    let location = samples.grid.locate(point.as_array()).map_err(|error| {
        StableBoundaryError::InvalidRegularGridTopology {
            message: error.to_string(),
        }
    })?;
    let first = phase_ids.binary_search(&pair.first).map_err(|_| {
        StableBoundaryError::MalformedGraphConnectivity {
            message: format!("phase {:?} is absent from sampling data", pair.first),
        }
    })?;
    let second = phase_ids.binary_search(&pair.second).map_err(|_| {
        StableBoundaryError::MalformedGraphConnectivity {
            message: format!("phase {:?} is absent from sampling data", pair.second),
        }
    })?;
    let first_values = samples
        .triangle_height_values(first, location.triangle.vertices)
        .ok_or(StableBoundaryError::RegularizationUndefinedPhase {
            phase: pair.first,
            point,
        })?;
    let second_values = samples
        .triangle_height_values(second, location.triangle.vertices)
        .ok_or(StableBoundaryError::RegularizationUndefinedPhase {
            phase: pair.second,
            point,
        })?;
    let differences = [
        first_values[0] - second_values[0],
        first_values[1] - second_values[1],
        first_values[2] - second_values[2],
    ];
    let vertices = location
        .triangle
        .vertices
        .map(|vertex| samples.grid.composition(vertex))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let vertices: [[f64; 3]; 3] =
        vertices
            .try_into()
            .map_err(|_| StableBoundaryError::InvalidRegularGridTopology {
                message: "located triangle does not have three vertices".into(),
            })?;
    let gradient = crate::simplex::global_gradient_ab(
        vertices,
        [
            differences[0] - differences[2],
            differences[1] - differences[2],
        ],
    )
    .ok_or(StableBoundaryError::RegularizationZeroGradient {
        phases: pair,
        residual: f64::NAN,
    })?;
    Ok((gradient, location.triangle.id))
}

fn evaluate_stable_pair(
    layers: &[PreparedSourceLayer<'_>],
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
    pair: StablePhasePair,
    point: TernaryCoordinate,
    stability_tolerance: f64,
) -> Result<Result<StablePairEvaluation, StableProjectionRejection>, StableBoundaryError> {
    let Some(first) = phase_value_at(layers, pair.first, point)? else {
        return Ok(Err(StableProjectionRejection::Undefined(pair.first, point)));
    };
    let Some(second) = phase_value_at(layers, pair.second, point)? else {
        return Ok(Err(StableProjectionRejection::Undefined(
            pair.second,
            point,
        )));
    };
    let pair_floor = first.min(second);
    for &phase in phase_ids {
        if pair.contains(phase) {
            continue;
        }
        if let Some(value) = phase_value_at(layers, phase, point)?
            && value > pair_floor + stability_tolerance
        {
            return Ok(Err(StableProjectionRejection::Unstable(point)));
        }
    }
    let (gradient_ab, triangle) = sampled_pair_gradient(samples, phase_ids, pair, point)?;
    Ok(Ok(StablePairEvaluation {
        sample: crate::path::ImplicitSample {
            residual: first - second,
            gradient_ab,
        },
        temperature: 0.5 * (first + second),
        triangle,
    }))
}

fn branch_candidate_valid(
    candidate: TernaryCoordinate,
    seed: TernaryCoordinate,
    segment: [TernaryCoordinate; 2],
    maximum_distance: f64,
) -> bool {
    if composition_distance(candidate, seed) > maximum_distance {
        return false;
    }
    let candidate = crate::simplex::logical_from_composition(candidate.as_array());
    let start = crate::simplex::logical_from_composition(segment[0].as_array());
    let end = crate::simplex::logical_from_composition(segment[1].as_array());
    let direction = [end[0] - start[0], end[1] - start[1]];
    let denominator = direction[0].powi(2) + direction[1].powi(2);
    if denominator <= f64::EPSILON {
        return false;
    }
    let parameter = ((candidate[0] - start[0]) * direction[0]
        + (candidate[1] - start[1]) * direction[1])
        / denominator;
    (-0.5..=1.5).contains(&parameter)
}

fn rejection_error(
    rejection: Option<StableProjectionRejection>,
    phases: StablePhasePair,
    residual: f64,
    iterations: usize,
) -> StableBoundaryError {
    match rejection {
        Some(StableProjectionRejection::Undefined(phase, point)) => {
            StableBoundaryError::RegularizationUndefinedPhase { phase, point }
        }
        Some(StableProjectionRejection::Unstable(point)) => {
            StableBoundaryError::RegularizationUnstableProjection { phases, point }
        }
        Some(StableProjectionRejection::Branch(point)) => {
            StableBoundaryError::RegularizationBranchSwitch { phases, point }
        }
        None => StableBoundaryError::RegularizationNonConvergence {
            phases,
            residual,
            iterations,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn regularize_univariant(
    path: &mut StableUnivariantPath,
    nodes: &[StableInvariantNode],
    layers: &[PreparedSourceLayer<'_>],
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
    boundary_options: StableBoundaryOptions,
    options: PathRegularizationOptions,
) -> Result<(), StableBoundaryError> {
    let start = nodes[path.start.0].point();
    let end = nodes[path.end.0].point();
    let raw = crate::path::cleanup(&path.points, false, boundary_options.geometry_tolerance)
        .map_err(|error| StableBoundaryError::InvalidOptions {
            message: format!("raw univariant cleanup failed: {error:?}"),
        })?;
    let raw_self_intersection =
        crate::path::has_self_intersection(&raw, false, boundary_options.geometry_tolerance);
    let mut diagnostics = StableUnivariantRegularizationDiagnostics {
        raw_point_count: raw.len(),
        raw_logical_length: crate::path::path_length(&raw, false),
        spacing_cv_before: crate::path::spacing_coefficient_of_variation(&raw, false),
        ..StableUnivariantRegularizationDiagnostics::default()
    };
    let mut current = raw;
    for _ in 0..options.redistribution_passes.max(1) {
        let redistributed =
            crate::path::redistribute(&current, false, options.spacing).map_err(|error| {
                StableBoundaryError::InvalidOptions {
                    message: format!("univariant redistribution failed: {error:?}"),
                }
            })?;
        let source_total = crate::path::path_length(&current, false);
        let seeds = redistributed
            .iter()
            .map(|sample| sample.point)
            .collect::<Vec<_>>();
        let mut projected = Vec::with_capacity(redistributed.len());
        for (index, sample) in redistributed.iter().enumerate() {
            if index == 0 {
                projected.push(start);
                continue;
            }
            if index + 1 == redistributed.len() {
                projected.push(end);
                continue;
            }
            let segment = [
                current[sample.source_segment],
                current[sample.source_segment + 1],
            ];
            let initial = evaluate_stable_pair(
                layers,
                samples,
                phase_ids,
                path.phases,
                sample.point,
                boundary_options.stability_tolerance,
            )?;
            let protected = sample.source_arclength <= options.protected_endpoint_distance
                || source_total - sample.source_arclength <= options.protected_endpoint_distance;
            if protected
                && initial
                    .as_ref()
                    .is_ok_and(|state| state.sample.residual.abs() <= options.projection_tolerance)
            {
                projected.push(sample.point);
                continue;
            }
            let initial_triangle = initial.as_ref().ok().map(|state| state.triangle);
            let mut last_rejection = initial.err();
            let maximum_distance = (options.max_normal_step * 2.0)
                .max(options.spacing * 2.0)
                .max(boundary_options.geometry_tolerance * 8.0);
            let result = crate::path::project_implicit(sample.point, options, |candidate| {
                if !branch_candidate_valid(candidate, sample.point, segment, maximum_distance) {
                    last_rejection = Some(StableProjectionRejection::Branch(candidate));
                    return Err(crate::path::ProjectionEvaluationError::Reject);
                }
                match evaluate_stable_pair(
                    layers,
                    samples,
                    phase_ids,
                    path.phases,
                    candidate,
                    boundary_options.stability_tolerance,
                ) {
                    Ok(Ok(state)) => Ok(state.sample),
                    Ok(Err(rejection)) => {
                        match rejection {
                            StableProjectionRejection::Undefined(_, _) => {
                                diagnostics.rejected_undefined_projections += 1;
                            }
                            StableProjectionRejection::Unstable(_) => {
                                diagnostics.rejected_unstable_projections += 1;
                            }
                            StableProjectionRejection::Branch(_) => {}
                        }
                        last_rejection = Some(rejection);
                        Err(crate::path::ProjectionEvaluationError::Reject)
                    }
                    Err(error) => Err(crate::path::ProjectionEvaluationError::Fatal(error)),
                }
            });
            let outcome = match result {
                Ok(outcome) => outcome,
                Err(crate::path::ImplicitProjectionError::Evaluation(error)) => return Err(error),
                Err(crate::path::ImplicitProjectionError::ZeroGradient { residual }) => {
                    return Err(StableBoundaryError::RegularizationZeroGradient {
                        phases: path.phases,
                        residual,
                    });
                }
                Err(crate::path::ImplicitProjectionError::NonConvergence {
                    residual,
                    iterations,
                }) => {
                    return Err(rejection_error(
                        last_rejection,
                        path.phases,
                        residual,
                        iterations,
                    ));
                }
                Err(crate::path::ImplicitProjectionError::RejectedInitialPoint) => {
                    return Err(rejection_error(last_rejection, path.phases, f64::NAN, 0));
                }
            };
            diagnostics.accepted_projections += 1;
            diagnostics.projection_iterations += outcome.iterations;
            diagnostics.backtracked_projections += outcome.backtracking_steps;
            let final_state = evaluate_stable_pair(
                layers,
                samples,
                phase_ids,
                path.phases,
                outcome.point,
                boundary_options.stability_tolerance,
            )?
            .map_err(|rejection| rejection_error(Some(rejection), path.phases, f64::NAN, 0))?;
            if initial_triangle.is_some_and(|triangle| triangle != final_state.triangle) {
                diagnostics.sampling_triangle_relocations += 1;
            }
            projected.push(outcome.point);
        }
        if projected
            .windows(2)
            .zip(seeds.windows(2))
            .any(|(final_pair, raw_pair)| {
                let final_start =
                    crate::simplex::logical_from_composition(final_pair[0].as_array());
                let final_end = crate::simplex::logical_from_composition(final_pair[1].as_array());
                let raw_start = crate::simplex::logical_from_composition(raw_pair[0].as_array());
                let raw_end = crate::simplex::logical_from_composition(raw_pair[1].as_array());
                (final_end[0] - final_start[0]) * (raw_end[0] - raw_start[0])
                    + (final_end[1] - final_start[1]) * (raw_end[1] - raw_start[1])
                    < -boundary_options.geometry_tolerance
            })
        {
            return Err(StableBoundaryError::RegularizationBranchSwitch {
                phases: path.phases,
                point: projected[0],
            });
        }
        current = crate::path::cleanup(&projected, false, boundary_options.geometry_tolerance)
            .map_err(|error| StableBoundaryError::InvalidOptions {
                message: format!("regularized univariant cleanup failed: {error:?}"),
            })?;
        current[0] = start;
        *current
            .last_mut()
            .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
                message: "regularized path lost its terminal endpoint".into(),
            })? = end;
    }

    if !raw_self_intersection
        && crate::path::has_self_intersection(&current, false, boundary_options.geometry_tolerance)
    {
        return Err(StableBoundaryError::RegularizationBranchSwitch {
            phases: path.phases,
            point: current[0],
        });
    }
    let mut temperatures = Vec::with_capacity(current.len());
    for &point in &current {
        let state = evaluate_stable_pair(
            layers,
            samples,
            phase_ids,
            path.phases,
            point,
            boundary_options.stability_tolerance,
        )?
        .map_err(|rejection| rejection_error(Some(rejection), path.phases, f64::NAN, 0))?;
        diagnostics.maximum_pair_residual = diagnostics
            .maximum_pair_residual
            .max(state.sample.residual.abs());
        if state.sample.residual.abs() > options.projection_tolerance {
            return Err(StableBoundaryError::RegularizationNonConvergence {
                phases: path.phases,
                residual: state.sample.residual,
                iterations: options.max_projection_iterations,
            });
        }
        temperatures.push(state.temperature);
    }
    diagnostics.final_point_count = current.len();
    diagnostics.final_logical_length = crate::path::path_length(&current, false);
    diagnostics.spacing_cv_after = crate::path::spacing_coefficient_of_variation(&current, false);
    path.points = current;
    path.temperatures = temperatures;
    path.regularization = Some(diagnostics);
    Ok(())
}

fn regularize_network(
    network: &mut StableBoundaryNetwork,
    layers: &[PreparedSourceLayer<'_>],
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
    boundary_options: StableBoundaryOptions,
    options: PathRegularizationOptions,
) -> Result<(), StableBoundaryError> {
    for path in &mut network.univariants {
        regularize_univariant(
            path,
            &network.nodes,
            layers,
            samples,
            phase_ids,
            boundary_options,
            options,
        )?;
    }
    Ok(())
}
fn validate_network(network: &StableBoundaryNetwork) -> Result<(), StableBoundaryError> {
    if network.incidence.len() != network.nodes.len() {
        return Err(StableBoundaryError::MalformedGraphConnectivity {
            message: "incidence array does not match the node count".into(),
        });
    }
    for (index, node) in network.nodes.iter().enumerate() {
        if node.id().0 != index {
            return Err(StableBoundaryError::MalformedGraphConnectivity {
                message: "node identifiers are not dense".into(),
            });
        }
    }
    for (index, path) in network.univariants.iter().enumerate() {
        if path.id.0 != index
            || path.points.len() != path.temperatures.len()
            || path.points.len() < 2
        {
            return Err(StableBoundaryError::MalformedGraphConnectivity {
                message: "univariant identifiers or point arrays are malformed".into(),
            });
        }
        let start = network.nodes.get(path.start.0).ok_or_else(|| {
            StableBoundaryError::MalformedGraphConnectivity {
                message: "univariant start node is absent".into(),
            }
        })?;
        let end = network.nodes.get(path.end.0).ok_or_else(|| {
            StableBoundaryError::MalformedGraphConnectivity {
                message: "univariant end node is absent".into(),
            }
        })?;
        if path.points.first() != Some(&start.point()) || path.points.last() != Some(&end.point()) {
            return Err(StableBoundaryError::MalformedGraphConnectivity {
                message: "univariant endpoints are not canonical node coordinates".into(),
            });
        }
        if !network.incidence[path.start.0].contains(&path.id)
            || !network.incidence[path.end.0].contains(&path.id)
        {
            return Err(StableBoundaryError::MalformedGraphConnectivity {
                message: "univariant is absent from terminal-node incidence".into(),
            });
        }
    }
    Ok(())
}

pub(crate) fn build_stable_boundary_network(
    traces: Vec<BinaryBoundaryTrace>,
    cells: &[super::partition::StableSamplingCell],
    samples: &super::sample::RegularSamplingGrid,
    phase_ids: &[StablePhaseId],
    layers: &[PreparedSourceLayer<'_>],
    options: StableBoundaryOptions,
) -> Result<StableBoundaryNetwork, StableBoundaryError> {
    options.validate()?;
    let topology = RegularSamplingTopology::new(samples.grid)?;
    let mut diagnostics = StableBoundaryDiagnostics::default();
    aggregate_binary_diagnostics(&traces, &mut diagnostics);
    topology_diagnostics(&topology, &mut diagnostics)?;
    let mut nodes = canonical_nodes(&traces)?;
    let mut fragments = collect_boundary_fragments(
        cells,
        samples,
        phase_ids,
        &topology,
        options,
        &mut diagnostics,
    )?;
    match_binary_endpoints(
        &mut fragments,
        &traces,
        &nodes,
        samples.grid.subdivisions(),
        options,
    )?;
    canonicalize_interior_nodes(&mut fragments, &mut nodes, options, &mut diagnostics)?;
    let adjacency = build_fragment_adjacency(&fragments);
    let (mut pending, pending_lookup) = create_pending_ends(&fragments, &nodes, &mut diagnostics)?;
    let mut used = vec![false; fragments.len()];
    let mut marks = RegularGridTraceMarks::new(&topology)?;
    let mut univariants = Vec::new();
    for pending_index in 0..pending.len() {
        if pending[pending_index].consumed {
            continue;
        }
        let mut path = trace_one_univariant(
            pending_index,
            &mut pending,
            &pending_lookup,
            &fragments,
            &adjacency,
            &mut used,
            &nodes,
            &topology,
            &mut marks,
            options,
            &mut diagnostics,
        )?;
        path.id = StableUnivariantId(univariants.len());
        univariants.push(path);
    }
    let unresolved = pending.iter().filter(|end| !end.consumed).count();
    if unresolved != 0 {
        return Err(StableBoundaryError::UnresolvedPendingEnds { count: unresolved });
    }
    let mut incidence = vec![Vec::new(); nodes.len()];
    for path in &univariants {
        incidence[path.start.0].push(path.id);
        incidence[path.end.0].push(path.id);
    }
    for paths in &mut incidence {
        paths.sort();
        paths.dedup();
    }
    diagnostics.completed_univariants = univariants.len();
    let mut network = StableBoundaryNetwork {
        nodes,
        univariants,
        binary_traces: traces,
        diagnostics,
        incidence,
    };
    validate_network(&network)?;
    if let Some(regularization) = options.regularization {
        regularize_network(
            &mut network,
            layers,
            samples,
            phase_ids,
            options,
            regularization,
        )?;
        validate_network(&network)?;
    }
    Ok(network)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PreparedStablePhaseEnsemble, StableContourQuantity, StableGridOptions, StablePhaseSource,
        StablePhaseUndefinedReason, StableScalarSource,
    };

    fn defined(value: f64) -> StablePhaseEvaluation {
        StablePhaseEvaluation::Defined { value }
    }

    fn undefined() -> StablePhaseEvaluation {
        StablePhaseEvaluation::Undefined {
            reason: StablePhaseUndefinedReason::OutsidePhaseDomain,
        }
    }

    #[test]
    fn canonical_binary_parameterizations_round_trip() {
        for boundary in BinaryBoundary::ALL {
            for parameter in [0.0, 0.125, 0.5, 0.875, 1.0] {
                let point = boundary.composition(parameter).unwrap();
                assert!((boundary.parameter(point).unwrap() - parameter).abs() < 1.0e-14);
                let components = point.as_array();
                assert!((components.into_iter().sum::<f64>() - 1.0).abs() < 1.0e-14);
            }
        }
    }

    #[test]
    fn partial_domains_overlap_only_near_the_stable_binary_transition() {
        let left = |[_a, b, _c]: [f64; 3]| {
            if b <= 0.60 {
                defined(1.0 - b)
            } else {
                undefined()
            }
        };
        let right = |[_a, b, _c]: [f64; 3]| {
            if b >= 0.40 { defined(b) } else { undefined() }
        };
        let phases = [
            StablePhaseSource::new(StablePhaseId(10), StableScalarSource::evaluator(&left)),
            StablePhaseSource::new(StablePhaseId(20), StableScalarSource::evaluator(&right)),
        ];
        let prepared = PreparedStablePhaseEnsemble::new(
            phases,
            StableContourQuantity::Height,
            StableGridOptions {
                subdivisions: 16,
                ..StableGridOptions::default()
            },
        )
        .unwrap();
        let traces = prepared
            .binary_boundary_traces(StableBoundaryOptions::default())
            .unwrap();
        let ab = &traces[0];
        let invariant = ab
            .invariants
            .iter()
            .find(|node| {
                node.phases.contains(&StablePhaseId(10)) && node.phases.contains(&StablePhaseId(20))
            })
            .unwrap();
        assert!((invariant.boundary_parameter - 0.5).abs() < 1.0e-9);
        assert!((invariant.temperature - 0.5).abs() < 1.0e-9);
        assert!(ab.diagnostics.cached_evaluations_reused > 0);
    }

    #[test]
    fn hidden_pairwise_intersection_inserts_the_stable_intermediate_phase() {
        let alpha = |[_a, b, _c]: [f64; 3]| defined(1.0 - b);
        let beta = |[_a, b, _c]: [f64; 3]| defined(b);
        let gamma = |[_a, b, _c]: [f64; 3]| defined(0.62 - 4.0 * (b - 0.5) * (b - 0.5));
        let phases = [
            StablePhaseSource::new(StablePhaseId(1), StableScalarSource::evaluator(&alpha)),
            StablePhaseSource::new(StablePhaseId(2), StableScalarSource::evaluator(&beta)),
            StablePhaseSource::new(StablePhaseId(3), StableScalarSource::evaluator(&gamma)),
        ];
        let prepared = PreparedStablePhaseEnsemble::new(
            phases,
            StableContourQuantity::Height,
            StableGridOptions {
                subdivisions: 24,
                ..StableGridOptions::default()
            },
        )
        .unwrap();
        let trace = &prepared
            .binary_boundary_traces(StableBoundaryOptions::default())
            .unwrap()[0];
        let ordered = trace
            .regions
            .iter()
            .filter_map(|region| region.stable_phases.first().copied())
            .collect::<Vec<_>>();
        assert!(
            ordered
                .windows(3)
                .any(|window| { window == [StablePhaseId(1), StablePhaseId(3), StablePhaseId(2)] })
        );
        assert!(
            !trace
                .invariants
                .iter()
                .any(|node| { node.phases == [StablePhaseId(1), StablePhaseId(2)] })
        );
    }

    #[test]
    fn higher_order_binary_tie_is_one_canonical_node() {
        let alpha = |[_a, b, _c]: [f64; 3]| defined(1.0 - b);
        let beta = |[_a, b, _c]: [f64; 3]| defined(b);
        let gamma = |[_a, b, _c]: [f64; 3]| defined(0.5 - 2.0 * (b - 0.5) * (b - 0.5));
        let phases = [
            StablePhaseSource::new(StablePhaseId(3), StableScalarSource::evaluator(&gamma)),
            StablePhaseSource::new(StablePhaseId(1), StableScalarSource::evaluator(&alpha)),
            StablePhaseSource::new(StablePhaseId(2), StableScalarSource::evaluator(&beta)),
        ];
        let prepared = PreparedStablePhaseEnsemble::new(
            phases,
            StableContourQuantity::Height,
            StableGridOptions {
                subdivisions: 16,
                ..StableGridOptions::default()
            },
        )
        .unwrap();
        let trace = &prepared
            .binary_boundary_traces(StableBoundaryOptions::default())
            .unwrap()[0];
        let higher = trace
            .invariants
            .iter()
            .filter(|node| node.phases.len() >= 3)
            .collect::<Vec<_>>();
        assert_eq!(higher.len(), 1);
        assert_eq!(
            higher[0].phases,
            [StablePhaseId(1), StablePhaseId(2), StablePhaseId(3)]
        );
    }

    #[test]
    fn affine_three_phase_network_connects_binary_nodes_to_one_interior_invariant() {
        let alpha = |[a, _b, _c]: [f64; 3]| defined(a);
        let beta = |[_a, b, _c]: [f64; 3]| defined(b);
        let gamma = |[_a, _b, c]: [f64; 3]| defined(c);
        let phases = [
            StablePhaseSource::new(StablePhaseId(30), StableScalarSource::evaluator(&gamma)),
            StablePhaseSource::new(StablePhaseId(10), StableScalarSource::evaluator(&alpha)),
            StablePhaseSource::new(StablePhaseId(20), StableScalarSource::evaluator(&beta)),
        ];
        let prepared = PreparedStablePhaseEnsemble::new(
            phases,
            StableContourQuantity::Height,
            StableGridOptions {
                subdivisions: 12,
                ..StableGridOptions::default()
            },
        )
        .unwrap();
        let network = prepared
            .stable_boundaries(StableBoundaryOptions::default())
            .unwrap();
        assert_eq!(network.nodes.len(), 4);
        assert_eq!(network.univariants.len(), 3);
        let interior = network
            .nodes
            .iter()
            .find(|node| matches!(node, StableInvariantNode::Interior(_)))
            .unwrap();
        assert_eq!(interior.phases().len(), 3);
        for path in &network.univariants {
            assert_eq!(
                path.points.first(),
                Some(&network.nodes[path.start.0].point())
            );
            assert_eq!(path.points.last(), Some(&network.nodes[path.end.0].point()));
            assert!(
                path.points
                    .windows(2)
                    .all(|points| { composition_distance(points[0], points[1]) > 1.0e-9 })
            );
            assert!(path.start == interior.id() || path.end == interior.id());
        }
        assert_eq!(
            network.incident_univariants(interior.id()).unwrap().len(),
            3
        );
    }

    #[test]
    fn affine_univariant_regularization_preserves_nodes_pairs_and_connectivity() {
        let alpha = |[a, _b, _c]: [f64; 3]| defined(a);
        let beta = |[_a, b, _c]: [f64; 3]| defined(b);
        let gamma = |[_a, _b, c]: [f64; 3]| defined(c);
        let phases = [
            StablePhaseSource::new(StablePhaseId(1), StableScalarSource::evaluator(&alpha)),
            StablePhaseSource::new(StablePhaseId(2), StableScalarSource::evaluator(&beta)),
            StablePhaseSource::new(StablePhaseId(3), StableScalarSource::evaluator(&gamma)),
        ];
        let prepared = PreparedStablePhaseEnsemble::new(
            phases,
            StableContourQuantity::Height,
            StableGridOptions {
                subdivisions: 12,
                ..StableGridOptions::default()
            },
        )
        .unwrap();
        let raw = prepared
            .stable_boundaries(StableBoundaryOptions::default())
            .unwrap();
        let regularized = prepared
            .stable_boundaries(StableBoundaryOptions {
                regularization: Some(PathRegularizationOptions {
                    spacing: 0.025,
                    protected_endpoint_distance: 0.0,
                    ..PathRegularizationOptions::default()
                }),
                ..StableBoundaryOptions::default()
            })
            .unwrap();
        assert_eq!(regularized.nodes, raw.nodes);
        assert_eq!(regularized.univariants.len(), raw.univariants.len());
        for (path, raw_path) in regularized.univariants.iter().zip(&raw.univariants) {
            assert_eq!(path.phases, raw_path.phases);
            assert_eq!((path.start, path.end), (raw_path.start, raw_path.end));
            assert_eq!(path.points[0], regularized.nodes[path.start.0].point());
            assert_eq!(
                path.points.last(),
                Some(&regularized.nodes[path.end.0].point())
            );
            let diagnostics = path.regularization.as_ref().unwrap();
            assert_eq!(diagnostics.raw_point_count, raw_path.points.len());
            assert_eq!(diagnostics.final_point_count, path.points.len());
            assert!(diagnostics.maximum_pair_residual <= 1.0e-9);
            assert!(
                path.points
                    .windows(2)
                    .all(|points| { composition_distance(points[0], points[1]) > 1.0e-9 })
            );
        }
        for node in &regularized.nodes {
            assert_eq!(
                regularized.incident_univariants(node.id()).unwrap(),
                raw.incident_univariants(node.id()).unwrap()
            );
        }
    }

    #[test]
    fn curved_univariant_is_projected_to_pair_equality_across_sampling_cells() {
        let alpha = |[a, _b, c]: [f64; 3]| defined(a + 0.4 * c * c);
        let beta = |[_a, b, _c]: [f64; 3]| defined(b);
        let phases = [
            StablePhaseSource::new(StablePhaseId(1), StableScalarSource::evaluator(&alpha)),
            StablePhaseSource::new(StablePhaseId(2), StableScalarSource::evaluator(&beta)),
        ];
        let prepared = PreparedStablePhaseEnsemble::new(
            phases,
            StableContourQuantity::Height,
            StableGridOptions {
                subdivisions: 8,
                ..StableGridOptions::default()
            },
        )
        .unwrap();
        let network = prepared
            .stable_boundaries(StableBoundaryOptions {
                regularization: Some(PathRegularizationOptions {
                    spacing: 0.02,
                    protected_endpoint_distance: 0.0,
                    max_normal_step: 0.2,
                    ..PathRegularizationOptions::default()
                }),
                ..StableBoundaryOptions::default()
            })
            .unwrap();
        assert_eq!(network.univariants.len(), 1);
        let path = &network.univariants[0];
        assert!(path.points.len() > 10);
        for point in &path.points {
            let [a, b, c] = point.as_array();
            assert!((a + 0.4 * c * c - b).abs() <= 1.0e-9);
        }
        let diagnostics = path.regularization.as_ref().unwrap();
        assert!(diagnostics.accepted_projections > 0);
        assert!(diagnostics.maximum_pair_residual <= 1.0e-9);
    }

    #[test]
    fn phase_input_permutation_and_repeated_raw_construction_are_exact() {
        let alpha = |[a, _b, _c]: [f64; 3]| defined(a);
        let beta = |[_a, b, _c]: [f64; 3]| defined(b);
        let gamma = |[_a, _b, c]: [f64; 3]| defined(c);
        let make = |phases| {
            PreparedStablePhaseEnsemble::new(
                phases,
                StableContourQuantity::Height,
                StableGridOptions {
                    subdivisions: 9,
                    ..StableGridOptions::default()
                },
            )
            .unwrap()
        };
        let first = make([
            StablePhaseSource::new(StablePhaseId(1), StableScalarSource::evaluator(&alpha)),
            StablePhaseSource::new(StablePhaseId(2), StableScalarSource::evaluator(&beta)),
            StablePhaseSource::new(StablePhaseId(3), StableScalarSource::evaluator(&gamma)),
        ]);
        let second = make([
            StablePhaseSource::new(StablePhaseId(3), StableScalarSource::evaluator(&gamma)),
            StablePhaseSource::new(StablePhaseId(1), StableScalarSource::evaluator(&alpha)),
            StablePhaseSource::new(StablePhaseId(2), StableScalarSource::evaluator(&beta)),
        ]);
        let expected = first
            .stable_boundaries(StableBoundaryOptions::default())
            .unwrap();
        assert_eq!(
            expected,
            first
                .stable_boundaries(StableBoundaryOptions::default())
                .unwrap()
        );
        assert_eq!(
            expected,
            second
                .stable_boundaries(StableBoundaryOptions::default())
                .unwrap()
        );
        assert!(
            expected
                .univariants
                .iter()
                .all(|path| path.regularization.is_none())
        );
    }

    #[test]
    fn isolated_closed_univariant_is_explicitly_deferred_without_a_boundary_seed() {
        let island = |[a, b, c]: [f64; 3]| {
            let radius =
                (a - 1.0 / 3.0).powi(2) + (b - 1.0 / 3.0).powi(2) + (c - 1.0 / 3.0).powi(2);
            defined(0.04 - radius)
        };
        let matrix = |_: [f64; 3]| defined(0.0);
        let prepared = PreparedStablePhaseEnsemble::new(
            [
                StablePhaseSource::new(StablePhaseId(1), StableScalarSource::evaluator(&island)),
                StablePhaseSource::new(StablePhaseId(2), StableScalarSource::evaluator(&matrix)),
            ],
            StableContourQuantity::Height,
            StableGridOptions {
                subdivisions: 24,
                ..StableGridOptions::default()
            },
        )
        .unwrap();
        let network = prepared
            .stable_boundaries(StableBoundaryOptions::default())
            .unwrap();
        assert!(network.nodes.is_empty());
        assert!(network.univariants.is_empty());
    }

    #[test]
    fn dense_trace_marks_reject_repeated_directed_states() {
        let topology =
            RegularSamplingTopology::new(crate::RegularTernaryGrid::new(2).unwrap()).unwrap();
        let mut marks = RegularGridTraceMarks::new(&topology).unwrap();
        let mut diagnostics = StableBoundaryDiagnostics::default();
        marks.begin();
        marks.mark_triangle(0).unwrap();
        assert!(matches!(
            marks.mark_triangle(0),
            Err(StableBoundaryError::RepeatedTriangleTransition { triangle: 0 })
        ));
        let feature = TraceFeature::Edge(topology.triangle_edges(0).unwrap()[0].edge);
        marks
            .mark_feature(&feature, 0, &topology, &mut diagnostics)
            .unwrap();
        assert!(matches!(
            marks.mark_feature(&feature, 0, &topology, &mut diagnostics),
            Err(StableBoundaryError::RepeatedDirectedEdgeTraversal { .. })
        ));
    }

    #[test]
    fn uncovered_binary_point_is_a_typed_error() {
        let left = |[_a, b, _c]: [f64; 3]| {
            if b <= 0.45 {
                defined(1.0 - b)
            } else {
                undefined()
            }
        };
        let right = |[_a, b, _c]: [f64; 3]| {
            if b >= 0.55 { defined(b) } else { undefined() }
        };
        let phases = [
            StablePhaseSource::new(StablePhaseId(1), StableScalarSource::evaluator(&left)),
            StablePhaseSource::new(StablePhaseId(2), StableScalarSource::evaluator(&right)),
        ];
        let prepared = PreparedStablePhaseEnsemble::new(
            phases,
            StableContourQuantity::Height,
            StableGridOptions {
                subdivisions: 3,
                ..StableGridOptions::default()
            },
        )
        .unwrap();
        assert!(matches!(
            prepared.binary_boundary_traces(StableBoundaryOptions::default()),
            Err(StableBoundaryError::NoPhaseDefined {
                boundary: Some(BinaryBoundary::Ab),
                ..
            })
        ));
    }
}
