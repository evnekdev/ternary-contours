use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::{FieldError, RegularSamplingTopology, TernaryCoordinate};

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

/// One complete phase-pair path between invariant nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct StableUnivariantPath {
    pub id: StableUnivariantId,
    pub phases: StablePhasePair,
    pub start: StableInvariantNodeId,
    pub end: StableInvariantNodeId,
    pub points: Vec<TernaryCoordinate>,
    pub temperatures: Vec<f64>,
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
