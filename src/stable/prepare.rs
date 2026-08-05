use crate::{
    NoopTraceSink, NumericalTraceEventKind, NumericalTraceLevel, NumericalTraceSession,
    NumericalTraceSink, NumericalTraceStage, RegularTernaryGrid, TraceBinaryBoundary, TraceCounts,
    TraceDecision, decision,
};

use super::{
    StableContourDiagnostics, StableContourError, StableContourLevel, StableContourQuantity,
    StableContourSet, StableGridOptions, StablePhaseId, StablePhaseSource,
    partition::{StableSamplingCell, build_stable_partition},
    paths::assemble_level,
    sample::{
        PreparedSourceLayer, RegularSamplingGrid, SourceGeometryGroup, prepare_sources,
        sample_regular_grid,
    },
    segments::extract_level_segments,
    verify::verify_sampling_grid,
};

/// Stable-phase ensemble prepared once on a common virtual regular sampling grid.
///
/// Source interpolators, including optional cubic-alpha models, are constructed
/// once. Preparation samples and verifies the final sampling-grid representation and
/// caches its exact affine stable polygons. Repeated [`Self::contours`] calls do
/// not locate source points, resample fields, or repeat stable half-plane
/// clipping.
pub struct PreparedStablePhaseEnsemble<'a> {
    quantity: StableContourQuantity,
    options: StableGridOptions,
    phase_ids: Vec<StablePhaseId>,
    samples: RegularSamplingGrid,
    cells: Vec<StableSamplingCell>,
    diagnostics: StableContourDiagnostics,
    layers: Vec<PreparedSourceLayer<'a>>,
    _groups: Vec<SourceGeometryGroup<'a>>,
}

impl<'a> PreparedStablePhaseEnsemble<'a> {
    /// Validate and prepare a stable phase ensemble without recording events.
    pub fn new(
        phases: impl IntoIterator<Item = StablePhaseSource<'a>>,
        quantity: StableContourQuantity,
        options: StableGridOptions,
    ) -> Result<Self, StableContourError> {
        let mut sink = NoopTraceSink;
        let mut trace = NumericalTraceSession::new(&mut sink);
        Self::new_with_trace_session(phases, quantity, options, &mut trace)
    }

    /// Validate and prepare a stable phase ensemble while emitting optional
    /// deterministic source-preparation observations to `sink`.
    pub fn new_with_trace(
        phases: impl IntoIterator<Item = StablePhaseSource<'a>>,
        quantity: StableContourQuantity,
        options: StableGridOptions,
        sink: &mut impl NumericalTraceSink,
    ) -> Result<Self, StableContourError> {
        let mut trace = NumericalTraceSession::new(sink);
        let result = Self::new_with_trace_session(phases, quantity, options, &mut trace);
        if let Err(error) = &result
            && trace.is_enabled(NumericalTraceLevel::Summary)
        {
            trace.emit(
                NumericalTraceLevel::Summary,
                NumericalTraceStage::SourcePreparation,
                decision(
                    NumericalTraceEventKind::SourcePreparationRejected,
                    TraceDecision {
                        reason: Some(error.to_string()),
                        ..TraceDecision::default()
                    },
                ),
            );
        }
        result
    }

    /// Shared-session variant for integrations that need one sequence across preparation, topology, and contours.
    pub fn new_with_trace_session(
        phases: impl IntoIterator<Item = StablePhaseSource<'a>>,
        quantity: StableContourQuantity,
        options: StableGridOptions,
        trace: &mut NumericalTraceSession<'_>,
    ) -> Result<Self, StableContourError> {
        options.validate()?;
        let mut phases: Vec<_> = phases.into_iter().collect();
        if trace.is_enabled(NumericalTraceLevel::Summary) {
            trace.emit(
                NumericalTraceLevel::Summary,
                NumericalTraceStage::SourcePreparation,
                decision(
                    NumericalTraceEventKind::SourcePreparationStarted,
                    TraceDecision {
                        counts: Some(TraceCounts {
                            calculated: phases.len(),
                            ..TraceCounts::default()
                        }),
                        reason: Some(format!(
                            "quantity={quantity:?}, sampling_subdivisions={}",
                            options.subdivisions
                        )),
                        ..TraceDecision::default()
                    },
                ),
            );
        }
        if phases.is_empty() {
            return Err(StableContourError::EmptyPhaseEnsemble);
        }
        phases.sort_by_key(StablePhaseSource::phase);
        if let Some(pair) = phases
            .windows(2)
            .find(|pair| pair[0].phase() == pair[1].phase())
        {
            return Err(StableContourError::DuplicatePhaseId {
                phase: pair[0].phase(),
            });
        }
        for phase in &phases {
            if let Some(secondary) = phase.secondary()
                && !phase.height().has_same_topology(secondary)
            {
                return Err(StableContourError::MismatchedPhaseTopology {
                    phase: phase.phase(),
                });
            }
            if quantity == StableContourQuantity::Secondary && phase.secondary().is_none() {
                return Err(StableContourError::MissingSecondaryScalar {
                    phase: phase.phase(),
                });
            }
        }

        let phase_ids = phases
            .iter()
            .map(StablePhaseSource::phase)
            .collect::<Vec<_>>();
        if trace.is_enabled(NumericalTraceLevel::Decisions) {
            for phase in &phase_ids {
                trace.emit(
                    NumericalTraceLevel::Decisions,
                    NumericalTraceStage::SourcePreparation,
                    decision(
                        NumericalTraceEventKind::PhaseSourceLocated,
                        TraceDecision {
                            phase: Some(phase.0),
                            ..TraceDecision::default()
                        },
                    ),
                );
            }
        }
        let mut diagnostics = StableContourDiagnostics {
            phase_count: phases.len(),
            ..StableContourDiagnostics::default()
        };
        let (layers, groups) = prepare_sources(&phases, quantity, &mut diagnostics)?;
        let mut subdivisions = options.subdivisions;
        let samples = loop {
            let grid = RegularTernaryGrid::new(subdivisions)
                .map_err(|_| StableContourError::SamplingSubdivisionOverflow)?;
            let samples = sample_regular_grid(
                grid,
                phases.len(),
                quantity,
                &layers,
                &groups,
                &mut diagnostics,
            )?;
            let verification = verify_sampling_grid(
                &samples,
                quantity,
                options.verification,
                options.stability_tolerance,
                &layers,
                &groups,
                &mut diagnostics,
            )?;
            diagnostics.verification_point_count += verification.verification_points;
            diagnostics.maximum_height_approximation_error =
                verification.maximum_height_approximation_error;
            diagnostics.rms_height_approximation_error =
                verification.rms_height_approximation_error;
            diagnostics.maximum_secondary_approximation_error =
                verification.maximum_secondary_approximation_error;
            diagnostics.ownership_mismatches = verification.ownership_mismatches;
            diagnostics.hidden_candidate_discoveries = verification.hidden_candidate_discoveries;
            diagnostics.unresolved_sampling_triangles = verification.unresolved_sampling_triangles;
            diagnostics.worst_unresolved_triangle = verification.worst_unresolved_triangle;
            let unresolved = verification.unresolved_sampling_triangles;
            diagnostics.verification_passes.push(verification);
            if !options.verification.enabled || unresolved == 0 {
                break samples;
            }
            let can_refine = diagnostics.refinement_passes
                < options.verification.maximum_refinement_passes
                && subdivisions < options.verification.maximum_subdivisions;
            if can_refine {
                let doubled = subdivisions
                    .checked_mul(2)
                    .ok_or(StableContourError::SamplingSubdivisionOverflow)?;
                let next = doubled.min(options.verification.maximum_subdivisions);
                if next > subdivisions {
                    subdivisions = next;
                    diagnostics.refinement_passes += 1;
                    continue;
                }
            }
            if !options.verification.allow_unresolved {
                return Err(StableContourError::SamplingResolutionInsufficient {
                    subdivisions,
                    unresolved_triangles: unresolved,
                    worst_triangle: diagnostics.worst_unresolved_triangle,
                    maximum_height_error: diagnostics.maximum_height_approximation_error,
                    maximum_secondary_error: diagnostics.maximum_secondary_approximation_error,
                });
            }
            break samples;
        };

        diagnostics.sampling_subdivisions = samples.grid.subdivisions();
        diagnostics.final_subdivisions = samples.grid.subdivisions();
        diagnostics.sampling_vertex_count = samples.grid.vertex_count();
        diagnostics.sampling_triangle_count = samples.grid.triangle_count()?;
        let cells = build_stable_partition(
            &samples,
            &phase_ids,
            options.stability_tolerance,
            options.geometry_tolerance,
            &mut diagnostics,
        )?;
        if trace.is_enabled(NumericalTraceLevel::Summary) {
            trace.emit(
                NumericalTraceLevel::Summary,
                NumericalTraceStage::SourcePreparation,
                decision(
                    NumericalTraceEventKind::SourcePreparationCompleted,
                    TraceDecision {
                        counts: Some(TraceCounts {
                            calculated: diagnostics.sampling_vertex_count,
                            ..TraceCounts::default()
                        }),
                        reason: Some(format!(
                            "sampling_triangles={}, stable_polygons={}",
                            diagnostics.sampling_triangle_count,
                            diagnostics.nonempty_stable_polygons,
                        )),
                        ..TraceDecision::default()
                    },
                ),
            );
        }
        Ok(Self {
            quantity,
            options,
            phase_ids,
            samples,
            cells,
            diagnostics,
            layers,
            _groups: groups,
        })
    }

    /// Discover ordered stable invariants on all three outer binary boundaries.
    pub fn binary_boundary_traces(
        &self,
        options: super::StableBoundaryOptions,
    ) -> Result<Vec<super::BinaryBoundaryTrace>, super::StableBoundaryError> {
        super::boundary::trace_binary_boundaries(&self.layers, &self.phase_ids, options)
    }
    /// Construct the raw boundary-connected stable invariant and univariant graph.
    ///
    /// Binary invariant discovery is performed from the original prepared phase
    /// evaluators. Interior paths use the cached affine stable partition on the
    /// common regular sampling grid. Isolated closed univariant loops that have
    /// no invariant-node seed are intentionally deferred.
    pub fn stable_boundaries(
        &self,
        options: super::StableBoundaryOptions,
    ) -> Result<super::StableBoundaryNetwork, super::StableBoundaryError> {
        let mut sink = NoopTraceSink;
        let mut trace = NumericalTraceSession::new(&mut sink);
        self.stable_boundaries_with_trace_session(options, &mut trace)
    }

    /// Construct the stable boundary network while recording optional
    /// deterministic topology observations to `sink`.
    pub fn stable_boundaries_with_trace(
        &self,
        options: super::StableBoundaryOptions,
        sink: &mut impl NumericalTraceSink,
    ) -> Result<super::StableBoundaryNetwork, super::StableBoundaryError> {
        let mut trace = NumericalTraceSession::new(sink);
        self.stable_boundaries_with_trace_session(options, &mut trace)
    }

    /// Shared-session variant for integrations that need one sequence across a complete projection.
    pub fn stable_boundaries_with_trace_session(
        &self,
        options: super::StableBoundaryOptions,
        trace: &mut NumericalTraceSession<'_>,
    ) -> Result<super::StableBoundaryNetwork, super::StableBoundaryError> {
        if trace.is_enabled(NumericalTraceLevel::Summary) {
            for boundary in super::BinaryBoundary::ALL {
                trace.emit(
                    NumericalTraceLevel::Summary,
                    NumericalTraceStage::BinaryBoundary,
                    decision(
                        NumericalTraceEventKind::BinaryBoundaryStarted,
                        TraceDecision {
                            boundary: Some(trace_boundary(boundary)),
                            ..TraceDecision::default()
                        },
                    ),
                );
            }
        }
        let result = (|| {
            let traces =
                super::boundary::trace_binary_boundaries(&self.layers, &self.phase_ids, options)?;
            super::boundary::build_stable_boundary_network(
                traces,
                &self.cells,
                &self.samples,
                &self.phase_ids,
                &self.layers,
                options,
            )
        })();
        match &result {
            Ok(network) if trace.is_enabled(NumericalTraceLevel::Decisions) => {
                for binary in &network.binary_traces {
                    trace.emit(
                        NumericalTraceLevel::Decisions,
                        NumericalTraceStage::BinaryBoundary,
                        decision(
                            NumericalTraceEventKind::BinaryBoundaryCompleted,
                            TraceDecision {
                                boundary: Some(trace_boundary(binary.boundary)),
                                counts: Some(TraceCounts {
                                    calculated: binary.diagnostics.samples_evaluated,
                                    ..TraceCounts::default()
                                }),
                                reason: Some(format!(
                                    "regions={}, invariants={}, pair_evaluations={}",
                                    binary.regions.len(),
                                    binary.invariants.len(),
                                    binary.diagnostics.pair_only_evaluations,
                                )),
                                ..TraceDecision::default()
                            },
                        ),
                    );
                    for node in &binary.invariants {
                        trace.emit(
                            NumericalTraceLevel::Decisions,
                            NumericalTraceStage::BinaryBoundary,
                            decision(
                                NumericalTraceEventKind::BinaryInvariantEmitted,
                                TraceDecision {
                                    boundary: Some(trace_boundary(node.boundary)),
                                    phase_pair: Some([
                                        node.left_stable_phase.0,
                                        node.right_stable_phase.0,
                                    ]),
                                    composition: Some(node.point.as_array()),
                                    value: Some(node.temperature),
                                    node_id: Some(node.id.0),
                                    ..TraceDecision::default()
                                },
                            ),
                        );
                    }
                }
                for node in &network.nodes {
                    if let super::StableInvariantNode::Interior(interior) = node {
                        trace.emit(
                            NumericalTraceLevel::Decisions,
                            NumericalTraceStage::InteriorInvariant,
                            decision(
                                NumericalTraceEventKind::InteriorInvariantAccepted,
                                TraceDecision {
                                    composition: Some(interior.point.as_array()),
                                    value: Some(interior.temperature),
                                    node_id: Some(interior.id.0),
                                    reason: Some(format!("phases={:?}", interior.phases)),
                                    ..TraceDecision::default()
                                },
                            ),
                        );
                    }
                }
                for path in &network.univariants {
                    trace.emit(
                        NumericalTraceLevel::Decisions,
                        NumericalTraceStage::Univariant,
                        decision(
                            NumericalTraceEventKind::UnivariantTraceCompleted,
                            TraceDecision {
                                phase_pair: Some([path.phases.first.0, path.phases.second.0]),
                                path_id: Some(path.id.0),
                                node_id: Some(path.end.0),
                                counts: Some(TraceCounts {
                                    calculated: path.points.len(),
                                    ..TraceCounts::default()
                                }),
                                ..TraceDecision::default()
                            },
                        ),
                    );
                }
            }
            Err(error) if trace.is_enabled(NumericalTraceLevel::Summary) => {
                trace.emit(
                    NumericalTraceLevel::Summary,
                    NumericalTraceStage::Error,
                    decision(
                        NumericalTraceEventKind::BinaryBoundaryFailed,
                        TraceDecision {
                            reason: Some(error.to_string()),
                            ..TraceDecision::default()
                        },
                    ),
                );
            }
            _ => {}
        }
        result
    }

    /// Trace phase-labelled contours at finite levels.
    ///
    /// Levels are returned in ascending order. Stable polygons and sampled
    /// source values are reused for every level and every call.
    pub fn contours(&self, levels: &[f64]) -> Result<StableContourSet, StableContourError> {
        let mut sink = NoopTraceSink;
        let mut trace = NumericalTraceSession::new(&mut sink);
        self.contours_with_trace_session(levels, &mut trace)
    }

    /// Trace phase-labelled contours using the ordinary cached geometry while
    /// optionally recording deterministic extraction and assembly events.
    pub fn contours_with_trace(
        &self,
        levels: &[f64],
        sink: &mut impl NumericalTraceSink,
    ) -> Result<StableContourSet, StableContourError> {
        let mut trace = NumericalTraceSession::new(sink);
        self.contours_with_trace_session(levels, &mut trace)
    }

    /// Shared-session variant for integrations that need one sequence across a complete projection.
    pub fn contours_with_trace_session(
        &self,
        levels: &[f64],
        trace: &mut NumericalTraceSession<'_>,
    ) -> Result<StableContourSet, StableContourError> {
        let levels = validated_levels(levels, self.options.value_tolerance)?;
        let mut diagnostics = self.diagnostics.clone();
        diagnostics.requested_levels = levels.len();
        let mut results = Vec::with_capacity(levels.len());
        for (level_index, level) in levels.into_iter().enumerate() {
            if trace.is_enabled(NumericalTraceLevel::Summary) {
                trace.emit(
                    NumericalTraceLevel::Summary,
                    NumericalTraceStage::Contour,
                    decision(
                        NumericalTraceEventKind::ContourLevelStarted,
                        TraceDecision {
                            level: Some(level),
                            path_id: Some(level_index),
                            ..TraceDecision::default()
                        },
                    ),
                );
            }
            let segments = extract_level_segments(
                &self.cells,
                &self.samples,
                &self.phase_ids,
                self.quantity,
                level,
                self.options.value_tolerance,
                self.options.stability_tolerance,
                self.options.geometry_tolerance,
                self.options.parameter_tolerance,
                &mut diagnostics,
            )?;
            if trace.is_enabled(NumericalTraceLevel::Decisions) {
                trace.emit(
                    NumericalTraceLevel::Decisions,
                    NumericalTraceStage::Contour,
                    decision(
                        NumericalTraceEventKind::ContourPathAssemblyStarted,
                        TraceDecision {
                            level: Some(level),
                            counts: Some(TraceCounts {
                                calculated: segments.len(),
                                ..TraceCounts::default()
                            }),
                            ..TraceDecision::default()
                        },
                    ),
                );
            }
            let (paths, junctions) = assemble_level(
                segments,
                self.quantity,
                level,
                self.options.geometry_tolerance,
                self.options.parameter_tolerance,
                &mut diagnostics,
            )?;
            if trace.is_enabled(NumericalTraceLevel::Decisions) {
                for (path_index, path) in paths.iter().enumerate() {
                    trace.emit(
                        NumericalTraceLevel::Decisions,
                        NumericalTraceStage::Contour,
                        decision(
                            NumericalTraceEventKind::ContourPathCompleted,
                            TraceDecision {
                                phase: Some(path.phase.0),
                                level: Some(level),
                                path_id: Some(path_index),
                                composition: path.points.first().map(|point| point.as_array()),
                                counts: Some(TraceCounts {
                                    calculated: path.points.len(),
                                    ..TraceCounts::default()
                                }),
                                reason: Some(if path.closed {
                                    "closed".into()
                                } else {
                                    "open".into()
                                }),
                                ..TraceDecision::default()
                            },
                        ),
                    );
                }
                for junction in &junctions {
                    trace.emit(
                        NumericalTraceLevel::Decisions,
                        NumericalTraceStage::Contour,
                        decision(
                            NumericalTraceEventKind::ContourJunctionCreated,
                            TraceDecision {
                                level: Some(level),
                                node_id: Some(junction.id.0),
                                composition: Some(junction.point.as_array()),
                                reason: Some(format!(
                                    "phases={:?}, kind={:?}",
                                    junction.phases, junction.kind
                                )),
                                ..TraceDecision::default()
                            },
                        ),
                    );
                }
            }
            if trace.is_enabled(NumericalTraceLevel::Summary) {
                trace.emit(
                    NumericalTraceLevel::Summary,
                    NumericalTraceStage::Contour,
                    decision(
                        NumericalTraceEventKind::ContourLevelCompleted,
                        TraceDecision {
                            level: Some(level),
                            path_id: Some(level_index),
                            counts: Some(TraceCounts {
                                calculated: paths.len(),
                                ..TraceCounts::default()
                            }),
                            reason: Some(format!("junctions={}", junctions.len())),
                            ..TraceDecision::default()
                        },
                    ),
                );
            }
            results.push(StableContourLevel {
                value: level,
                paths,
                junctions,
            });
        }
        Ok(StableContourSet {
            quantity: self.quantity,
            levels: results,
            diagnostics,
        })
    }
    /// Return preparation and stable-partition diagnostics.
    pub const fn diagnostics(&self) -> &StableContourDiagnostics {
        &self.diagnostics
    }

    /// Return the final virtual regular sampling grid.
    pub const fn sampling_grid(&self) -> RegularTernaryGrid {
        self.samples.grid
    }

    /// Return the traced quantity.
    pub const fn quantity(&self) -> StableContourQuantity {
        self.quantity
    }

    /// Return canonical phase IDs in deterministic order.
    pub fn phase_ids(&self) -> &[StablePhaseId] {
        &self.phase_ids
    }

    /// Return the validated sampling-grid and verification options.
    pub const fn options(&self) -> StableGridOptions {
        self.options
    }
}

fn trace_boundary(boundary: super::BinaryBoundary) -> TraceBinaryBoundary {
    match boundary {
        super::BinaryBoundary::Ab => TraceBinaryBoundary::Ab,
        super::BinaryBoundary::Bc => TraceBinaryBoundary::Bc,
        super::BinaryBoundary::Ca => TraceBinaryBoundary::Ca,
    }
}
fn validated_levels(levels: &[f64], tolerance: f64) -> Result<Vec<f64>, StableContourError> {
    let mut validated = levels.to_vec();
    for (index, value) in validated.iter().copied().enumerate() {
        if !value.is_finite() {
            return Err(StableContourError::NonFiniteLevel { index, value });
        }
    }
    validated.sort_by(f64::total_cmp);
    if let Some(pair) = validated
        .windows(2)
        .find(|pair| (pair[1] - pair[0]).abs() <= tolerance)
    {
        return Err(StableContourError::DuplicateLevel {
            first: pair[0],
            second: pair[1],
        });
    }
    Ok(validated)
}
