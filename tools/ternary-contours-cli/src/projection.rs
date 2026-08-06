use std::collections::BTreeMap;

use ternary_contours::{
    BinaryExtrapolation, CubicAlphaBuildOptions, CubicAlphaMethod, CubicBoundaryPolicy,
    CubicPartialDomainPolicy, FieldInterpolation, IrregularTernaryMesh, NoopTraceSink,
    NumericalTraceConfig, NumericalTraceEventKind, NumericalTraceLevel, NumericalTracePayload,
    NumericalTraceSession, NumericalTraceSink, NumericalTraceStage, PathRegularizationOptions,
    PreparedStablePhaseEnsemble, RegularTernaryGrid, RegularTernaryScalarField,
    StableBoundaryNetwork, StableBoundaryOptions, StableContourQuantity, StableContourSet,
    StableGridOptions, StablePhaseEvaluation, StablePhaseEvaluator, StablePhaseId,
    StablePhaseSource, StablePhaseUndefinedReason, StableScalarSource, TraceCounts, TraceDecision,
    TraceRunCompleted, TraceRunFailed, TraceRunStarted, decision,
};

use crate::{
    RegularTabulatedGrid, TabulatedField, TabulatedGrid, TabulatedTernaryDataset, TabulatedValue,
    TabulatedValueState,
};
#[cfg(feature = "inspection")]
use ternary_contours::{PartialCubicGridField, RegularTernaryPartialScalarField};

/// Source interpolation family used consistently for every participating phase.
///
/// Cubic-alpha is a regular-grid model derived from one-dimensional edge slopes;
/// its continuation policy controls ternary interior evaluation. Classified
/// undefined values use the selected partial-domain policy and never enter a
/// finite stencil or get bridged by an invented scalar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SourceInterpolation {
    /// Piecewise-affine source evaluation.
    #[default]
    Linear,
    /// Cubic-alpha source evaluation with the selected edge-slope method and
    /// ternary continuation policy.
    CubicAlpha {
        method: CubicAlphaMethod,
        continuation: BinaryExtrapolation,
    },
}

/// Canonical source-field interpolation model shared by inspection, projection
/// construction, topology tracing, regularization, and numerical tracing.
///
/// This is intentionally independent of the surrounding projection options so
/// that every caller can retain one immutable interpolation snapshot. Its
/// `Default` implementation is the only regular-grid default in the product.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterpolationOptions {
    pub source: SourceInterpolation,
    pub partial_domain_policy: CubicPartialDomainPolicy,
}

impl Default for InterpolationOptions {
    fn default() -> Self {
        Self {
            source: SourceInterpolation::CubicAlpha {
                method: CubicAlphaMethod::Akima,
                continuation: BinaryExtrapolation::Muggianu,
            },
            partial_domain_policy: CubicPartialDomainPolicy::OneSidedThenLinear,
        }
    }
}

impl InterpolationOptions {
    pub const fn cubic_options(self) -> Option<CubicAlphaBuildOptions> {
        match self.source {
            SourceInterpolation::Linear => None,
            SourceInterpolation::CubicAlpha {
                method,
                continuation,
            } => Some(CubicAlphaBuildOptions {
                method,
                boundary_policy: CubicBoundaryPolicy::LinearFallback,
                extrapolation: continuation,
                partial_domain_policy: self.partial_domain_policy,
            }),
        }
    }
}

/// Controls for a stable liquidus calculation.
#[derive(Clone, Debug, Default)]
pub struct ProjectionOptions {
    pub levels: Vec<f64>,
    /// When present, derive levels from stable topology: the lowest finite
    /// invariant temperature (or the calculated-source minimum) through the
    /// calculated-source maximum at this positive finite step.
    pub automatic_level_step: Option<f64>,
    pub sampling_subdivisions: Option<usize>,
    pub regularize: bool,
    pub regularization_spacing: Option<f64>,
    pub interpolation: InterpolationOptions,
}

/// Authoritative range used by automatic Viewer isotherms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutomaticIsoRange {
    pub minimum: f64,
    pub maximum: f64,
    pub used_invariant_minimum: bool,
}

#[derive(Clone, Debug)]
pub struct InputSummary {
    pub phase_count: usize,
    pub regular_grid_count: usize,
    pub irregular_grid_count: usize,
    pub temperature_range: [f64; 2],
}

#[derive(Clone, Debug)]
pub struct ProjectionDiagnostics {
    pub sampling_subdivisions: usize,
    pub regularized: bool,
    /// True when this request reused an accepted stable-boundary network and
    /// rebuilt only levels and stable contour paths.
    pub stable_topology_reused: bool,
    pub contour_path_count: usize,
    pub invariant_count: usize,
    pub stable_polygon_count: usize,
    pub univariant_count: usize,
    /// Equality branches retained as typed source-domain truncation diagnostics.
    pub domain_truncated_univariant_count: usize,
    /// Raw univariants retained after their optional regularization failed.
    pub regularization_failure_count: usize,
    /// Local interpolation summaries for partial regular cubic-alpha fields.
    pub partial_cubic_summaries: Vec<String>,
    /// Finite EX source cells accepted by this projection request.
    pub extrapolated_source_values_used: usize,
    /// Largest EX layer accepted by this projection request.
    pub maximum_extrapolation_layer_used: Option<u16>,
    /// Distinct EX methods used by accepted source cells in deterministic order.
    pub extrapolation_methods_used: Vec<String>,
}

/// Numerical output shared by terminal reporting and static renderers.
#[derive(Clone, Debug)]
pub struct LiquidusProjection {
    pub levels: Vec<f64>,
    /// Present only when levels were generated from automatic Viewer range.
    pub automatic_iso_range: Option<AutomaticIsoRange>,
    pub stable_contours: StableContourSet,
    pub stable_boundaries: StableBoundaryNetwork,
    pub input_summary: InputSummary,
    pub diagnostics: ProjectionDiagnostics,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("phase `{phase}` has no temperature field")]
    MissingTemperature { phase: String },
    #[error("could not prepare irregular field for phase {phase:?}: {message}")]
    IrregularMesh {
        phase: StablePhaseId,
        message: String,
    },
    #[error("no calculated finite temperature samples are available: {details}")]
    NoCalculatedTemperatureSamples { details: String },
    #[error("invalid projection levels: {0}")]
    Levels(String),
    #[error(
        "no contour paths were produced because the requested levels do not intersect the calculated temperature range: {minimum} to {maximum} {unit}"
    )]
    NoContourPaths {
        minimum: f64,
        maximum: f64,
        unit: String,
    },
    #[error("stable liquidus preparation failed: {error}; tabulated source coverage: {details}")]
    Preparation {
        error: ternary_contours::StableContourError,
        details: String,
    },
    #[error("stable boundary calculation failed: {0}")]
    Boundaries(#[from] ternary_contours::StableBoundaryError),
    #[error(
        "cubic-alpha cannot safely prepare grid `{grid}` field `{phase}.{property}`: calculated {calculated}, extrapolated {extrapolated}, cut-off {cut_off}, missing {missing}"
    )]
    CubicSourceIncomplete {
        grid: String,
        phase: String,
        property: String,
        calculated: usize,
        extrapolated: usize,
        cut_off: usize,
        missing: usize,
    },
    #[error(
        "cubic-alpha source interpolation is unavailable for irregular grid `{grid}` field `{phase}.{property}` in this viewer build; use Linear Delaunay"
    )]
    CubicIrregularUnavailable {
        grid: String,
        phase: String,
        property: String,
    },
    #[error(
        "could not construct cubic-alpha source for grid `{grid}` field `{phase}.{property}`: {message}"
    )]
    CubicSourceConstruction {
        grid: String,
        phase: String,
        property: String,
        message: String,
    },
}

enum RuntimeField {
    Regular {
        grid: RegularTernaryGrid,
        values: Vec<TabulatedValue>,
    },
    Irregular {
        mesh: Box<IrregularTernaryMesh>,
        values: Vec<TabulatedValue>,
    },
}

struct RuntimePhase {
    field: RuntimeField,
}

enum PhaseSourceModel {
    Evaluator(RuntimePhase),
    CubicRegular(RegularTernaryScalarField),
    #[cfg(feature = "inspection")]
    CubicPartial(RegularTernaryPartialScalarField),
}

impl StablePhaseEvaluator for RuntimePhase {
    fn evaluate(&self, composition: [f64; 3]) -> StablePhaseEvaluation {
        let result = match &self.field {
            RuntimeField::Regular { grid, values } => grid
                .locate(composition)
                .map_err(|_| StablePhaseUndefinedReason::OutsidePhaseDomain)
                .and_then(|location| {
                    interpolate_tabulated(
                        values,
                        location
                            .triangle
                            .vertices
                            .into_iter()
                            .zip(location.barycentric)
                            .map(|(vertex, weight)| (vertex.0, weight)),
                    )
                }),
            RuntimeField::Irregular { mesh, values } => mesh
                .locate(composition)
                .map_err(|_| StablePhaseUndefinedReason::OutsidePhaseDomain)
                .and_then(|location| {
                    interpolate_tabulated(
                        values,
                        location
                            .triangle
                            .vertices
                            .into_iter()
                            .zip(location.barycentric)
                            .map(|(vertex, weight)| (vertex.0, weight)),
                    )
                }),
        };
        match result {
            Ok(value) if value.is_finite() => StablePhaseEvaluation::Defined { value },
            Ok(_) => StablePhaseEvaluation::Undefined {
                reason: StablePhaseUndefinedReason::NonFiniteResult,
            },
            Err(reason) => StablePhaseEvaluation::Undefined { reason },
        }
    }
}

/// Evaluate a regular tabulated field through the exact linear source adapter
/// used by `calculate_projection`. Diagnostic tools use this to avoid a
/// second interpolation implementation.
pub(crate) fn evaluate_regular_linear_field(
    grid: &RegularTabulatedGrid,
    field: &TabulatedField,
    composition: [f64; 3],
) -> StablePhaseEvaluation {
    RuntimePhase {
        field: RuntimeField::Regular {
            grid: RegularTernaryGrid::new(grid.subdivisions)
                .expect("a parsed regular grid has positive subdivisions"),
            values: field.values.clone(),
        },
    }
    .evaluate(composition)
}

pub(crate) fn interpolate_tabulated(
    values: &[TabulatedValue],
    vertices: impl IntoIterator<Item = (usize, f64)>,
) -> Result<f64, StablePhaseUndefinedReason> {
    vertices
        .into_iter()
        .try_fold(0.0, |interpolated, (index, weight)| {
            let sample = values
                .get(index)
                .ok_or(StablePhaseUndefinedReason::MissingTabulatedInput)?;
            let value = sample
                .defined_value()
                .ok_or_else(|| undefined_reason(sample))?;
            Ok(interpolated + weight * value)
        })
}

pub(crate) fn undefined_reason(value: &TabulatedValue) -> StablePhaseUndefinedReason {
    match value.state {
        TabulatedValueState::Calculated | TabulatedValueState::Extrapolated => {
            StablePhaseUndefinedReason::NonFiniteResult
        }
        TabulatedValueState::CutOff => StablePhaseUndefinedReason::ClassifiedCutOff,
        TabulatedValueState::Missing => StablePhaseUndefinedReason::MissingTabulatedInput,
    }
}

/// Request metadata included in a trace envelope without participating in
/// numerical option equality, caching, or document state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NumericalTraceRunContext {
    pub input_identifier: Option<String>,
    pub input_content_hash: Option<String>,
    pub dataset_revision: Option<u64>,
    pub options_revision: Option<u64>,
    pub request_id: Option<u64>,
}

/// Convert parsed temperature fields to explicit partial-domain evaluators, then
/// calculate stable isotherms and the boundary-connected univariant network.
pub fn calculate_projection(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
) -> Result<LiquidusProjection, ProjectionError> {
    let mut sink = NoopTraceSink;
    calculate_projection_with_trace(dataset, options, &mut sink)
}

/// Calculate a liquidus projection while emitting optional deterministic,
/// observation-only numerical trace events.
pub fn calculate_projection_with_trace(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
    sink: &mut impl NumericalTraceSink,
) -> Result<LiquidusProjection, ProjectionError> {
    calculate_projection_with_trace_context(
        dataset,
        options,
        sink,
        &NumericalTraceRunContext::default(),
    )
}

/// Calculate a projection with deterministic trace events and request-local
/// metadata supplied by the caller. The context is observation-only.
pub fn calculate_projection_with_trace_context(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
    sink: &mut impl NumericalTraceSink,
    context: &NumericalTraceRunContext,
) -> Result<LiquidusProjection, ProjectionError> {
    calculate_projection_with_trace_context_reusing_stable_topology(
        dataset, options, sink, context, None,
    )
}

/// Trace-aware form of isotherm-only recalculation. The trace observes the
/// accepted request itself and records topology reuse rather than forcing an
/// unrelated full stable-boundary rebuild merely to obtain trace output.
pub fn calculate_projection_with_trace_context_reusing_stable_topology(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
    sink: &mut impl NumericalTraceSink,
    context: &NumericalTraceRunContext,
    stable_topology: Option<&StableBoundaryNetwork>,
) -> Result<LiquidusProjection, ProjectionError> {
    let mut trace = NumericalTraceSession::new(sink);
    if trace.is_enabled(NumericalTraceLevel::Summary) {
        trace.emit(
            NumericalTraceLevel::Summary,
            NumericalTraceStage::Run,
            NumericalTracePayload::RunStarted(trace_run_started(
                dataset,
                options,
                trace.config(),
                context,
            )),
        );
    }
    let result = calculate_projection_with_trace_session_reusing_topology(
        dataset,
        options,
        &mut trace,
        stable_topology,
    );
    match &result {
        Ok(projection) if trace.is_configured(NumericalTraceLevel::Summary) => {
            trace.emit_terminal(NumericalTracePayload::RunCompleted(TraceRunCompleted {
                invariant_count: projection.diagnostics.invariant_count,
                univariant_count: projection.diagnostics.univariant_count,
                contour_path_count: projection.diagnostics.contour_path_count,
                trace_events: trace.emitted(),
                truncated: trace.is_truncated(),
            }));
        }
        Err(error) if trace.is_configured(NumericalTraceLevel::Summary) => {
            trace.emit_terminal(NumericalTracePayload::RunFailed(TraceRunFailed {
                error_kind: NumericalTraceEventKind::InvalidOptions,
                message: error.to_string(),
            }));
        }
        _ => {}
    }
    result
}

/// Recalculate only stable isotherms while retaining an already accepted
/// stable-boundary network. The caller must use this only when the dataset and
/// every topology-affecting option are unchanged.
pub fn calculate_projection_reusing_stable_topology(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
    stable_topology: &StableBoundaryNetwork,
) -> Result<LiquidusProjection, ProjectionError> {
    let mut sink = NoopTraceSink;
    let mut trace = NumericalTraceSession::new(&mut sink);
    calculate_projection_with_trace_session_reusing_topology(
        dataset,
        options,
        &mut trace,
        Some(stable_topology),
    )
}

fn calculate_projection_with_trace_session_reusing_topology(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
    trace: &mut NumericalTraceSession<'_>,
    reused_topology: Option<&StableBoundaryNetwork>,
) -> Result<LiquidusProjection, ProjectionError> {
    let mut fields = BTreeMap::<StablePhaseId, PhaseSourceModel>::new();
    let mut regular_grid_count = 0;
    let mut irregular_grid_count = 0;
    let mut extrema = Vec::new();
    let mut unavailable_fields = Vec::new();
    let mut source_coverage = Vec::new();
    let mut extrapolated_source_values_used = 0usize;
    let mut maximum_extrapolation_layer_used = None;
    let mut extrapolation_methods_used = std::collections::BTreeSet::new();
    #[allow(unused_mut)]
    let mut partial_cubic_summaries = Vec::new();
    for grid in &dataset.grids {
        match grid {
            TabulatedGrid::Regular(_) => regular_grid_count += 1,
            TabulatedGrid::Irregular(_) => irregular_grid_count += 1,
        }
        for field in grid.fields() {
            if field.property != "T" {
                continue;
            }
            let calculated = field
                .values
                .iter()
                .filter_map(TabulatedValue::defined_value)
                .collect::<Vec<_>>();
            let counts = tabulated_state_counts(&field.values);
            for value in &field.values {
                if let Some(metadata) = value.extrapolation.as_ref() {
                    extrapolated_source_values_used += 1;
                    maximum_extrapolation_layer_used = Some(
                        maximum_extrapolation_layer_used
                            .map_or(metadata.layer, |maximum: u16| maximum.max(metadata.layer)),
                    );
                    extrapolation_methods_used.insert(format!("{:?}", metadata.method));
                }
            }
            if trace.is_enabled(NumericalTraceLevel::Decisions) {
                trace.emit(
                    NumericalTraceLevel::Decisions,
                    NumericalTraceStage::SourcePreparation,
                    decision(
                        NumericalTraceEventKind::PhaseFieldLocated,
                        TraceDecision {
                            phase: Some(field.phase_id.0),
                            counts: Some(TraceCounts {
                                calculated: counts[0],
                                extrapolated: counts[1],
                                non_existing: 0,
                                cut_off: counts[2],
                                missing: counts[3],
                            }),
                            reason: Some(format!(
                                "grid={}, property={}, geometry={}",
                                grid.name(),
                                field.property,
                                match grid {
                                    TabulatedGrid::Regular(_) => "regular",
                                    TabulatedGrid::Irregular(_) => "irregular",
                                },
                            )),
                            ..TraceDecision::default()
                        },
                    ),
                );
            }
            let coverage = format!(
                "grid {} phase {}.{} (calculated {}, extrapolated {}, cut-off {}, missing {})",
                grid.name(),
                field.phase_id.0,
                field.property,
                counts[0],
                counts[1],
                counts[2],
                counts[3]
            );
            if calculated.is_empty() {
                unavailable_fields.push(coverage.clone());
            }
            source_coverage.push(coverage);
            extrema.extend(calculated);
            let phase = dataset
                .phases
                .iter()
                .find(|phase| phase.id == field.phase_id)
                .map(|phase| phase.name.clone())
                .unwrap_or_else(|| format!("Phase {}", field.phase_id.0));
            let source = match (options.interpolation.source, grid) {
                (SourceInterpolation::Linear, TabulatedGrid::Regular(grid)) => {
                    PhaseSourceModel::Evaluator(RuntimePhase {
                        field: RuntimeField::Regular {
                            grid: RegularTernaryGrid::new(grid.subdivisions)
                                .expect("parser validated positive subdivisions"),
                            values: field.values.clone(),
                        },
                    })
                }
                (SourceInterpolation::Linear, TabulatedGrid::Irregular(grid)) => {
                    PhaseSourceModel::Evaluator(RuntimePhase {
                        field: RuntimeField::Irregular {
                            mesh: Box::new(
                                IrregularTernaryMesh::new(grid.compositions.iter().copied())
                                    .map_err(|error| ProjectionError::IrregularMesh {
                                        phase: field.phase_id,
                                        message: error.to_string(),
                                    })?,
                            ),
                            values: field.values.clone(),
                        },
                    })
                }
                (SourceInterpolation::CubicAlpha { .. }, TabulatedGrid::Regular(grid)) => {
                    let values = field
                        .values
                        .iter()
                        .map(TabulatedValue::defined_value)
                        .collect::<Vec<_>>();
                    if options.interpolation.partial_domain_policy
                        == CubicPartialDomainPolicy::Strict
                        && values.iter().any(Option::is_none)
                    {
                        return Err(ProjectionError::CubicSourceIncomplete {
                            grid: grid.name.clone(),
                            phase: phase.clone(),
                            property: field.property.clone(),
                            calculated: counts[0],
                            extrapolated: counts[1],
                            cut_off: counts[2],
                            missing: counts[3],
                        });
                    }
                    if values.iter().all(Option::is_some) {
                        let complete = values.into_iter().map(Option::unwrap).collect::<Vec<_>>();
                        let cubic = RegularTernaryScalarField::new(grid.subdivisions, complete)
                            .map_err(|error| ProjectionError::CubicSourceConstruction {
                                grid: grid.name.clone(),
                                phase: phase.clone(),
                                property: field.property.clone(),
                                message: error.to_string(),
                            })?;
                        PhaseSourceModel::CubicRegular(cubic)
                    } else {
                        #[cfg(feature = "inspection")]
                        {
                            let partial =
                                RegularTernaryPartialScalarField::new(grid.subdivisions, values)
                                    .map_err(|error| ProjectionError::CubicSourceConstruction {
                                        grid: grid.name.clone(),
                                        phase: phase.clone(),
                                        property: field.property.clone(),
                                        message: error.to_string(),
                                    })?;
                            let cubic_options = options
                                .interpolation
                                .cubic_options()
                                .expect("cubic source model carries cubic options");
                            let preview =
                                PartialCubicGridField::new(partial.clone(), cubic_options)
                                    .map_err(|error| ProjectionError::CubicSourceConstruction {
                                        grid: grid.name.clone(),
                                        phase: phase.clone(),
                                        property: field.property.clone(),
                                        message: error.to_string(),
                                    })?;
                            let diagnostics = preview.diagnostics();
                            partial_cubic_summaries.push(format!(
                                "{} {}.{}: cubic triangles {}, one-sided cubic triangles {}, linear fallback triangles {}, undefined triangles {}",
                                grid.name,
                                phase,
                                field.property,
                                diagnostics.cubic_triangles,
                                diagnostics.one_sided_cubic_triangles,
                                diagnostics.linear_fallback_triangles,
                                diagnostics.undefined_triangles,
                            ));
                            PhaseSourceModel::CubicPartial(partial)
                        }
                        #[cfg(not(feature = "inspection"))]
                        {
                            let _ = values;
                            return Err(ProjectionError::CubicSourceIncomplete {
                                grid: grid.name.clone(),
                                phase: phase.clone(),
                                property: field.property.clone(),
                                calculated: counts[0],
                                extrapolated: counts[1],
                                cut_off: counts[2],
                                missing: counts[3],
                            });
                        }
                    }
                }
                (SourceInterpolation::CubicAlpha { .. }, TabulatedGrid::Irregular(grid)) => {
                    return Err(ProjectionError::CubicIrregularUnavailable {
                        grid: grid.name.clone(),
                        phase,
                        property: field.property.clone(),
                    });
                }
            };
            fields.insert(field.phase_id, source);
        }
    }
    let minimum = extrema.iter().copied().reduce(f64::min).ok_or_else(|| {
        ProjectionError::NoCalculatedTemperatureSamples {
            details: unavailable_fields.join("; "),
        }
    })?;
    let maximum = extrema
        .iter()
        .copied()
        .reduce(f64::max)
        .expect("minimum is present");

    let source_models = dataset
        .phases
        .iter()
        .map(|phase| {
            fields
                .remove(&phase.id)
                .ok_or_else(|| ProjectionError::MissingTemperature {
                    phase: phase.name.clone(),
                })
        })
        .collect::<Result<Vec<_>, ProjectionError>>()?;
    let sources = dataset
        .phases
        .iter()
        .zip(&source_models)
        .map(|(phase, model)| {
            let source = match model {
                PhaseSourceModel::Evaluator(evaluator) => StableScalarSource::evaluator(evaluator),
                PhaseSourceModel::CubicRegular(field) => {
                    let cubic_options = options
                        .interpolation
                        .cubic_options()
                        .expect("cubic source model carries cubic options");
                    StableScalarSource::regular(
                        field,
                        FieldInterpolation::CubicAlpha(cubic_options),
                    )
                }
                #[cfg(feature = "inspection")]
                PhaseSourceModel::CubicPartial(field) => {
                    let cubic_options = options
                        .interpolation
                        .cubic_options()
                        .expect("cubic source model carries cubic options");
                    StableScalarSource::partial_regular(
                        field,
                        FieldInterpolation::CubicAlpha(cubic_options),
                    )
                }
            };
            StablePhaseSource::new(phase.id, source)
        })
        .collect::<Vec<_>>();
    let sampling_subdivisions = options.sampling_subdivisions.unwrap_or_else(|| {
        dataset
            .grids
            .iter()
            .filter_map(|grid| match grid {
                TabulatedGrid::Regular(grid) => Some(grid.subdivisions),
                TabulatedGrid::Irregular(_) => None,
            })
            .max()
            .unwrap_or(24)
            .max(2)
    });
    if sampling_subdivisions == 0 {
        return Err(ProjectionError::Levels(
            "sampling subdivisions must be positive".into(),
        ));
    }
    if let Some(spacing) = options.regularization_spacing
        && (!spacing.is_finite() || spacing <= 0.0)
    {
        return Err(ProjectionError::Levels(
            "regularization spacing must be finite and positive".into(),
        ));
    }
    let prepared = PreparedStablePhaseEnsemble::new_with_trace_session(
        sources,
        StableContourQuantity::Height,
        StableGridOptions {
            subdivisions: sampling_subdivisions,
            ..StableGridOptions::default()
        },
        trace,
    )
    .map_err(|error| ProjectionError::Preparation {
        error,
        details: source_coverage.join("; "),
    })?;
    // Stable topology is independent of requested isotherm levels. The
    // reuse path reconstructs source evaluators for contour extraction but
    // never rescans binary boundaries, refines invariants, traces
    // univariants, or regularizes paths.
    let stable_boundaries = if let Some(network) = reused_topology {
        if trace.is_enabled(NumericalTraceLevel::Summary) {
            trace.emit(
                NumericalTraceLevel::Summary,
                NumericalTraceStage::StableSelection,
                decision(
                    NumericalTraceEventKind::StableTopologyReused,
                    TraceDecision {
                        reason: Some("stable topology reused for isotherm-only request".into()),
                        ..TraceDecision::default()
                    },
                ),
            );
        }
        network.clone()
    } else {
        if trace.is_enabled(NumericalTraceLevel::Summary) {
            trace.emit(
                NumericalTraceLevel::Summary,
                NumericalTraceStage::StableSelection,
                decision(
                    NumericalTraceEventKind::StableTopologyBuilt,
                    TraceDecision {
                        reason: Some("stable topology rebuilt from current sources".into()),
                        ..TraceDecision::default()
                    },
                ),
            );
        }
        prepared.stable_boundaries_with_trace_session(
            StableBoundaryOptions {
                regularization: options.regularize.then_some(PathRegularizationOptions {
                    spacing: options.regularization_spacing.unwrap_or(0.02),
                    protected_endpoint_distance: 0.0,
                    ..PathRegularizationOptions::default()
                }),
                ..StableBoundaryOptions::default()
            },
            trace,
        )?
    };
    let automatic_range = options
        .automatic_level_step
        .map(|step| automatic_iso_range(&stable_boundaries, minimum, maximum, step))
        .transpose()?;
    let levels = match automatic_range {
        Some(range) => automatic_iso_levels(
            range.minimum,
            range.maximum,
            options
                .automatic_level_step
                .expect("automatic range carries a step"),
        )?,
        None if options.levels.is_empty() => default_levels(minimum, maximum),
        None => {
            validate_levels(&options.levels)?;
            options.levels.clone()
        }
    };
    if trace.is_enabled(NumericalTraceLevel::Summary) {
        trace.emit(
            NumericalTraceLevel::Summary,
            NumericalTraceStage::Contour,
            decision(
                NumericalTraceEventKind::StableIsothermsRebuilt,
                TraceDecision {
                    reason: Some(format!("{} stable isotherm levels", levels.len())),
                    ..TraceDecision::default()
                },
            ),
        );
    }
    let stable_contours = prepared
        .contours_with_trace_session(&levels, trace)
        .map_err(|error| ProjectionError::Preparation {
            error,
            details: source_coverage.join("; "),
        })?;
    if stable_contours
        .levels
        .iter()
        .all(|level| level.paths.is_empty())
        && levels
            .iter()
            .all(|level| *level < minimum || *level > maximum)
    {
        return Err(ProjectionError::NoContourPaths {
            minimum,
            maximum,
            unit: dataset
                .property("T")
                .map(|property| property.unit.clone())
                .unwrap_or_default(),
        });
    }
    let diagnostics = ProjectionDiagnostics {
        sampling_subdivisions,
        regularized: options.regularize,
        stable_topology_reused: reused_topology.is_some(),
        contour_path_count: stable_contours
            .levels
            .iter()
            .map(|level| level.paths.len())
            .sum(),
        stable_polygon_count: stable_contours.diagnostics.nonempty_stable_polygons,
        invariant_count: stable_boundaries.nodes.len(),
        univariant_count: stable_boundaries.univariants.len(),
        domain_truncated_univariant_count: stable_boundaries.truncated_univariants.len(),
        regularization_failure_count: stable_boundaries.regularization_failures.len(),
        partial_cubic_summaries,
        extrapolated_source_values_used,
        maximum_extrapolation_layer_used,
        extrapolation_methods_used: extrapolation_methods_used.into_iter().collect(),
    };
    Ok(LiquidusProjection {
        levels,
        automatic_iso_range: automatic_range,
        stable_contours,
        stable_boundaries,
        input_summary: InputSummary {
            phase_count: dataset.phases.len(),
            regular_grid_count,
            irregular_grid_count,
            temperature_range: [minimum, maximum],
        },
        diagnostics,
    })
}

fn trace_run_started(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
    trace_config: &NumericalTraceConfig,
    context: &NumericalTraceRunContext,
) -> TraceRunStarted {
    let property = dataset.property("T");
    TraceRunStarted {
        crate_version: env!("CARGO_PKG_VERSION").into(),
        git_commit: option_env!("VERGEN_GIT_SHA").map(str::to_owned),
        calculation_kind: "stable_liquidus_projection".into(),
        input_identifier: context.input_identifier.clone().or_else(|| {
            dataset
                .source_path
                .as_ref()
                .map(|path| path.display().to_string())
        }),
        input_content_hash: context.input_content_hash.clone(),
        components: dataset.components.clone().map(|component| component.name),
        phase_ids: dataset.phases.iter().map(|phase| phase.id.0).collect(),
        phase_names: dataset
            .phases
            .iter()
            .map(|phase| phase.name.clone())
            .collect(),
        property: "T".into(),
        unit: property
            .map(|property| property.unit.clone())
            .unwrap_or_default(),
        sampling_subdivisions: options.sampling_subdivisions,
        interpolation: format!("{:?}", options.interpolation.source),
        partial_domain_policy: format!("{:?}", options.interpolation.partial_domain_policy),
        continuation: match options.interpolation.source {
            SourceInterpolation::Linear => "not_applicable".into(),
            SourceInterpolation::CubicAlpha { continuation, .. } => format!("{continuation:?}"),
        },
        regularization: options.regularize,
        requested_levels: options.levels.clone(),
        dataset_revision: context.dataset_revision,
        options_revision: context.options_revision,
        request_id: context.request_id,
        trace_level: trace_config.level,
        trace_maximum_events: trace_config.maximum_events,
    }
}
fn tabulated_state_counts(values: &[TabulatedValue]) -> [usize; 4] {
    values.iter().fold([0usize; 4], |mut counts, value| {
        counts[match value.state {
            TabulatedValueState::Calculated => 0,
            TabulatedValueState::Extrapolated => 1,
            TabulatedValueState::CutOff => 2,
            TabulatedValueState::Missing => 3,
        }] += 1;
        counts
    })
}

/// Derive the Viewer automatic range from stable invariant topology and finite
/// calculated source extrema. The source minimum is used only when topology
/// did not produce a finite invariant temperature.
pub fn automatic_iso_range(
    boundaries: &StableBoundaryNetwork,
    source_minimum: f64,
    source_maximum: f64,
    step: f64,
) -> Result<AutomaticIsoRange, ProjectionError> {
    if !source_minimum.is_finite() || !source_maximum.is_finite() || source_maximum < source_minimum
    {
        return Err(ProjectionError::Levels(
            "automatic isotherm range requires finite calculated source extrema".into(),
        ));
    }
    if !step.is_finite() || step <= 0.0 {
        return Err(ProjectionError::Levels(
            "automatic isotherm step must be finite and positive".into(),
        ));
    }
    let invariant_minimum = boundaries
        .nodes
        .iter()
        .map(|node| node.temperature())
        .filter(|temperature| temperature.is_finite())
        .reduce(f64::min);
    Ok(AutomaticIsoRange {
        minimum: invariant_minimum.unwrap_or(source_minimum),
        maximum: source_maximum,
        used_invariant_minimum: invariant_minimum.is_some(),
    })
}

/// Generate deterministic automatic levels beginning at the exact calculated
/// minimum. The final value never exceeds the calculated maximum.
pub fn automatic_iso_levels(
    minimum: f64,
    maximum: f64,
    step: f64,
) -> Result<Vec<f64>, ProjectionError> {
    if !minimum.is_finite() || !maximum.is_finite() || maximum < minimum {
        return Err(ProjectionError::Levels(
            "automatic isotherm range must be finite with Tmax >= Tmin".into(),
        ));
    }
    if !step.is_finite() || step <= 0.0 {
        return Err(ProjectionError::Levels(
            "automatic isotherm step must be finite and positive".into(),
        ));
    }
    let mut levels = Vec::new();
    let mut level = minimum;
    while level <= maximum + step.abs() * 1.0e-12 {
        levels.push(level.min(maximum));
        if levels.len() > 10_000 {
            return Err(ProjectionError::Levels(
                "automatic isotherm range has too many levels".into(),
            ));
        }
        level += step;
    }
    levels.dedup_by(|left, right| (*left - *right).abs() <= step.abs() * 1.0e-12);
    validate_levels(&levels)?;
    Ok(levels)
}
fn default_levels(minimum: f64, maximum: f64) -> Vec<f64> {
    if (maximum - minimum).abs() <= f64::EPSILON {
        vec![minimum]
    } else {
        (1..=5)
            .map(|index| minimum + (maximum - minimum) * index as f64 / 6.0)
            .collect()
    }
}

fn validate_levels(levels: &[f64]) -> Result<(), ProjectionError> {
    if levels.is_empty() || levels.iter().any(|level| !level.is_finite()) {
        return Err(ProjectionError::Levels(
            "levels must be a non-empty finite list".into(),
        ));
    }
    if levels.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ProjectionError::Levels(
            "levels must be strictly ascending without duplicates".into(),
        ));
    }
    Ok(())
}

/// Parse `800,900,1000` or `800:1400:50` into strictly ascending values.
pub fn parse_level_spec(value: &str) -> Result<Vec<f64>, ProjectionError> {
    if let Some((start, rest)) = value.split_once(':') {
        let (end, step) = rest
            .split_once(':')
            .ok_or_else(|| ProjectionError::Levels("range levels use `start:end:step`".into()))?;
        let start = parse_level(start)?;
        let end = parse_level(end)?;
        let step = parse_level(step)?;
        if step <= 0.0 || end < start {
            return Err(ProjectionError::Levels(
                "range step must be positive and end must not precede start".into(),
            ));
        }
        let mut values = Vec::new();
        let mut next = start;
        while next <= end + step * 1.0e-12 {
            values.push(next.min(end));
            next += step;
            if values.len() > 100_000 {
                return Err(ProjectionError::Levels(
                    "level range has too many entries".into(),
                ));
            }
        }
        validate_levels(&values)?;
        Ok(values)
    } else {
        let values = value
            .split(',')
            .map(parse_level)
            .collect::<Result<Vec<_>, _>>()?;
        validate_levels(&values)?;
        Ok(values)
    }
}

fn parse_level(value: &str) -> Result<f64, ProjectionError> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| ProjectionError::Levels(format!("`{value}` is not a finite number")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn canonical_regular_interpolation_default_is_cubic_akima_muggianu_with_local_fallback() {
        let interpolation = InterpolationOptions::default();
        assert!(matches!(
            interpolation.source,
            SourceInterpolation::CubicAlpha {
                method: CubicAlphaMethod::Akima,
                continuation: BinaryExtrapolation::Muggianu,
            }
        ));
        assert_eq!(
            interpolation.partial_domain_policy,
            CubicPartialDomainPolicy::OneSidedThenLinear
        );
        assert_eq!(ProjectionOptions::default().interpolation, interpolation);
    }

    use super::*;
    use crate::parse_str;

    #[test]
    fn partial_phase_domains_are_supported_without_global_rejection() {
        let dataset = parse_str(include_str!("../fixtures/partial-phase-domain.tct")).unwrap();
        let options = ProjectionOptions {
            interpolation: InterpolationOptions {
                source: SourceInterpolation::Linear,
                ..InterpolationOptions::default()
            },
            ..ProjectionOptions::default()
        };
        let projection = calculate_projection(&dataset, &options).unwrap();
        assert_eq!(projection.input_summary.phase_count, 2);
        assert!(projection.input_summary.temperature_range[0].is_finite());
    }

    #[test]
    fn all_declared_temperature_fields_are_in_the_stable_ensemble() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let projection = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
        assert_eq!(projection.input_summary.phase_count, 3);
        assert!(projection.diagnostics.stable_polygon_count > 0);
    }

    #[test]
    fn classified_missing_coverage_reports_each_temperature_field() {
        let error = calculate_projection(
            &crate::default_regular_dataset(),
            &ProjectionOptions::default(),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(matches!(
            error,
            ProjectionError::NoCalculatedTemperatureSamples { .. }
        ));
        assert!(message.contains("grid regular phase 1.T"));
        assert!(message.contains("extrapolated 0, cut-off 0, missing 66"));
    }

    #[cfg(feature = "inspection")]
    #[test]
    fn cubic_alpha_selection_reaches_every_regular_phase_source() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let projection = calculate_projection(
            &dataset,
            &ProjectionOptions {
                interpolation: InterpolationOptions {
                    source: SourceInterpolation::CubicAlpha {
                        method: CubicAlphaMethod::Makima,
                        continuation: BinaryExtrapolation::Kohler,
                    },
                    ..InterpolationOptions::default()
                },
                ..ProjectionOptions::default()
            },
        )
        .unwrap();
        assert_eq!(projection.input_summary.phase_count, dataset.phases.len());
        assert!(projection.diagnostics.stable_polygon_count > 0);
    }

    #[cfg(feature = "inspection")]
    #[test]
    fn cubic_option_matrix_uses_the_same_complete_projection_pipeline() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        for method in [
            CubicAlphaMethod::Akima,
            CubicAlphaMethod::Makima,
            CubicAlphaMethod::Pchip,
            CubicAlphaMethod::Steffen,
        ] {
            for continuation in [
                BinaryExtrapolation::RawBarycentric,
                BinaryExtrapolation::Muggianu,
                BinaryExtrapolation::Kohler,
            ] {
                for partial_domain_policy in [
                    CubicPartialDomainPolicy::Strict,
                    CubicPartialDomainPolicy::OneSided,
                    CubicPartialDomainPolicy::OneSidedThenLinear,
                    CubicPartialDomainPolicy::LinearNearDomain,
                ] {
                    let projection = calculate_projection(
                        &dataset,
                        &ProjectionOptions {
                            automatic_level_step: Some(10.0),
                            sampling_subdivisions: Some(12),
                            interpolation: InterpolationOptions {
                                source: SourceInterpolation::CubicAlpha {
                                    method,
                                    continuation,
                                },
                                partial_domain_policy,
                            },
                            ..ProjectionOptions::default()
                        },
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "{method:?}/{continuation:?}/{partial_domain_policy:?} did not reach the projection pipeline: {error}"
                        )
                    });
                    assert_eq!(projection.input_summary.phase_count, 3);
                    assert!(projection.diagnostics.stable_polygon_count > 0);
                    assert!(!projection.levels.is_empty());
                }
            }
        }
    }
    #[cfg(feature = "inspection")]
    #[test]
    fn cubic_alpha_partial_regular_domains_use_local_fallbacks() {
        let mut dataset = crate::default_regular_dataset();
        if let TabulatedGrid::Regular(grid) = &mut dataset.grids[0] {
            for field in &mut grid.fields {
                for (index, value) in field.values.iter_mut().enumerate() {
                    *value = TabulatedValue::calculated((index + field.phase_id.0 as usize) as f64)
                        .unwrap();
                }
            }
            grid.fields[0].values[0] = TabulatedValue::missing();
        }
        let projection = calculate_projection(
            &dataset,
            &ProjectionOptions {
                interpolation: InterpolationOptions {
                    source: SourceInterpolation::CubicAlpha {
                        method: CubicAlphaMethod::Akima,
                        continuation: BinaryExtrapolation::Muggianu,
                    },
                    ..InterpolationOptions::default()
                },
                ..ProjectionOptions::default()
            },
        )
        .unwrap();
        assert_eq!(projection.input_summary.phase_count, 3);
        assert!(projection.diagnostics.stable_polygon_count > 0);
    }

    #[cfg(feature = "inspection")]
    #[test]
    fn cubic_alpha_strict_policy_retains_explicit_partial_rejection() {
        let mut dataset = crate::default_regular_dataset();
        if let TabulatedGrid::Regular(grid) = &mut dataset.grids[0] {
            for field in &mut grid.fields {
                for (index, value) in field.values.iter_mut().enumerate() {
                    *value = TabulatedValue::calculated((index + 1) as f64).unwrap();
                }
            }
            grid.fields[0].values[0] = TabulatedValue::missing();
        }
        let error = calculate_projection(
            &dataset,
            &ProjectionOptions {
                interpolation: InterpolationOptions {
                    source: SourceInterpolation::CubicAlpha {
                        method: CubicAlphaMethod::Akima,
                        continuation: BinaryExtrapolation::Muggianu,
                    },
                    partial_domain_policy: CubicPartialDomainPolicy::Strict,
                },
                ..ProjectionOptions::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ProjectionError::CubicSourceIncomplete { .. }
        ));
    }

    #[test]
    fn traced_projection_is_exactly_equivalent_to_untraced_projection() {
        let dataset = parse_str(include_str!("../fixtures/interior-invariant.tct")).unwrap();
        let options = ProjectionOptions::default();
        let plain = calculate_projection(&dataset, &options).unwrap();
        let mut sink = ternary_contours::VecTraceSink::new(
            ternary_contours::NumericalTraceConfig::decisions(),
        );
        let traced = calculate_projection_with_trace(&dataset, &options, &mut sink).unwrap();
        assert_eq!(plain.levels, traced.levels);
        assert_eq!(plain.stable_boundaries, traced.stable_boundaries);
        assert_eq!(plain.stable_contours, traced.stable_contours);
        let events = sink.events();
        assert!(matches!(
            events.first().map(|event| &event.payload),
            Some(ternary_contours::NumericalTracePayload::RunStarted(_))
        ));
        assert!(matches!(
            events.last().map(|event| &event.payload),
            Some(ternary_contours::NumericalTracePayload::RunCompleted(_))
        ));
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
    }
    #[test]
    fn trace_context_is_recorded_without_affecting_projection() {
        let dataset = parse_str(include_str!("../fixtures/interior-invariant.tct")).unwrap();
        let options = ProjectionOptions::default();
        let mut sink = ternary_contours::VecTraceSink::new(
            ternary_contours::NumericalTraceConfig::decisions(),
        );
        let context = NumericalTraceRunContext {
            input_identifier: Some("fixture.tct".into()),
            input_content_hash: Some("fnv1a64:0000000000000000".into()),
            dataset_revision: Some(7),
            options_revision: Some(11),
            request_id: Some(13),
        };
        calculate_projection_with_trace_context(&dataset, &options, &mut sink, &context).unwrap();
        let Some(ternary_contours::NumericalTracePayload::RunStarted(started)) =
            sink.events().first().map(|event| &event.payload)
        else {
            panic!("trace must begin with run metadata");
        };
        assert_eq!(started.input_identifier.as_deref(), Some("fixture.tct"));
        assert_eq!(
            started.input_content_hash.as_deref(),
            Some("fnv1a64:0000000000000000")
        );
        assert_eq!(started.dataset_revision, Some(7));
        assert_eq!(started.options_revision, Some(11));
        assert_eq!(started.request_id, Some(13));
        assert!(started.interpolation.contains("CubicAlpha"));
        assert_eq!(started.partial_domain_policy, "OneSidedThenLinear");
        assert_eq!(started.continuation, "Muggianu");
    }

    #[test]
    fn traced_isotherm_update_reuses_the_accepted_topology() {
        let dataset = parse_str(include_str!("../fixtures/interior-invariant.tct")).unwrap();
        let initial = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
        let options = ProjectionOptions {
            levels: vec![100.0, 110.0, 120.0],
            ..ProjectionOptions::default()
        };
        let mut sink = ternary_contours::VecTraceSink::new(
            ternary_contours::NumericalTraceConfig::decisions(),
        );
        let reused = calculate_projection_with_trace_context_reusing_stable_topology(
            &dataset,
            &options,
            &mut sink,
            &NumericalTraceRunContext::default(),
            Some(&initial.stable_boundaries),
        )
        .unwrap();
        assert!(reused.diagnostics.stable_topology_reused);
        assert_eq!(reused.stable_boundaries, initial.stable_boundaries);
        assert!(sink.events().iter().any(|event| {
            event.payload.kind() == ternary_contours::NumericalTraceEventKind::StableTopologyReused
        }));
    }

    #[cfg(feature = "inspection")]
    #[test]
    fn detailed_ex_cao_pbo_zno_has_verified_stable_topology_at_20_and_40() {
        let dataset = parse_str(include_str!(
            "../../../calculations/CaO-PbO-ZnO_detailed.tct"
        ))
        .expect("committed detailed EX fixture parses");
        let projections = [20, 40]
            .into_iter()
            .map(|sampling_subdivisions| {
                let projection = calculate_projection(
                    &dataset,
                    &ProjectionOptions {
                        automatic_level_step: Some(100.0),
                        sampling_subdivisions: Some(sampling_subdivisions),
                        interpolation: InterpolationOptions::default(),
                        ..ProjectionOptions::default()
                    },
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "detailed EX fixture at sampling {sampling_subdivisions} failed: {error}"
                    )
                });
                let binary = projection
                    .stable_boundaries
                    .nodes
                    .iter()
                    .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Binary(_)))
                    .count();
                let interior = projection
                    .stable_boundaries
                    .nodes
                    .iter()
                    .filter(|node| {
                        matches!(node, ternary_contours::StableInvariantNode::Interior(_))
                    })
                    .count();
                assert_eq!((binary, interior), (3, 1));
                assert_eq!(projection.stable_boundaries.univariants.len(), 3);
                assert_eq!(projection.stable_boundaries.truncated_univariants.len(), 0);
                assert_eq!(
                    projection
                        .stable_boundaries
                        .interior_invariant_verifications
                        .len(),
                    1
                );
                let verification = &projection
                    .stable_boundaries
                    .interior_invariant_verifications[0];
                assert!(
                    verification.maximum_equality_residual <= 1.0e-9,
                    "{verification:?}"
                );
                (sampling_subdivisions, projection)
            })
            .collect::<Vec<_>>();

        let (_, at_20) = &projections[0];
        let (_, at_40) = &projections[1];
        let topology_20 = ternary_contours::stable_topology_signature(
            &at_20.stable_boundaries,
            ternary_contours::StableTopologyComparisonMode::TopologyOnly,
        )
        .unwrap();
        let topology_40 = ternary_contours::stable_topology_signature(
            &at_40.stable_boundaries,
            ternary_contours::StableTopologyComparisonMode::TopologyOnly,
        )
        .unwrap();
        let comparison = ternary_contours::compare_stable_topology(&topology_20, &topology_40);
        assert!(
            comparison.equal,
            "sampling 20 versus 40 topology mismatch: {}",
            comparison.differences.join("; ")
        );

        fn interior_node(
            projection: &LiquidusProjection,
        ) -> &ternary_contours::StableInvariantNode {
            projection
                .stable_boundaries
                .nodes
                .iter()
                .find(|node| matches!(node, ternary_contours::StableInvariantNode::Interior(_)))
                .expect("exactly one interior invariant")
        }
        let node_20 = interior_node(at_20);
        let node_40 = interior_node(at_40);
        let point_20 = node_20.point().as_array();
        let point_40 = node_40.point().as_array();
        let logical = |point: [f64; 3]| {
            [
                point[1] + 0.5 * point[2],
                0.866_025_403_784_438_6 * point[2],
            ]
        };
        let left = logical(point_20);
        let right = logical(point_40);
        assert!(
            (left[0] - right[0]).hypot(left[1] - right[1]) <= 0.005,
            "interior invariant moved too far: {point_20:?} versus {point_40:?}"
        );
        assert!(
            (node_20.temperature() - node_40.temperature()).abs() <= 2.0,
            "interior invariant temperature moved too far: {} versus {}",
            node_20.temperature(),
            node_40.temperature()
        );
        let pairs = |projection: &LiquidusProjection,
                     node: &ternary_contours::StableInvariantNode| {
            let mut pairs = projection
                .stable_boundaries
                .incident_univariants(node.id())
                .unwrap()
                .iter()
                .map(|path_id| projection.stable_boundaries.univariants[path_id.0].phases)
                .collect::<Vec<_>>();
            pairs.sort_unstable();
            pairs
        };
        assert_eq!(pairs(at_20, node_20), pairs(at_40, node_40));
        fn interior_verification(
            projection: &LiquidusProjection,
        ) -> &ternary_contours::StableInvariantVerification {
            projection
                .stable_boundaries
                .interior_invariant_verifications
                .first()
                .expect("interior verification")
        }
        assert_eq!(
            interior_verification(at_20).stability_margin.is_infinite(),
            interior_verification(at_40).stability_margin.is_infinite()
        );
    }

    #[cfg(feature = "inspection")]
    #[test]
    fn isotherm_only_projection_reuses_the_accepted_stable_topology() {
        let dataset = parse_str(include_str!(
            "../../../calculations/CaO-PbO-ZnO_detailed.tct"
        ))
        .expect("committed detailed EX fixture parses");
        let topology_options = ProjectionOptions {
            automatic_level_step: Some(100.0),
            sampling_subdivisions: Some(20),
            interpolation: InterpolationOptions::default(),
            ..ProjectionOptions::default()
        };
        let initial = calculate_projection(&dataset, &topology_options).unwrap();
        let isotherm_only = ProjectionOptions {
            levels: vec![800.0, 810.0, 820.0],
            automatic_level_step: None,
            ..topology_options
        };
        let reused = calculate_projection_reusing_stable_topology(
            &dataset,
            &isotherm_only,
            &initial.stable_boundaries,
        )
        .unwrap();
        assert!(reused.diagnostics.stable_topology_reused);
        ternary_contours::assert_same_stable_topology(
            &initial.stable_boundaries,
            &reused.stable_boundaries,
        )
        .unwrap();
        assert_eq!(
            initial.stable_boundaries.nodes,
            reused.stable_boundaries.nodes
        );
        assert_eq!(reused.levels, vec![800.0, 810.0, 820.0]);
        assert_ne!(initial.levels, reused.levels);
    }

    #[cfg(feature = "inspection")]
    #[test]
    fn detailed_ex_raw_and_regularized_networks_have_identical_topology() {
        let dataset = parse_str(include_str!(
            "../../../calculations/CaO-PbO-ZnO_detailed.tct"
        ))
        .expect("committed detailed EX fixture parses");
        let raw = calculate_projection(
            &dataset,
            &ProjectionOptions {
                automatic_level_step: Some(100.0),
                sampling_subdivisions: Some(20),
                interpolation: InterpolationOptions::default(),
                ..ProjectionOptions::default()
            },
        )
        .unwrap();
        for spacing in [0.04, 0.02, 0.01, 0.005] {
            let regularized = calculate_projection(
                &dataset,
                &ProjectionOptions {
                    automatic_level_step: Some(100.0),
                    sampling_subdivisions: Some(20),
                    regularize: true,
                    regularization_spacing: Some(spacing),
                    interpolation: InterpolationOptions::default(),
                    ..ProjectionOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("regularization spacing {spacing} failed: {error}"));
            ternary_contours::assert_same_stable_topology(
                &raw.stable_boundaries,
                &regularized.stable_boundaries,
            )
            .unwrap_or_else(|difference| {
                panic!("raw and regularized topology differ at spacing {spacing}: {difference}")
            });
            assert_eq!(
                raw.stable_boundaries.nodes, regularized.stable_boundaries.nodes,
                "regularization must never move invariant nodes"
            );
            assert_eq!(
                raw.stable_boundaries.truncated_univariants.len(),
                regularized.stable_boundaries.truncated_univariants.len()
            );
        }
    }

    #[test]
    fn cao_pbo_zno_fixture_reports_unsupported_binary_transitions_without_losing_contours() {
        let dataset = parse_str(include_str!("../../../calculations/CaO-PbO-ZnO.tct")).unwrap();
        let options = ProjectionOptions {
            automatic_level_step: Some(100.0),
            sampling_subdivisions: Some(20),
            regularize: true,
            interpolation: InterpolationOptions {
                partial_domain_policy: CubicPartialDomainPolicy::OneSidedThenLinear,
                ..InterpolationOptions::default()
            },
            ..ProjectionOptions::default()
        };
        let projection = calculate_projection(&dataset, &options).unwrap();
        assert_eq!(projection.stable_boundaries.nodes.len(), 1);
        assert!(projection.stable_boundaries.univariants.is_empty());
        assert!(projection.diagnostics.contour_path_count > 0);
        let unavailable = projection
            .stable_boundaries
            .binary_traces
            .iter()
            .flat_map(|trace| trace.incomplete_transitions.iter())
            .collect::<Vec<_>>();
        assert_eq!(unavailable.len(), 2);
        assert!(unavailable.iter().all(|transition| matches!(
            transition.reason,
            ternary_contours::BinaryTransitionUnavailableReason::NoRootInOverlappingDomain
        )));
    }

    #[test]
    fn automatic_levels_start_at_exact_minimum_and_never_exceed_maximum() {
        let levels = automatic_iso_levels(742.5, 1_042.5, 100.0).unwrap();
        assert_eq!(levels, vec![742.5, 842.5, 942.5, 1_042.5]);
        assert!(levels.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(levels.iter().all(|level| *level <= 1_042.5));
    }

    #[test]
    fn automatic_levels_reject_non_positive_or_non_finite_steps() {
        assert!(automatic_iso_levels(1.0, 2.0, 0.0).is_err());
        assert!(automatic_iso_levels(1.0, 2.0, f64::NAN).is_err());
    }
    #[test]
    fn out_of_range_levels_have_an_explicit_empty_contour_error() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let error = calculate_projection(
            &dataset,
            &ProjectionOptions {
                levels: vec![1_000.0, 1_100.0],
                ..ProjectionOptions::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ProjectionError::NoContourPaths {
                minimum: 100.0,
                maximum: 120.0,
                ..
            }
        ));
        assert!(
            error
                .to_string()
                .contains("do not intersect the calculated temperature range")
        );
    }
}
