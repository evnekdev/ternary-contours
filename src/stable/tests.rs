use super::*;
use crate::{ContourOptions, ContourSet, FieldInterpolation, RegularTernaryScalarField};

fn options(subdivisions: usize) -> StableGridOptions {
    StableGridOptions {
        subdivisions,
        value_tolerance: 1.0e-11,
        stability_tolerance: 1.0e-10,
        geometry_tolerance: 1.0e-9,
        parameter_tolerance: 1.0e-12,
        verification: StableGridVerification {
            maximum_subdivisions: subdivisions.max(1),
            ..StableGridVerification::default()
        },
    }
}

fn field(subdivisions: usize, function: impl FnMut([f64; 3]) -> f64) -> RegularTernaryScalarField {
    RegularTernaryScalarField::from_fn(subdivisions, function).unwrap()
}

fn regular(field: &RegularTernaryScalarField) -> StableScalarSource<'_> {
    StableScalarSource::regular(field, FieldInterpolation::Linear)
}

fn phase<'a>(id: u32, height: &'a RegularTernaryScalarField) -> StablePhaseSource<'a> {
    StablePhaseSource::new(StablePhaseId(id), regular(height))
}

fn close(left: f64, right: f64) {
    assert!((left - right).abs() <= 2.0e-9, "{left:?} != {right:?}");
}

#[test]
fn validates_empty_duplicate_secondary_and_paired_topology() {
    assert!(matches!(
        PreparedStablePhaseEnsemble::new(Vec::new(), StableContourQuantity::Height, options(2)),
        Err(StableContourError::EmptyPhaseEnsemble)
    ));
    let first = field(2, |[a, _, _]| a);
    let second = field(3, |[_, b, _]| b);
    assert!(matches!(
        PreparedStablePhaseEnsemble::new(
            [phase(7, &first), phase(7, &first)],
            StableContourQuantity::Height,
            options(2)
        ),
        Err(StableContourError::DuplicatePhaseId {
            phase: StablePhaseId(7)
        })
    ));
    assert!(matches!(
        PreparedStablePhaseEnsemble::new(
            [phase(1, &first)],
            StableContourQuantity::Secondary,
            options(2)
        ),
        Err(StableContourError::MissingSecondaryScalar {
            phase: StablePhaseId(1)
        })
    ));
    let mismatched = phase(2, &first).with_secondary(regular(&second));
    assert!(matches!(
        PreparedStablePhaseEnsemble::new(
            [mismatched],
            StableContourQuantity::Secondary,
            options(2)
        ),
        Err(StableContourError::MismatchedPhaseTopology {
            phase: StablePhaseId(2)
        })
    ));
}

#[test]
fn one_phase_height_matches_ordinary_linear_contour_geometry() {
    let source = field(5, |[a, b, c]| 2.0 * a - b + 0.5 * c);
    let prepared = PreparedStablePhaseEnsemble::new(
        [phase(9, &source)],
        StableContourQuantity::Height,
        options(5),
    )
    .unwrap();
    let stable = prepared.contours(&[0.35]).unwrap();
    let ordinary = ContourSet::compute(&source, &[0.35], ContourOptions::linear()).unwrap();
    assert_eq!(stable.levels[0].junctions.len(), 0);
    assert_eq!(stable.levels[0].paths.len(), ordinary.levels[0].paths.len());
    for (stable, ordinary) in stable.levels[0].paths.iter().zip(&ordinary.levels[0].paths) {
        assert_eq!(stable.phase, StablePhaseId(9));
        assert_eq!(stable.closed, ordinary.closed);
        assert_eq!(stable.points.len(), ordinary.points.len());
        for (stable, ordinary) in stable.points.iter().zip(&ordinary.points) {
            for (stable, ordinary) in stable.as_array().into_iter().zip(ordinary.as_array()) {
                close(stable, ordinary);
            }
        }
    }
}

#[test]
fn one_phase_secondary_contours_retain_phase_identity() {
    let height = field(4, |_| 12.0);
    let secondary = field(4, |[a, b, c]| a - 2.0 * b + c);
    let prepared = PreparedStablePhaseEnsemble::new(
        [phase(3, &height).with_secondary(regular(&secondary))],
        StableContourQuantity::Secondary,
        options(4),
    )
    .unwrap();
    let result = prepared.contours(&[0.2]).unwrap();
    assert!(!result.levels[0].paths.is_empty());
    assert!(
        result.levels[0]
            .paths
            .iter()
            .all(|path| path.phase == StablePhaseId(3))
    );
    assert!(result.levels[0].junctions.is_empty());
}

#[test]
fn two_affine_phases_share_one_height_junction_but_not_one_path() {
    let a = field(7, |[a, _, _]| a);
    let b = field(7, |[_, b, _]| b);
    let prepared = PreparedStablePhaseEnsemble::new(
        [phase(1, &a), phase(2, &b)],
        StableContourQuantity::Height,
        options(7),
    )
    .unwrap();
    let result = prepared.contours(&[0.4]).unwrap();
    let level = &result.levels[0];
    assert_eq!(level.junctions.len(), 1);
    let junction = &level.junctions[0];
    assert_eq!(junction.kind, StableContourJunctionKind::Univariant);
    assert_eq!(junction.phases, vec![StablePhaseId(1), StablePhaseId(2)]);
    let [ja, jb, jc] = junction.point.as_array();
    close(ja, 0.4);
    close(jb, 0.4);
    close(jc, 0.2);
    assert_eq!(level.paths.len(), 2);
    assert_ne!(level.paths[0].phase, level.paths[1].phase);
    assert!(level.paths.iter().all(|path| {
        path.start_junction == Some(junction.id) || path.end_junction == Some(junction.id)
    }));
}

#[test]
fn metastable_pairwise_equality_below_third_phase_is_not_a_junction() {
    let a = field(8, |[a, _, _]| a);
    let b = field(8, |[_, b, _]| b);
    let c = field(8, |_| 0.8);
    let prepared = PreparedStablePhaseEnsemble::new(
        [phase(1, &a), phase(2, &b), phase(3, &c)],
        StableContourQuantity::Height,
        options(8),
    )
    .unwrap();
    let result = prepared.contours(&[0.2]).unwrap();
    assert!(result.levels[0].paths.is_empty());
    assert!(result.levels[0].junctions.is_empty());
}

#[test]
fn narrow_phase_with_no_triangle_vertex_win_is_found_by_clipping() {
    let base = |[a, b, _c]: [f64; 3]| a + 2.0 * b;
    let centre = field(1, base);
    let beyond_a = field(1, |point @ [a, _, _]| base(point) + a - 0.6);
    let beyond_b = field(1, |point @ [_, b, _]| base(point) + b - 0.6);
    let beyond_c = field(1, |point @ [_, _, c]| base(point) + c - 0.6);
    let prepared = PreparedStablePhaseEnsemble::new(
        [
            phase(10, &beyond_a),
            phase(20, &beyond_b),
            phase(30, &beyond_c),
            phase(40, &centre),
        ],
        StableContourQuantity::Height,
        options(1),
    )
    .unwrap();
    assert!(
        prepared
            .diagnostics()
            .interior_stable_polygons_without_vertex_winner
            >= 1
    );
    let result = prepared.contours(&[1.0]).unwrap();
    assert!(
        result.levels[0]
            .paths
            .iter()
            .any(|path| path.phase == StablePhaseId(40))
    );
}

#[test]
fn three_and_four_phase_invariants_keep_all_tied_phase_ids() {
    // Adding the same affine drift preserves phase ownership but makes the
    // invariant height a contour endpoint rather than the convex envelope's
    // isolated minimum.
    let a = field(9, |[a, _, _]| 3.0 * a);
    let b = field(9, |[a, b, _]| 2.0 * a + b);
    let c = field(9, |[a, _, c]| 2.0 * a + c);
    let three = PreparedStablePhaseEnsemble::new(
        [phase(1, &a), phase(2, &b), phase(3, &c)],
        StableContourQuantity::Height,
        options(9),
    )
    .unwrap()
    .contours(&[1.0])
    .unwrap();
    let invariant = three.levels[0]
        .junctions
        .iter()
        .find(|junction| junction.kind == StableContourJunctionKind::Invariant)
        .unwrap();
    assert_eq!(
        invariant.phases,
        vec![StablePhaseId(1), StablePhaseId(2), StablePhaseId(3)]
    );

    let d = field(9, |[a, b, c]| 2.0 * a + 0.6 * a + 0.3 * b + 0.1 * c);
    let four = PreparedStablePhaseEnsemble::new(
        [phase(1, &a), phase(2, &b), phase(3, &c), phase(4, &d)],
        StableContourQuantity::Height,
        options(9),
    )
    .unwrap()
    .contours(&[1.0])
    .unwrap();
    let invariant = four.levels[0]
        .junctions
        .iter()
        .find(|junction| junction.kind == StableContourJunctionKind::Invariant)
        .unwrap();
    assert_eq!(invariant.phases.len(), 4);
}

#[test]
fn tangential_target_is_diagnostic_only_and_positive_tie_is_typed() {
    let a = field(4, |[a, _, _]| a);
    let tangential =
        PreparedStablePhaseEnsemble::new([phase(1, &a)], StableContourQuantity::Height, options(4))
            .unwrap()
            .contours(&[1.0])
            .unwrap();
    assert!(tangential.levels[0].paths.is_empty());
    assert!(tangential.diagnostics.isolated_target_points > 0);

    let zero = field(4, |_| 0.0);
    let tied = PreparedStablePhaseEnsemble::new(
        [phase(1, &a), phase(2, &zero)],
        StableContourQuantity::Height,
        options(4),
    )
    .unwrap();
    assert!(matches!(
        tied.contours(&[0.0]),
        Err(StableContourError::CoincidentTargetSegment { .. })
    ));
}

#[test]
fn secondary_discontinuity_does_not_force_phase_paths_to_meet() {
    let h_a = field(12, |[a, _, _]| a);
    let h_b = field(12, |[_, b, _]| b);
    let q_a = field(12, |[_, _, c]| c);
    let q_b = field(12, |[a, _, c]| c + 0.2 * a);
    let result = PreparedStablePhaseEnsemble::new(
        [
            phase(1, &h_a).with_secondary(regular(&q_a)),
            phase(2, &h_b).with_secondary(regular(&q_b)),
        ],
        StableContourQuantity::Secondary,
        options(12),
    )
    .unwrap()
    .contours(&[0.2])
    .unwrap();
    let contacts: Vec<_> = result.levels[0]
        .junctions
        .iter()
        .filter(|junction| junction.kind == StableContourJunctionKind::StableBoundaryContact)
        .collect();
    assert_eq!(contacts.len(), 2);
    assert!(!super::segments::points_close(
        contacts[0].point,
        contacts[1].point,
        1.0e-6
    ));
}

#[test]
fn continuous_secondary_endpoints_are_canonicalised() {
    let h_a = field(10, |[a, _, _]| a);
    let h_b = field(10, |[_, b, _]| b);
    let q_a = field(10, |[_, _, c]| c);
    let q_b = field(10, |[_, _, c]| c);
    let result = PreparedStablePhaseEnsemble::new(
        [
            phase(1, &h_a).with_secondary(regular(&q_a)),
            phase(2, &h_b).with_secondary(regular(&q_b)),
        ],
        StableContourQuantity::Secondary,
        options(10),
    )
    .unwrap()
    .contours(&[0.2])
    .unwrap();
    assert_eq!(result.levels[0].junctions.len(), 1);
    let id = result.levels[0].junctions[0].id;
    assert!(
        result.levels[0]
            .paths
            .iter()
            .filter(|path| path.start_junction == Some(id) || path.end_junction == Some(id))
            .count()
            >= 2
    );
}

#[test]
fn shared_regular_geometry_reuses_locations_and_prunes_low_phases() {
    let high = field(6, |[a, b, _]| a + b);
    let second = field(6, |[a, b, _]| a - b);
    let low: Vec<_> = (0..12)
        .map(|index| field(6, move |_| -100.0 - index as f64))
        .collect();
    let mut phases = vec![phase(1, &high), phase(2, &second)];
    phases.extend(
        low.iter()
            .enumerate()
            .map(|(index, field)| phase(100 + index as u32, field)),
    );
    let prepared =
        PreparedStablePhaseEnsemble::new(phases, StableContourQuantity::Height, options(6))
            .unwrap();
    let diagnostics = prepared.diagnostics();
    assert_eq!(diagnostics.geometry_group_count, 1);
    assert_eq!(
        diagnostics.source_point_location_count,
        diagnostics.sampling_vertex_count
    );
    assert_eq!(
        diagnostics.source_scalar_evaluation_count,
        diagnostics.sampling_vertex_count * diagnostics.source_scalar_layer_count
    );
    assert_eq!(
        diagnostics.reused_source_locations,
        diagnostics.source_scalar_evaluation_count - diagnostics.source_point_location_count
    );
    assert!(diagnostics.phases_removed_by_envelope_floor > 0);
}

#[test]
fn verification_refines_globally_and_can_report_insufficient_resolution() {
    let nonlinear = field(8, |[a, b, c]| a * a + 0.5 * b * b - c * c);
    let mut refined_options = options(1);
    refined_options.verification = StableGridVerification {
        enabled: true,
        maximum_refinement_passes: 3,
        maximum_subdivisions: 8,
        height_error_tolerance: 1.0e-12,
        secondary_error_tolerance: 1.0e-12,
        ownership_tolerance: 1.0e-9,
        ..StableGridVerification::default()
    };
    let refined = PreparedStablePhaseEnsemble::new(
        [phase(1, &nonlinear)],
        StableContourQuantity::Height,
        refined_options,
    )
    .unwrap();
    assert!(refined.diagnostics().refinement_passes > 0);
    assert_eq!(refined.diagnostics().final_subdivisions, 8);
    assert_eq!(refined.diagnostics().unresolved_sampling_triangles, 0);

    let mut insufficient_options = options(1);
    insufficient_options.verification = StableGridVerification {
        enabled: true,
        maximum_refinement_passes: 0,
        maximum_subdivisions: 1,
        height_error_tolerance: 1.0e-14,
        ..StableGridVerification::default()
    };
    assert!(matches!(
        PreparedStablePhaseEnsemble::new(
            [phase(1, &nonlinear)],
            StableContourQuantity::Height,
            insufficient_options
        ),
        Err(StableContourError::SamplingResolutionInsufficient { .. })
    ));
}

#[test]
fn input_and_level_order_do_not_change_geometry() {
    let a = field(7, |[a, _, _]| a);
    let b = field(7, |[_, b, _]| b);
    let first = PreparedStablePhaseEnsemble::new(
        [phase(20, &b), phase(10, &a)],
        StableContourQuantity::Height,
        options(7),
    )
    .unwrap()
    .contours(&[0.6, 0.4])
    .unwrap();
    let second = PreparedStablePhaseEnsemble::new(
        [phase(10, &a), phase(20, &b)],
        StableContourQuantity::Height,
        options(7),
    )
    .unwrap()
    .contours(&[0.4, 0.6])
    .unwrap();
    assert_eq!(first.levels, second.levels);
    assert_eq!(first.levels[0].value, 0.4);
}

#[cfg(not(feature = "cubic-alpha"))]
#[test]
fn unavailable_regular_cubic_source_has_a_stable_context_error() {
    let source = field(5, |[a, b, _]| a * a + b);
    let cubic = StableScalarSource::regular(
        &source,
        FieldInterpolation::CubicAlpha(crate::CubicAlphaBuildOptions::default()),
    );
    assert!(matches!(
        PreparedStablePhaseEnsemble::new(
            [StablePhaseSource::new(StablePhaseId(1), cubic)],
            StableContourQuantity::Height,
            options(5)
        ),
        Err(StableContourError::UnsupportedSourceFeature {
            feature: "cubic-alpha",
            ..
        })
    ));
}

#[cfg(feature = "cubic-alpha")]
#[test]
fn regular_cubic_verification_refinement_reduces_sampling_grid_residual() {
    let source = field(10, |[a, b, c]| a * a + 0.4 * b * b - 0.7 * c * c);
    let cubic = StableScalarSource::regular(
        &source,
        FieldInterpolation::CubicAlpha(crate::CubicAlphaBuildOptions::default()),
    );
    let mut refined = options(2);
    refined.verification = StableGridVerification {
        enabled: true,
        maximum_refinement_passes: 2,
        maximum_subdivisions: 8,
        height_error_tolerance: 0.0,
        allow_unresolved: true,
        ..StableGridVerification::default()
    };
    let prepared = PreparedStablePhaseEnsemble::new(
        [StablePhaseSource::new(StablePhaseId(1), cubic)],
        StableContourQuantity::Height,
        refined,
    )
    .unwrap();
    let passes = &prepared.diagnostics().verification_passes;
    assert_eq!(passes.len(), 3);
    assert_eq!(prepared.diagnostics().final_subdivisions, 8);
    assert!(
        passes.last().unwrap().maximum_height_approximation_error
            < passes.first().unwrap().maximum_height_approximation_error
    );
}

#[cfg(feature = "cubic-alpha")]
#[test]
fn regular_cubic_source_is_sampled_once_then_contoured_linearly() {
    let source = field(8, |[a, b, c]| a * a + b * b - c * c);
    let cubic = StableScalarSource::regular(
        &source,
        FieldInterpolation::CubicAlpha(crate::CubicAlphaBuildOptions::default()),
    );
    let prepared = PreparedStablePhaseEnsemble::new(
        [StablePhaseSource::new(StablePhaseId(1), cubic)],
        StableContourQuantity::Height,
        options(6),
    )
    .unwrap();
    let locations = prepared.diagnostics().source_point_location_count;
    let first = prepared.contours(&[0.0]).unwrap();
    let second = prepared.contours(&[-0.1, 0.1]).unwrap();
    assert_eq!(
        prepared.diagnostics().source_point_location_count,
        locations
    );
    assert!(!first.levels[0].paths.is_empty());
    assert_eq!(second.levels.len(), 2);
}

#[cfg(feature = "irregular-delaunay")]
fn irregular_mesh() -> crate::IrregularTernaryMesh {
    crate::IrregularTernaryMesh::new([
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.62, 0.23, 0.15],
        [0.19, 0.64, 0.17],
        [0.16, 0.21, 0.63],
        [0.31, 0.41, 0.28],
    ])
    .unwrap()
}

#[cfg(feature = "irregular-delaunay")]
#[test]
fn mixed_regular_irregular_sources_share_one_sampling_grid_without_extrapolation() {
    let regular_field = field(6, |[a, b, _]| a - b);
    let irregular_field =
        crate::IrregularTernaryScalarField::from_fn(irregular_mesh(), |[a, b, c]| {
            0.2 + 0.5 * a - b + c
        })
        .unwrap();
    let irregular_source =
        StableScalarSource::irregular(&irregular_field, crate::IrregularFieldInterpolation::Linear);
    let prepared = PreparedStablePhaseEnsemble::new(
        [
            phase(1, &regular_field),
            StablePhaseSource::new(StablePhaseId(2), irregular_source),
        ],
        StableContourQuantity::Height,
        options(6),
    )
    .unwrap();
    assert_eq!(prepared.diagnostics().geometry_group_count, 2);
    assert_eq!(prepared.diagnostics().irregular_geometry_group_count, 1);
    assert_eq!(prepared.contours(&[0.25]).unwrap().levels.len(), 1);
}

#[cfg(feature = "irregular-delaunay")]
#[test]
fn shared_irregular_height_secondary_pair_reuses_exact_mesh_identity() {
    let mesh = irregular_mesh();
    let height = crate::IrregularTernaryScalarField::from_fn(mesh.clone(), |[a, _, _]| a).unwrap();
    let secondary = crate::IrregularTernaryScalarField::from_fn(mesh, |[_, b, _]| b).unwrap();
    let phase = StablePhaseSource::new(
        StablePhaseId(8),
        StableScalarSource::irregular(&height, crate::IrregularFieldInterpolation::Linear),
    )
    .with_secondary(StableScalarSource::irregular(
        &secondary,
        crate::IrregularFieldInterpolation::Linear,
    ));
    let prepared =
        PreparedStablePhaseEnsemble::new([phase], StableContourQuantity::Secondary, options(5))
            .unwrap();
    assert_eq!(prepared.diagnostics().geometry_group_count, 1);
    assert_eq!(
        prepared.diagnostics().source_point_location_count,
        prepared.diagnostics().sampling_vertex_count
    );
    assert_eq!(
        prepared.diagnostics().reused_source_locations,
        prepared.diagnostics().sampling_vertex_count
    );
}

#[cfg(feature = "irregular-delaunay")]
#[test]
fn incomplete_irregular_convex_hull_is_rejected_at_an_sampling_vertex() {
    let mesh = crate::IrregularTernaryMesh::new([
        [0.8, 0.1, 0.1],
        [0.1, 0.8, 0.1],
        [0.1, 0.1, 0.8],
        [0.34, 0.33, 0.33],
    ])
    .unwrap();
    let source = crate::IrregularTernaryScalarField::from_fn(mesh, |[a, _, _]| a).unwrap();
    let phase = StablePhaseSource::new(
        StablePhaseId(4),
        StableScalarSource::irregular(&source, crate::IrregularFieldInterpolation::Linear),
    );
    assert!(matches!(
        PreparedStablePhaseEnsemble::new([phase], StableContourQuantity::Height, options(4)),
        Err(StableContourError::IncompleteSourceCoverage { .. })
    ));
}
fn local_endpoint(point: [f64; 3]) -> super::segments::LocalEndpoint {
    super::segments::LocalEndpoint {
        point: point.into(),
        tied_phases: vec![StablePhaseId(1)],
        junction_kind: None,
        source: super::segments::EndpointSource::Interior,
    }
}

fn local_segment(
    triangle: usize,
    start: [f64; 3],
    end: [f64; 3],
) -> super::segments::LocalStableSegment {
    super::segments::LocalStableSegment {
        phase: StablePhaseId(1),
        triangle,
        start: local_endpoint(start),
        end: local_endpoint(end),
    }
}

fn assert_strict_path_progress(path: &StableContourPath) {
    let mut cumulative = 0.0;
    let mut previous_cumulative = 0.0;
    for pair in path.points.windows(2) {
        let left = crate::simplex::logical_from_composition(pair[0].as_array());
        let right = crate::simplex::logical_from_composition(pair[1].as_array());
        let length = (right[0] - left[0]).hypot(right[1] - left[1]);
        assert!(length > 1.0e-12);
        cumulative += length;
        assert!(cumulative > previous_cumulative);
        previous_cumulative = cumulative;
    }
    for triple in path.points.windows(3) {
        let first = crate::simplex::logical_from_composition(triple[0].as_array());
        let second = crate::simplex::logical_from_composition(triple[1].as_array());
        let third = crate::simplex::logical_from_composition(triple[2].as_array());
        let incoming = [second[0] - first[0], second[1] - first[1]];
        let outgoing = [third[0] - second[0], third[1] - second[1]];
        assert!(incoming[0] * outgoing[0] + incoming[1] * outgoing[1] > 0.0);
    }
}

#[test]
fn local_events_sort_forward_replace_provisional_order_and_merge_near_hits() {
    use super::segments::{LocalParameterEvent, forward_events};
    let events = vec![
        LocalParameterEvent {
            point: [0.2, 0.3, 0.5],
            parameter: 0.8,
        },
        // This corrected event is behind the provisional event in input order.
        LocalParameterEvent {
            point: [0.3, 0.2, 0.5],
            parameter: 0.6,
        },
        LocalParameterEvent {
            point: [0.2 + 1.0e-13, 0.3, 0.5 - 1.0e-13],
            parameter: 0.8 + 5.0e-13,
        },
    ];
    let accepted = forward_events(events, 1.0e-9, 1.0e-12, 4, StablePhaseId(1)).unwrap();
    assert_eq!(accepted.len(), 2);
    assert!(accepted[0].parameter + 1.0e-12 < accepted[1].parameter);
    assert_eq!(accepted[0].parameter, 0.6);
    assert_eq!(accepted[1].parameter, 0.8);
}

#[test]
fn extrapolated_edge_root_is_rejected_outside_current_interval() {
    assert_eq!(super::segments::bounded_edge_root(2.0, 1.0, 1.0e-12), None);
    assert_eq!(
        super::segments::bounded_edge_root(-1.0, 1.0, 1.0e-12),
        Some(0.5)
    );
}

#[test]
fn shared_edge_and_multi_cell_vertex_hits_progress_without_zigzag() {
    let shared_edge = field(8, |[a, b, _]| 2.0 * a + b);
    let result = PreparedStablePhaseEnsemble::new(
        [phase(1, &shared_edge)],
        StableContourQuantity::Height,
        options(8),
    )
    .unwrap()
    .contours(&[0.875])
    .unwrap();
    assert!(!result.levels[0].paths.is_empty());
    for path in &result.levels[0].paths {
        assert_strict_path_progress(path);
    }

    let vertex_hit = field(8, |[a, b, _]| a - b);
    let result = PreparedStablePhaseEnsemble::new(
        [phase(1, &vertex_hit)],
        StableContourQuantity::Height,
        options(8),
    )
    .unwrap()
    .contours(&[0.0])
    .unwrap();
    assert_eq!(result.levels[0].paths.len(), 1);
    assert_strict_path_progress(&result.levels[0].paths[0]);
}

#[test]
fn duplicate_directed_state_and_immediate_retrace_are_typed_errors() {
    let first = local_segment(2, [0.1, 0.2, 0.7], [0.4, 0.2, 0.4]);
    let duplicate = first.clone();
    let mut diagnostics = StableContourDiagnostics::default();
    assert!(
        super::paths::assemble_level(
            vec![first],
            StableContourQuantity::Height,
            0.0,
            1.0e-9,
            1.0e-12,
            &mut diagnostics,
        )
        .is_ok()
    );
    let first = local_segment(2, [0.1, 0.2, 0.7], [0.4, 0.2, 0.4]);
    assert!(matches!(
        super::paths::assemble_level(
            vec![first, duplicate],
            StableContourQuantity::Height,
            0.0,
            1.0e-9,
            1.0e-12,
            &mut StableContourDiagnostics::default(),
        ),
        Err(StableContourError::DirectedTraversalCycle { .. })
    ));

    let retrace = vec![
        local_segment(1, [0.1, 0.2, 0.7], [0.5, 0.2, 0.3]),
        local_segment(2, [0.3, 0.2, 0.5], [0.5, 0.2, 0.3]),
    ];
    assert!(matches!(
        super::paths::assemble_level(
            retrace,
            StableContourQuantity::Height,
            0.0,
            1.0e-9,
            1.0e-12,
            &mut StableContourDiagnostics::default(),
        ),
        Err(StableContourError::NonForwardPathAssembly { .. })
    ));
}

#[test]
fn vertex_branch_is_rejected_instead_of_selecting_a_backward_cell() {
    let centre = [0.4, 0.3, 0.3];
    let segments = vec![
        local_segment(1, [0.2, 0.3, 0.5], centre),
        local_segment(2, centre, [0.6, 0.2, 0.2]),
        local_segment(3, centre, [0.3, 0.6, 0.1]),
    ];
    assert!(matches!(
        super::paths::assemble_level(
            segments,
            StableContourQuantity::Height,
            0.0,
            1.0e-9,
            1.0e-12,
            &mut StableContourDiagnostics::default(),
        ),
        Err(StableContourError::AmbiguousPathAssembly { degree: 3, .. })
    ));
}
#[test]
fn repeated_intermediate_stable_sequence_comes_from_upper_envelope_clipping() {
    let switches = [0.15, 0.30, 0.45, 0.60, 0.75];
    let slopes = [-5.0, -3.0, -1.0, 1.0, 3.0, 5.0];
    let mut intercepts = [0.0; 6];
    for index in 1..6 {
        intercepts[index] =
            intercepts[index - 1] + (slopes[index - 1] - slopes[index]) * switches[index - 1];
    }
    let heights: Vec<_> = slopes
        .into_iter()
        .zip(intercepts)
        .map(|(slope, intercept)| field(20, move |[a, _, _]| slope * a + intercept))
        .collect();
    let secondary: Vec<_> = (0..6).map(|_| field(20, |[_, b, _]| b)).collect();
    let ids = [1, 4, 5, 3, 6, 2];
    let phases: Vec<_> = ids
        .into_iter()
        .enumerate()
        .map(|(index, id)| phase(id, &heights[index]).with_secondary(regular(&secondary[index])))
        .collect();
    let result =
        PreparedStablePhaseEnsemble::new(phases, StableContourQuantity::Secondary, options(20))
            .unwrap()
            .contours(&[0.1])
            .unwrap();
    let mut ordered: Vec<_> = result.levels[0]
        .paths
        .iter()
        .map(|path| {
            let mean_a = path
                .points
                .iter()
                .map(|point| point.as_array()[0])
                .sum::<f64>()
                / path.points.len() as f64;
            (mean_a, path.phase)
        })
        .collect();
    ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
    assert_eq!(
        ordered
            .into_iter()
            .map(|(_, phase)| phase)
            .collect::<Vec<_>>(),
        ids.map(StablePhaseId)
    );
    assert!(
        result.levels[0]
            .junctions
            .iter()
            .all(|junction| junction.phases.len() == 2)
    );
}

#[test]
fn positive_affine_height_transform_preserves_stable_geometry() {
    let a = field(9, |[a, b, _]| 0.4 + 1.5 * a - 0.2 * b);
    let b = field(9, |[a, b, _]| -0.1 + 0.3 * a + 1.2 * b);
    let transformed_a = field(9, |[a, b, _]| 3.0 * (0.4 + 1.5 * a - 0.2 * b) - 7.0);
    let transformed_b = field(9, |[a, b, _]| 3.0 * (-0.1 + 0.3 * a + 1.2 * b) - 7.0);
    let original = PreparedStablePhaseEnsemble::new(
        [phase(1, &a), phase(2, &b)],
        StableContourQuantity::Height,
        options(9),
    )
    .unwrap()
    .contours(&[0.8])
    .unwrap();
    let transformed = PreparedStablePhaseEnsemble::new(
        [phase(1, &transformed_a), phase(2, &transformed_b)],
        StableContourQuantity::Height,
        options(9),
    )
    .unwrap()
    .contours(&[3.0 * 0.8 - 7.0])
    .unwrap();
    assert_eq!(
        original.levels[0].paths.len(),
        transformed.levels[0].paths.len()
    );
    assert_eq!(
        original.levels[0].junctions.len(),
        transformed.levels[0].junctions.len()
    );
    for (left, right) in original.levels[0]
        .paths
        .iter()
        .zip(&transformed.levels[0].paths)
    {
        assert_eq!(left.phase, right.phase);
        assert_eq!(left.points.len(), right.points.len());
        for (left, right) in left.points.iter().zip(&right.points) {
            for (left, right) in left.as_array().into_iter().zip(right.as_array()) {
                close(left, right);
            }
        }
    }
}

#[test]
fn every_output_point_satisfies_target_and_upper_envelope() {
    let heights = [
        field(11, |[a, b, c]| 0.2 + 1.3 * a - 0.4 * b + 0.1 * c),
        field(11, |[a, b, c]| -0.1 + 0.2 * a + 1.1 * b + 0.3 * c),
        field(11, |[a, b, c]| 0.4 - 0.2 * a + 0.1 * b + 0.9 * c),
    ];
    let prepared = PreparedStablePhaseEnsemble::new(
        [
            phase(1, &heights[0]),
            phase(2, &heights[1]),
            phase(3, &heights[2]),
        ],
        StableContourQuantity::Height,
        options(11),
    )
    .unwrap();
    let result = prepared.contours(&[0.55, 0.75]).unwrap();
    for level in &result.levels {
        for path in &level.paths {
            let owner = (path.phase.0 - 1) as usize;
            let evaluators: Vec<_> = heights
                .iter()
                .map(|field| {
                    crate::InterpolatedTernaryField::new(field, FieldInterpolation::Linear).unwrap()
                })
                .collect();
            for point in &path.points {
                let values: Vec<_> = evaluators
                    .iter()
                    .map(|evaluator| evaluator.value(point.as_array()).unwrap())
                    .collect();
                close(values[owner], level.value);
                assert!(values.iter().all(|value| {
                    values[owner] >= *value - prepared.options().stability_tolerance * 2.0
                }));
            }
        }
    }
}

#[test]
fn unresolved_output_requires_explicit_opt_in_and_height_ignores_secondary() {
    let nonlinear = field(6, |[a, b, _]| a * a + b * b);
    let ignored_secondary = field(6, |[_, _, c]| c);
    let height_only = phase(1, &nonlinear).with_secondary(regular(&ignored_secondary));
    let mut unresolved = options(1);
    unresolved.verification = StableGridVerification {
        enabled: true,
        maximum_refinement_passes: 0,
        maximum_subdivisions: 1,
        height_error_tolerance: 1.0e-14,
        allow_unresolved: true,
        ..StableGridVerification::default()
    };
    let prepared =
        PreparedStablePhaseEnsemble::new([height_only], StableContourQuantity::Height, unresolved)
            .unwrap();
    assert!(prepared.diagnostics().unresolved_sampling_triangles > 0);
    assert_eq!(prepared.diagnostics().source_scalar_layer_count, 1);
}

#[test]
fn positive_area_top_height_tie_is_never_assigned_by_phase_order() {
    let first = field(4, |[a, b, _]| a + b);
    let second = field(4, |[a, b, _]| a + b);
    assert!(matches!(
        PreparedStablePhaseEnsemble::new(
            [phase(20, &first), phase(10, &second)],
            StableContourQuantity::Height,
            options(4)
        ),
        Err(StableContourError::PositiveAreaHeightTie { phases, .. })
            if phases == [StablePhaseId(10), StablePhaseId(20)]
    ));
}

#[cfg(feature = "irregular-delaunay")]
#[test]
fn cloned_mesh_identity_is_required_for_irregular_scalar_pairing() {
    let first_mesh = irregular_mesh();
    let second_mesh = irregular_mesh();
    let height = crate::IrregularTernaryScalarField::from_fn(first_mesh, |[a, _, _]| a).unwrap();
    let secondary =
        crate::IrregularTernaryScalarField::from_fn(second_mesh, |[_, b, _]| b).unwrap();
    let source = StablePhaseSource::new(
        StablePhaseId(1),
        StableScalarSource::irregular(&height, crate::IrregularFieldInterpolation::Linear),
    )
    .with_secondary(StableScalarSource::irregular(
        &secondary,
        crate::IrregularFieldInterpolation::Linear,
    ));
    assert!(matches!(
        PreparedStablePhaseEnsemble::new([source], StableContourQuantity::Secondary, options(4)),
        Err(StableContourError::MismatchedPhaseTopology { .. })
    ));
}

#[cfg(all(feature = "irregular-delaunay", not(feature = "irregular-cubic-alpha")))]
#[test]
fn unavailable_irregular_cubic_source_has_a_stable_context_error() {
    let source =
        crate::IrregularTernaryScalarField::from_fn(irregular_mesh(), |[a, b, _]| a * a + b)
            .unwrap();
    let cubic = StableScalarSource::irregular(
        &source,
        crate::IrregularFieldInterpolation::CubicAlpha(crate::IrregularCubicAlphaOptions::default()),
    );
    assert!(matches!(
        PreparedStablePhaseEnsemble::new(
            [StablePhaseSource::new(StablePhaseId(1), cubic)],
            StableContourQuantity::Height,
            options(5)
        ),
        Err(StableContourError::UnsupportedSourceFeature {
            feature: "irregular-cubic-alpha",
            ..
        })
    ));
}

#[cfg(feature = "irregular-cubic-alpha")]
#[test]
fn irregular_cubic_source_is_prepared_once_and_sampled_onto_sampling_grid() {
    let source = crate::IrregularTernaryScalarField::from_fn(irregular_mesh(), |[a, b, c]| {
        1.5 * a - 0.7 * b + 0.2 * c
    })
    .unwrap();
    let phase = StablePhaseSource::new(
        StablePhaseId(1),
        StableScalarSource::irregular(
            &source,
            crate::IrregularFieldInterpolation::CubicAlpha(
                crate::IrregularCubicAlphaOptions::default(),
            ),
        ),
    );
    let prepared =
        PreparedStablePhaseEnsemble::new([phase], StableContourQuantity::Height, options(6))
            .unwrap();
    let locations = prepared.diagnostics().source_point_location_count;
    let first = prepared.contours(&[0.1]).unwrap();
    let second = prepared.contours(&[-0.1, 0.3]).unwrap();
    assert_eq!(
        prepared.diagnostics().source_point_location_count,
        locations
    );
    assert!(!first.levels[0].paths.is_empty());
    assert_eq!(second.levels.len(), 2);
}
