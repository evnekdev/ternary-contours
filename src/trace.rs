//! Deterministic, opt-in structured observation events for numerical algorithms.
//!
//! A trace session is calculation-local and never participates in numerical
//! options, cache keys, document state, or floating-point decisions.  Ordinary
//! callers use [`NoopTraceSink`], which has no allocation or formatting cost.

use std::sync::OnceLock;

/// Stable schema version written by JSON Lines consumers.
pub const NUMERICAL_TRACE_SCHEMA_VERSION: u32 = 1;

/// Requested trace detail.  Higher levels include lower-level events.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "numerical-trace-serde", serde(rename_all = "snake_case"))]
pub enum NumericalTraceLevel {
    /// No observation or allocation.
    #[default]
    Off,
    /// Run metadata and stage summaries.
    Summary,
    /// Stable ownership, topology, and fallback decisions.
    Decisions,
    /// Per-iteration and local-geometry diagnostics.
    Iterations,
}

impl NumericalTraceLevel {
    /// Whether this configured level includes an event at `required` detail.
    pub fn includes(self, required: Self) -> bool {
        self != Self::Off && self >= required
    }
}

/// Canonical outer simplex boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "numerical-trace-serde", serde(rename_all = "UPPERCASE"))]
pub enum TraceBinaryBoundary {
    /// A/B with C=0.
    Ab,
    /// B/C with A=0.
    Bc,
    /// C/A with B=0.
    Ca,
}

/// Inclusive semantic A/B/C region filter.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct CompositionRegion {
    /// Inclusive lower A/B/C bounds.
    pub minimum: [f64; 3],
    /// Inclusive upper A/B/C bounds.
    pub maximum: [f64; 3],
}

impl CompositionRegion {
    /// Whether a finite composition belongs to this inclusive region.
    pub fn contains(self, composition: [f64; 3]) -> bool {
        composition
            .into_iter()
            .zip(self.minimum)
            .zip(self.maximum)
            .all(|((value, minimum), maximum)| {
                value.is_finite()
                    && minimum.is_finite()
                    && maximum.is_finite()
                    && minimum <= value
                    && value <= maximum
            })
    }
}

/// Stable names for machine-readable trace events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "numerical-trace-serde", serde(rename_all = "snake_case"))]
pub enum NumericalTraceEventKind {
    RunStarted,
    RunCompleted,
    RunFailed,
    TraceTruncated,
    SourcePreparationStarted,
    PhaseSourceLocated,
    PhaseFieldLocated,
    SourceGeometryClassified,
    RegularSourcePrepared,
    IrregularSourcePrepared,
    CubicSourcePrepared,
    PartialSourcePrepared,
    SourceCoverageComputed,
    SourceValueClassified,
    SourcePreparationCompleted,
    SourcePreparationRejected,
    InterpolationTriangleLocated,
    InterpolationOutsideDomain,
    InterpolationUndefined,
    InterpolationModeSelected,
    CubicFallbackSelected,
    OneSidedCubicSelected,
    LinearFallbackSelected,
    StablePhaseEvaluationStarted,
    PhaseValueEvaluated,
    PhaseValueUndefined,
    StableWinnerSelected,
    StableTieDetected,
    NoStablePhaseDefined,
    CompetitorAdded,
    CompetitorRejected,
    BinaryBoundaryStarted,
    BinarySampleEvaluated,
    BinaryStableRegionDetected,
    BinaryTransitionDetected,
    BinaryTransitionBracketed,
    BinaryIntermediatePhaseInserted,
    BinaryRootIteration,
    BinaryRootConverged,
    BinaryRootRejectedMetastable,
    BinaryInvariantEmitted,
    BinaryBoundaryCompleted,
    BinaryBoundaryFailed,
    InteriorCandidateDetected,
    InteriorCandidatePhases,
    InteriorSolveStarted,
    InteriorSolveIteration,
    InteriorSolveConverged,
    InteriorCandidateRejectedMetastable,
    InteriorCandidateRejectedOutsideTriangle,
    InteriorInvariantMergedWithKnown,
    InteriorInvariantAccepted,
    InteriorInvariantFailed,
    PendingEndCreated,
    PendingEndMatched,
    PendingEndConsumed,
    PendingEndLeftUnresolved,
    UnivariantTraceStarted,
    UnivariantTriangleEntered,
    UnivariantSamplingEdgeCrossed,
    UnivariantSamplingVertexCrossed,
    CanonicalEdgeHitReused,
    LocalCompetitorRefinementStarted,
    LocalCompetitorRefinementCompleted,
    KnownInvariantRevisited,
    UnivariantReachedInvariant,
    UnivariantTraceCompleted,
    UnivariantTraceRejected,
    UnivariantTraceFailed,
    ContourLevelStarted,
    ContourTriangleVisited,
    LocalContourSegmentGenerated,
    SegmentRejectedUndefined,
    SegmentClippedToStableRegion,
    StableBoundaryContactCreated,
    ContourJunctionCreated,
    ContourPathAssemblyStarted,
    ContourSegmentsJoined,
    ContourAssemblyAmbiguous,
    ContourPathCompleted,
    ContourLevelCompleted,
    RegularizationStarted,
    RegularizationPointProposed,
    RegularizationProjectionIteration,
    RegularizationProjectionAccepted,
    RegularizationProjectionBacktracked,
    RegularizationRejectedUnstable,
    RegularizationRejectedUndefined,
    RegularizationRejectedBranchSwitch,
    RegularizationSpacingUpdated,
    RegularizationCompleted,
    RegularizationFailed,
    InvalidOptions,
    NoPhaseDefined,
    BinaryDiscoveryResolutionExhausted,
    InvalidRootBracket,
    PairwiseRootRefinementExhausted,
    MetastableTransitionUnresolved,
    RepeatedDirectedEdgeTraversal,
    InvariantRefinementExhausted,
    UnresolvedPendingEnds,
    PathAssemblyAmbiguity,
    RegularizationNonConvergence,
}

/// Deterministic stage that owns an event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "numerical-trace-serde", serde(rename_all = "snake_case"))]
pub enum NumericalTraceStage {
    Run,
    SourcePreparation,
    Interpolation,
    StableSelection,
    BinaryBoundary,
    InteriorInvariant,
    Univariant,
    Contour,
    Regularization,
    Error,
}

/// Counted typed source states or accepted topology objects.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct TraceCounts {
    pub calculated: usize,
    pub non_existing: usize,
    pub cut_off: usize,
    pub missing: usize,
}

/// Metadata for a complete projection request.  The caller supplies only
/// reproducible identifiers; wall-clock data is intentionally absent.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct TraceRunStarted {
    pub crate_version: String,
    pub git_commit: Option<String>,
    pub calculation_kind: String,
    pub input_identifier: Option<String>,
    pub input_content_hash: Option<String>,
    pub components: [String; 3],
    pub phase_ids: Vec<u32>,
    pub phase_names: Vec<String>,
    pub property: String,
    pub unit: String,
    pub sampling_subdivisions: Option<usize>,
    pub interpolation: String,
    pub partial_domain_policy: String,
    pub continuation: String,
    pub regularization: bool,
    pub requested_levels: Vec<f64>,
    pub dataset_revision: Option<u64>,
    pub options_revision: Option<u64>,
    pub request_id: Option<u64>,
    pub trace_level: NumericalTraceLevel,
    pub trace_maximum_events: usize,
}

/// Summary emitted when a traceable calculation succeeds.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct TraceRunCompleted {
    pub invariant_count: usize,
    pub univariant_count: usize,
    pub contour_path_count: usize,
    pub trace_events: u64,
    pub truncated: bool,
}

/// Failure observation.  It augments, rather than replaces, the public error.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct TraceRunFailed {
    pub error_kind: NumericalTraceEventKind,
    pub message: String,
}

/// Typed decision fields shared by topology and interpolation observations.
/// Missing fields are inapplicable, never sentinel floating-point values.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct TraceDecision {
    pub boundary: Option<TraceBinaryBoundary>,
    pub phase: Option<u32>,
    pub phase_pair: Option<[u32; 2]>,
    pub triangle: Option<usize>,
    pub source_rows: Option<[usize; 3]>,
    pub composition: Option<[f64; 3]>,
    pub local_barycentric: Option<[f64; 3]>,
    pub level: Option<f64>,
    pub value: Option<f64>,
    pub secondary_value: Option<f64>,
    pub bracket: Option<[f64; 2]>,
    pub residual: Option<f64>,
    pub iteration: Option<usize>,
    pub path_id: Option<usize>,
    pub node_id: Option<usize>,
    pub pending_end_id: Option<usize>,
    pub counts: Option<TraceCounts>,
    pub reason: Option<String>,
}

/// Structured typed event payload.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "numerical-trace-serde", serde(rename_all = "snake_case"))]
pub enum NumericalTracePayload {
    RunStarted(TraceRunStarted),
    RunCompleted(TraceRunCompleted),
    RunFailed(TraceRunFailed),
    TraceTruncated {
        configured_limit: usize,
        events_emitted: u64,
        events_omitted_when_detected: u64,
    },
    Decision {
        kind: NumericalTraceEventKind,
        detail: TraceDecision,
    },
}

impl NumericalTracePayload {
    /// Event vocabulary name.
    pub const fn kind(&self) -> NumericalTraceEventKind {
        match self {
            Self::RunStarted(_) => NumericalTraceEventKind::RunStarted,
            Self::RunCompleted(_) => NumericalTraceEventKind::RunCompleted,
            Self::RunFailed(_) => NumericalTraceEventKind::RunFailed,
            Self::TraceTruncated { .. } => NumericalTraceEventKind::TraceTruncated,
            Self::Decision { kind, .. } => *kind,
        }
    }

    fn composition(&self) -> Option<[f64; 3]> {
        match self {
            Self::Decision { detail, .. } => detail.composition,
            _ => None,
        }
    }
}

/// Common deterministic event envelope.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "numerical-trace-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct NumericalTraceEvent {
    pub schema_version: u32,
    pub sequence: u64,
    pub stage: NumericalTraceStage,
    pub payload: NumericalTracePayload,
}

/// Request-local configuration.  This is deliberately separate from all
/// numerical options and document revisions.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericalTraceConfig {
    pub level: NumericalTraceLevel,
    pub maximum_events: usize,
    pub event_filter: Option<NumericalTraceEventKind>,
    pub boundary_filter: Option<TraceBinaryBoundary>,
    pub phase_filter: Option<u32>,
    pub phase_pair_filter: Option<[u32; 2]>,
    pub triangle_filter: Option<usize>,
    pub composition_region: Option<CompositionRegion>,
}

impl Default for NumericalTraceConfig {
    fn default() -> Self {
        Self {
            level: NumericalTraceLevel::Off,
            maximum_events: 0,
            event_filter: None,
            boundary_filter: None,
            phase_filter: None,
            phase_pair_filter: None,
            triangle_filter: None,
            composition_region: None,
        }
    }
}

impl NumericalTraceConfig {
    /// Default bounded developer diagnostic configuration.
    pub const fn decisions() -> Self {
        Self {
            level: NumericalTraceLevel::Decisions,
            maximum_events: 500_000,
            event_filter: None,
            boundary_filter: None,
            phase_filter: None,
            phase_pair_filter: None,
            triangle_filter: None,
            composition_region: None,
        }
    }
}

/// Consumer of observation events.  `record` has no error return so an output
/// failure can never become a numerical failure.
pub trait NumericalTraceSink {
    /// Trace request configuration owned by this sink.
    fn config(&self) -> &NumericalTraceConfig;

    /// Cheap preflight for event-only work.
    fn is_enabled(&self, level: NumericalTraceLevel) -> bool {
        self.config().level.includes(level)
    }

    /// Consume one complete deterministic event.
    fn record(&mut self, event: NumericalTraceEvent);
}

/// Allocation-free sink used by ordinary non-traced entry points.
#[derive(Default)]
pub struct NoopTraceSink;

impl NumericalTraceSink for NoopTraceSink {
    fn config(&self) -> &NumericalTraceConfig {
        static CONFIG: OnceLock<NumericalTraceConfig> = OnceLock::new();
        CONFIG.get_or_init(NumericalTraceConfig::default)
    }

    fn record(&mut self, _event: NumericalTraceEvent) {}
}

/// In-memory sink suited to tests and embedding.
pub struct VecTraceSink {
    config: NumericalTraceConfig,
    events: Vec<NumericalTraceEvent>,
}

impl VecTraceSink {
    pub fn new(config: NumericalTraceConfig) -> Self {
        Self {
            config,
            events: Vec::new(),
        }
    }

    pub fn events(&self) -> &[NumericalTraceEvent] {
        &self.events
    }

    pub fn into_events(self) -> Vec<NumericalTraceEvent> {
        self.events
    }
}

impl NumericalTraceSink for VecTraceSink {
    fn config(&self) -> &NumericalTraceConfig {
        &self.config
    }

    fn record(&mut self, event: NumericalTraceEvent) {
        self.events.push(event);
    }
}

/// One calculation-local deterministic sequence allocator.
pub struct NumericalTraceSession<'a> {
    sink: &'a mut dyn NumericalTraceSink,
    next_sequence: u64,
    emitted: u64,
    truncated: bool,
    omitted: u64,
}

impl<'a> NumericalTraceSession<'a> {
    pub fn new(sink: &'a mut dyn NumericalTraceSink) -> Self {
        Self {
            sink,
            next_sequence: 0,
            emitted: 0,
            truncated: false,
            omitted: 0,
        }
    }

    pub fn is_enabled(&self, level: NumericalTraceLevel) -> bool {
        !self.truncated && self.sink.is_enabled(level)
    }

    /// Whether this session was configured for a level, even if normal event
    /// recording has been truncated. Terminal run events use this to preserve a
    /// complete lifecycle after the one required `TraceTruncated` event.
    pub fn is_configured(&self, level: NumericalTraceLevel) -> bool {
        self.sink.is_enabled(level)
    }

    /// Record a RunCompleted or RunFailed terminal event even after ordinary
    /// event collection was truncated. Observation limits never erase the
    /// lifecycle terminator.
    pub fn emit_terminal(&mut self, payload: NumericalTracePayload) {
        debug_assert!(matches!(
            payload,
            NumericalTracePayload::RunCompleted(_) | NumericalTracePayload::RunFailed(_)
        ));
        if self.sink.is_enabled(NumericalTraceLevel::Summary) && self.matches(&payload) {
            self.record_unchecked(NumericalTraceStage::Run, payload);
        }
    }

    /// The request-local observation configuration.
    pub fn config(&self) -> &NumericalTraceConfig {
        self.sink.config()
    }

    pub const fn emitted(&self) -> u64 {
        self.emitted
    }
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
    pub const fn omitted(&self) -> u64 {
        self.omitted
    }

    /// Record one event if the configured level and filters allow it.
    pub fn emit(
        &mut self,
        level: NumericalTraceLevel,
        stage: NumericalTraceStage,
        payload: NumericalTracePayload,
    ) {
        if !self.sink.is_enabled(level) || !self.matches(&payload) {
            return;
        }
        let limit = self.sink.config().maximum_events;
        if self.emitted >= u64::try_from(limit).unwrap_or(u64::MAX) {
            self.omitted += 1;
            if !self.truncated {
                self.truncated = true;
                self.record_unchecked(
                    NumericalTraceStage::Run,
                    NumericalTracePayload::TraceTruncated {
                        configured_limit: limit,
                        events_emitted: self.emitted,
                        events_omitted_when_detected: self.omitted,
                    },
                );
            }
            return;
        }
        self.record_unchecked(stage, payload);
    }

    fn matches(&self, payload: &NumericalTracePayload) -> bool {
        let config = self.sink.config();
        if let Some(kind) = config.event_filter
            && payload.kind() != kind
        {
            return false;
        }
        let NumericalTracePayload::Decision { detail, .. } = payload else {
            return true;
        };
        if let Some(boundary) = config.boundary_filter
            && detail.boundary != Some(boundary)
        {
            return false;
        }
        if let Some(phase) = config.phase_filter
            && detail.phase != Some(phase)
            && !detail.phase_pair.is_some_and(|pair| pair.contains(&phase))
        {
            return false;
        }
        if let Some(pair) = config.phase_pair_filter
            && detail.phase_pair != Some(pair)
        {
            return false;
        }
        if let Some(triangle) = config.triangle_filter
            && detail.triangle != Some(triangle)
        {
            return false;
        }
        config.composition_region.is_none_or(|region| {
            payload
                .composition()
                .is_none_or(|point| region.contains(point))
        })
    }

    fn record_unchecked(&mut self, stage: NumericalTraceStage, payload: NumericalTracePayload) {
        let event = NumericalTraceEvent {
            schema_version: NUMERICAL_TRACE_SCHEMA_VERSION,
            sequence: self.next_sequence,
            stage,
            payload,
        };
        self.next_sequence += 1;
        self.emitted += 1;
        self.sink.record(event);
    }
}

/// Construct a typed decision payload without formatting text.
pub fn decision(kind: NumericalTraceEventKind, detail: TraceDecision) -> NumericalTracePayload {
    NumericalTracePayload::Decision { kind, detail }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_is_explicit_and_sequenced() {
        let mut sink = VecTraceSink::new(NumericalTraceConfig {
            level: NumericalTraceLevel::Summary,
            maximum_events: 1,
            ..NumericalTraceConfig::default()
        });
        let mut trace = NumericalTraceSession::new(&mut sink);
        trace.emit(
            NumericalTraceLevel::Summary,
            NumericalTraceStage::Run,
            decision(
                NumericalTraceEventKind::RunStarted,
                TraceDecision::default(),
            ),
        );
        trace.emit(
            NumericalTraceLevel::Summary,
            NumericalTraceStage::Run,
            decision(
                NumericalTraceEventKind::RunCompleted,
                TraceDecision::default(),
            ),
        );
        assert_eq!(sink.events().len(), 2);
        assert_eq!(sink.events()[0].sequence, 0);
        assert_eq!(sink.events()[1].sequence, 1);
        assert!(matches!(
            sink.events()[1].payload,
            NumericalTracePayload::TraceTruncated { .. }
        ));
    }

    #[test]
    fn terminal_event_survives_truncation() {
        let mut sink = VecTraceSink::new(NumericalTraceConfig {
            level: NumericalTraceLevel::Summary,
            maximum_events: 1,
            ..NumericalTraceConfig::default()
        });
        let mut trace = NumericalTraceSession::new(&mut sink);
        trace.emit(
            NumericalTraceLevel::Summary,
            NumericalTraceStage::Run,
            decision(
                NumericalTraceEventKind::RunStarted,
                TraceDecision::default(),
            ),
        );
        trace.emit(
            NumericalTraceLevel::Summary,
            NumericalTraceStage::Run,
            decision(
                NumericalTraceEventKind::ContourLevelStarted,
                TraceDecision::default(),
            ),
        );
        trace.emit_terminal(NumericalTracePayload::RunCompleted(TraceRunCompleted {
            invariant_count: 0,
            univariant_count: 0,
            contour_path_count: 0,
            trace_events: trace.emitted(),
            truncated: trace.is_truncated(),
        }));
        assert!(matches!(
            sink.events().last().map(|event| &event.payload),
            Some(NumericalTracePayload::RunCompleted(_))
        ));
    }
    #[test]
    fn disabled_trace_has_no_events() {
        let mut sink = VecTraceSink::new(NumericalTraceConfig::default());
        let mut trace = NumericalTraceSession::new(&mut sink);
        trace.emit(
            NumericalTraceLevel::Summary,
            NumericalTraceStage::Run,
            decision(
                NumericalTraceEventKind::RunStarted,
                TraceDecision::default(),
            ),
        );
        assert!(sink.events().is_empty());
    }
}
