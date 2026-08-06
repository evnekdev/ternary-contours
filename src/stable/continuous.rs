//! Continuous stable-contour transfer-root isolation.
//!
//! The sampled partition is retained solely as a deterministic accelerator for
//! phase-local segments.  Junction roots are isolated on the accepted stable
//! boundary graph and then corrected with the same prepared source evaluators
//! used by stable-boundary construction.  This deliberately prevents sampled
//! polygon endpoints from becoming physical contour topology.

use core::cmp::Ordering;

use crate::{TernaryCoordinate, simplex::logical_from_composition};

use super::{
    StableBoundaryNetwork, StableContourDiagnostics, StableContourError, StableContourHalfEdge,
    StableContourJunction, StableContourJunctionKind, StableContourJunctionVerification,
    StableContourPath, StableContourPathGeometryState, StableContourQuantity, StableGridOptions,
    StableInvariantNode, StableInvariantNodeId, StablePathGeometryState, StablePhaseEvaluation,
    StablePhaseId, StablePhasePair, StableUnivariantId,
    sample::{PreparedSourceLayer, evaluate_layer_at_point},
    source::ScalarRole,
};

/// The branch sampler is an isolation grid, not a topology cap.  Every
/// retained interval launches a continuous two-equation correction and roots
/// are deduplicated only after that verification.
const BRANCH_ISOLATION_SUBDIVISIONS: usize = 32;
const MAX_NEWTON_ITERATIONS: usize = 32;
const DIFFERENCE_STEP: f64 = 1.0e-6;

#[derive(Clone)]
struct Candidate {
    point: TernaryCoordinate,
    pair: StablePhasePair,
    branch: StableUnivariantId,
    invariant: Option<StableInvariantNodeId>,
    kind: StableContourJunctionKind,
    verification: StableContourJunctionVerification,
}

pub(crate) fn isolate_transfer_junctions(
    layers: &[PreparedSourceLayer<'_>],
    phase_ids: &[StablePhaseId],
    quantity: StableContourQuantity,
    options: StableGridOptions,
    boundaries: &StableBoundaryNetwork,
    level: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<Vec<StableContourJunction>, StableContourError> {
    let mut candidates = Vec::new();
    for branch in &boundaries.univariants {
        diagnostics.continuous_boundary_branches_searched += 1;
        for segment in branch.points.windows(2) {
            let intervals = isolate_intervals(layers, quantity, branch.phases, segment, level)?;
            diagnostics.contour_root_isolation_regions += intervals.len();
            for (left, right) in intervals {
                diagnostics.continuous_solver_launches += 1;
                let seed = bisect_seed(layers, quantity, branch.phases, left, right, level)?;
                let Some((point, iterations)) = solve_pair_level(
                    layers,
                    quantity,
                    branch.phases,
                    level,
                    seed,
                    options.value_tolerance,
                )?
                else {
                    diagnostics.contour_root_rejections += 1;
                    continue;
                };
                let Some((kind, invariant, verification)) = verify_candidate(
                    layers,
                    phase_ids,
                    quantity,
                    options,
                    boundaries,
                    branch.id,
                    branch.phases,
                    level,
                    point,
                    iterations,
                )?
                else {
                    diagnostics.contour_root_rejections += 1;
                    continue;
                };
                candidates.push(Candidate {
                    point,
                    pair: branch.phases,
                    branch: branch.id,
                    invariant,
                    kind,
                    verification,
                });
            }
        }
    }
    candidates.sort_by(candidate_order);
    let mut accepted = Vec::new();
    for candidate in candidates {
        if accepted.iter().any(|existing: &Candidate| {
            same_root(
                existing,
                &candidate,
                options.geometry_tolerance,
                options.value_tolerance,
            )
        }) {
            diagnostics.contour_duplicate_roots_removed += 1;
            continue;
        }
        accepted.push(candidate);
    }
    accepted.sort_by(candidate_order);
    let result = accepted
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| StableContourJunction {
            id: super::StableJunctionId(index),
            point: candidate.point,
            phases: match candidate.kind {
                StableContourJunctionKind::InvariantLevelCoincidence => boundaries
                    .nodes
                    .get(
                        candidate
                            .invariant
                            .expect("invariant coincidence carries node")
                            .0,
                    )
                    .map(|node| node.phases().to_vec())
                    .unwrap_or_else(|| vec![candidate.pair.first, candidate.pair.second]),
                _ => vec![candidate.pair.first, candidate.pair.second],
            },
            kind: candidate.kind,
            branch: Some(candidate.branch),
            invariant: candidate.invariant,
            verification: Some(candidate.verification),
        })
        .collect::<Vec<_>>();
    diagnostics.continuous_transfer_junctions += result
        .iter()
        .filter(|junction| junction.kind == StableContourJunctionKind::RegularTransfer)
        .count();
    diagnostics.one_sided_secondary_contacts += result
        .iter()
        .filter(|junction| junction.kind == StableContourJunctionKind::OneSidedSecondaryContact)
        .count();
    diagnostics.invariant_level_coincidences += result
        .iter()
        .filter(|junction| junction.kind == StableContourJunctionKind::InvariantLevelCoincidence)
        .count();
    Ok(result)
}

fn candidate_order(left: &Candidate, right: &Candidate) -> Ordering {
    left.branch
        .cmp(&right.branch)
        .then_with(|| left.pair.cmp(&right.pair))
        .then_with(|| lex_point(left.point, right.point))
        .then_with(|| left.kind.cmp(&right.kind))
}

fn same_root(left: &Candidate, right: &Candidate, geometry: f64, value: f64) -> bool {
    if left.kind == StableContourJunctionKind::InvariantLevelCoincidence
        && right.kind == StableContourJunctionKind::InvariantLevelCoincidence
        && left.invariant == right.invariant
    {
        return true;
    }
    left.pair == right.pair
        && left.branch == right.branch
        && logical_distance(left.point, right.point) <= geometry
        && (left.verification.level_residuals[0].1 - right.verification.level_residuals[0].1).abs()
            <= value
}

fn isolate_intervals(
    layers: &[PreparedSourceLayer<'_>],
    quantity: StableContourQuantity,
    pair: StablePhasePair,
    segment: &[TernaryCoordinate],
    level: f64,
) -> Result<Vec<(TernaryCoordinate, TernaryCoordinate)>, StableContourError> {
    let start = segment[0];
    let end = segment[1];
    let mut samples = Vec::with_capacity(BRANCH_ISOLATION_SUBDIVISIONS + 1);
    for index in 0..=BRANCH_ISOLATION_SUBDIVISIONS {
        let parameter = index as f64 / BRANCH_ISOLATION_SUBDIVISIONS as f64;
        let point = interpolate(start, end, parameter);
        let value = quantity_value(layers, pair.first, quantity, point)?;
        samples.push((point, value.map(|value| value - level)));
    }
    let mut intervals = Vec::new();
    for window in samples.windows(2) {
        let (left_point, left) = window[0];
        let (right_point, right) = window[1];
        let (Some(left), Some(right)) = (left, right) else {
            continue;
        };
        if left == 0.0 || right == 0.0 || left.is_sign_positive() != right.is_sign_positive() {
            intervals.push((left_point, right_point));
        }
    }
    Ok(intervals)
}

fn bisect_seed(
    layers: &[PreparedSourceLayer<'_>],
    quantity: StableContourQuantity,
    pair: StablePhasePair,
    mut left: TernaryCoordinate,
    mut right: TernaryCoordinate,
    level: f64,
) -> Result<TernaryCoordinate, StableContourError> {
    let Some(mut left_value) = quantity_value(layers, pair.first, quantity, left)? else {
        return Ok(left);
    };
    left_value -= level;
    for _ in 0..24 {
        let middle = interpolate(left, right, 0.5);
        let Some(value) = quantity_value(layers, pair.first, quantity, middle)? else {
            break;
        };
        let value = value - level;
        if value.abs() <= 1.0e-12 {
            return Ok(middle);
        }
        if left_value.is_sign_positive() != value.is_sign_positive() {
            right = middle;
        } else {
            left = middle;
            left_value = value;
        }
    }
    Ok(interpolate(left, right, 0.5))
}

fn solve_pair_level(
    layers: &[PreparedSourceLayer<'_>],
    quantity: StableContourQuantity,
    pair: StablePhasePair,
    level: f64,
    seed: TernaryCoordinate,
    tolerance: f64,
) -> Result<Option<(TernaryCoordinate, usize)>, StableContourError> {
    let mut point = seed;
    for iteration in 0..MAX_NEWTON_ITERATIONS {
        let Some([height_a, height_b, quantity_a]) =
            values_for_pair(layers, quantity, pair, point)?
        else {
            return Ok(None);
        };
        let residual = [height_a - height_b, quantity_a - level];
        if residual[0].abs() <= tolerance && residual[1].abs() <= tolerance {
            return Ok(Some((point, iteration)));
        }
        let Some(first) = shifted(point, DIFFERENCE_STEP, 0.0) else {
            return Ok(None);
        };
        let Some(second) = shifted(point, 0.0, DIFFERENCE_STEP) else {
            return Ok(None);
        };
        let Some([ha_x, hb_x, qa_x]) = values_for_pair(layers, quantity, pair, first)? else {
            return Ok(None);
        };
        let Some([ha_y, hb_y, qa_y]) = values_for_pair(layers, quantity, pair, second)? else {
            return Ok(None);
        };
        let jacobian = [
            [
                (ha_x - hb_x - residual[0]) / DIFFERENCE_STEP,
                (ha_y - hb_y - residual[0]) / DIFFERENCE_STEP,
            ],
            [
                (qa_x - level - residual[1]) / DIFFERENCE_STEP,
                (qa_y - level - residual[1]) / DIFFERENCE_STEP,
            ],
        ];
        let determinant = jacobian[0][0] * jacobian[1][1] - jacobian[0][1] * jacobian[1][0];
        if !determinant.is_finite() || determinant.abs() <= f64::EPSILON {
            return Ok(None);
        }
        let delta_b = (-residual[0] * jacobian[1][1] + jacobian[0][1] * residual[1]) / determinant;
        let delta_c = (-jacobian[0][0] * residual[1] + residual[0] * jacobian[1][0]) / determinant;
        let mut scale = 1.0;
        let mut next = None;
        while scale >= 1.0 / 128.0 {
            if let Some(candidate) = shifted(point, delta_b * scale, delta_c * scale) {
                next = Some(candidate);
                break;
            }
            scale *= 0.5;
        }
        let Some(next) = next else { return Ok(None) };
        point = next;
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn verify_candidate(
    layers: &[PreparedSourceLayer<'_>],
    phase_ids: &[StablePhaseId],
    quantity: StableContourQuantity,
    options: StableGridOptions,
    boundaries: &StableBoundaryNetwork,
    branch: StableUnivariantId,
    pair: StablePhasePair,
    level: f64,
    point: TernaryCoordinate,
    solver_iterations: usize,
) -> Result<
    Option<(
        StableContourJunctionKind,
        Option<StableInvariantNodeId>,
        StableContourJunctionVerification,
    )>,
    StableContourError,
> {
    let mut heights = Vec::with_capacity(phase_ids.len());
    for &phase in phase_ids {
        let Some(value) = quantity_value(layers, phase, StableContourQuantity::Height, point)?
        else {
            return Ok(None);
        };
        heights.push((phase, value));
    }
    let Some(height_a) = heights
        .iter()
        .find(|(phase, _)| *phase == pair.first)
        .map(|(_, value)| *value)
    else {
        return Ok(None);
    };
    let Some(height_b) = heights
        .iter()
        .find(|(phase, _)| *phase == pair.second)
        .map(|(_, value)| *value)
    else {
        return Ok(None);
    };
    let maximum = heights
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::NEG_INFINITY, f64::max);
    if height_a < maximum - options.stability_tolerance
        || height_b < maximum - options.stability_tolerance
    {
        return Ok(None);
    }
    let tied = heights
        .iter()
        .filter(|(_, value)| *value >= maximum - options.stability_tolerance)
        .map(|(phase, _)| *phase)
        .collect::<Vec<_>>();
    let quantity_a =
        quantity_value(layers, pair.first, quantity, point)?.expect("defined with height");
    let quantity_b =
        quantity_value(layers, pair.second, quantity, point)?.expect("defined with height");
    let verification = StableContourJunctionVerification {
        height_values: heights.clone(),
        quantity_values: vec![(pair.first, quantity_a), (pair.second, quantity_b)],
        equality_residual: height_a - height_b,
        level_residuals: vec![
            (pair.first, quantity_a - level),
            (pair.second, quantity_b - level),
        ],
        stability_margin: heights
            .iter()
            .filter(|(phase, _)| *phase != pair.first && *phase != pair.second)
            .map(|(_, value)| maximum - *value)
            .fold(f64::INFINITY, f64::min),
        sampling_triangle: None,
        branch: Some(branch),
        solver_iterations,
    };
    if (height_a - height_b).abs() > options.value_tolerance
        || (quantity_a - level).abs() > options.value_tolerance
    {
        return Ok(None);
    }
    let invariant = boundaries.nodes.iter().find_map(|node| match node {
        StableInvariantNode::Interior(interior)
            if interior.phases.len() == 3
                && logical_distance(interior.point, point) <= options.geometry_tolerance
                && (interior.temperature - level).abs() <= options.value_tolerance =>
        {
            Some(interior.id)
        }
        _ => None,
    });
    if invariant.is_some() && quantity == StableContourQuantity::Height {
        return Ok(Some((
            StableContourJunctionKind::InvariantLevelCoincidence,
            invariant,
            verification,
        )));
    }
    if tied.len() != 2 {
        return Ok(Some((
            StableContourJunctionKind::Degenerate,
            None,
            verification,
        )));
    }
    match quantity {
        StableContourQuantity::Height => Ok(Some((
            StableContourJunctionKind::RegularTransfer,
            None,
            verification,
        ))),
        StableContourQuantity::Secondary
            if (quantity_b - level).abs() <= options.value_tolerance =>
        {
            Ok(Some((
                StableContourJunctionKind::RegularTransfer,
                None,
                verification,
            )))
        }
        StableContourQuantity::Secondary => Ok(Some((
            StableContourJunctionKind::OneSidedSecondaryContact,
            None,
            verification,
        ))),
    }
}

fn values_for_pair(
    layers: &[PreparedSourceLayer<'_>],
    quantity: StableContourQuantity,
    pair: StablePhasePair,
    point: TernaryCoordinate,
) -> Result<Option<[f64; 3]>, StableContourError> {
    let Some(height_a) = quantity_value(layers, pair.first, StableContourQuantity::Height, point)?
    else {
        return Ok(None);
    };
    let Some(height_b) = quantity_value(layers, pair.second, StableContourQuantity::Height, point)?
    else {
        return Ok(None);
    };
    let Some(quantity_a) = quantity_value(layers, pair.first, quantity, point)? else {
        return Ok(None);
    };
    Ok(Some([height_a, height_b, quantity_a]))
}

fn quantity_value(
    layers: &[PreparedSourceLayer<'_>],
    phase: StablePhaseId,
    quantity: StableContourQuantity,
    point: TernaryCoordinate,
) -> Result<Option<f64>, StableContourError> {
    let role = match quantity {
        StableContourQuantity::Height => ScalarRole::Height,
        StableContourQuantity::Secondary => ScalarRole::Secondary,
    };
    let Some(layer) = layers
        .iter()
        .find(|layer| layer.phase == phase && layer.role == role)
    else {
        return Ok(None);
    };
    match evaluate_layer_at_point(layer, point.as_array())? {
        StablePhaseEvaluation::Defined { value } => Ok(Some(value)),
        StablePhaseEvaluation::Undefined { .. } => Ok(None),
    }
}

fn interpolate(
    left: TernaryCoordinate,
    right: TernaryCoordinate,
    parameter: f64,
) -> TernaryCoordinate {
    let left = left.as_array();
    let right = right.as_array();
    [
        left[0] + (right[0] - left[0]) * parameter,
        left[1] + (right[1] - left[1]) * parameter,
        left[2] + (right[2] - left[2]) * parameter,
    ]
    .into()
}

fn shifted(point: TernaryCoordinate, delta_b: f64, delta_c: f64) -> Option<TernaryCoordinate> {
    let [_, b, c] = point.as_array();
    let b = b + delta_b;
    let c = c + delta_c;
    let a = 1.0 - b - c;
    (a.is_finite() && b.is_finite() && c.is_finite() && a >= 0.0 && b >= 0.0 && c >= 0.0)
        .then_some([a, b, c].into())
}

fn logical_distance(left: TernaryCoordinate, right: TernaryCoordinate) -> f64 {
    let left = logical_from_composition(left.as_array());
    let right = logical_from_composition(right.as_array());
    (left[0] - right[0]).hypot(left[1] - right[1])
}

fn lex_point(left: TernaryCoordinate, right: TernaryCoordinate) -> Ordering {
    left.as_array()[0]
        .total_cmp(&right.as_array()[0])
        .then_with(|| left.as_array()[1].total_cmp(&right.as_array()[1]))
        .then_with(|| left.as_array()[2].total_cmp(&right.as_array()[2]))
}

/// Carry the raw/regularized provenance of the accepted stable-boundary branch
/// into each phase-labelled level path without letting regularization change
/// its topology. A path without a transfer endpoint remains raw.
pub(crate) fn apply_boundary_geometry_state(
    paths: &mut [StableContourPath],
    junctions: &[StableContourJunction],
    boundaries: &StableBoundaryNetwork,
) {
    for path in paths {
        let states = [path.start_junction, path.end_junction]
            .into_iter()
            .flatten()
            .filter_map(|id| junctions.get(id.0))
            .filter_map(|junction| junction.branch)
            .filter_map(|branch| boundaries.path_geometry_state(branch))
            .collect::<Vec<_>>();
        path.geometry_state = if states.is_empty()
            || states
                .iter()
                .all(|state| *state == StablePathGeometryState::Raw)
        {
            StableContourPathGeometryState::Raw
        } else if states
            .iter()
            .all(|state| *state == StablePathGeometryState::Regularized)
        {
            StableContourPathGeometryState::Regularized
        } else {
            StableContourPathGeometryState::RawFallback
        };
    }
}

/// Continuously correct interior points of each phase-labelled contour route.
/// Junction endpoints remain owned by the continuous stable-boundary root
/// solver; this phase-local correction cannot alter transfer identity.
pub(crate) fn refine_paths_continuously(
    layers: &[PreparedSourceLayer<'_>],
    phase_ids: &[StablePhaseId],
    quantity: StableContourQuantity,
    options: StableGridOptions,
    level: f64,
    paths: &mut [StableContourPath],
    diagnostics: &mut StableContourDiagnostics,
) -> Result<(), StableContourError> {
    for path in paths {
        diagnostics.continuous_phase_contour_segments += path.points.len().saturating_sub(1);
        let last = path.points.len().saturating_sub(1);
        for index in 0..path.points.len() {
            let protected = (index == 0 && path.start_junction.is_some())
                || (index == last && path.end_junction.is_some());
            if protected {
                continue;
            }
            let seed = path.points[index];
            if let Some(point) = project_phase_level(
                layers, phase_ids, quantity, path.phase, level, seed, options,
            )? {
                path.points[index] = point;
                diagnostics.continuous_phase_contour_points += 1;
            } else {
                diagnostics.continuous_phase_contour_rejections += 1;
            }
        }
    }
    Ok(())
}

fn project_phase_level(
    layers: &[PreparedSourceLayer<'_>],
    phase_ids: &[StablePhaseId],
    quantity: StableContourQuantity,
    phase: StablePhaseId,
    level: f64,
    mut point: TernaryCoordinate,
    options: StableGridOptions,
) -> Result<Option<TernaryCoordinate>, StableContourError> {
    for _ in 0..MAX_NEWTON_ITERATIONS {
        let Some(value) = quantity_value(layers, phase, quantity, point)? else {
            return Ok(None);
        };
        let residual = value - level;
        if residual.abs() <= options.value_tolerance {
            let mut maximum = f64::NEG_INFINITY;
            let mut owner = None;
            for &candidate in phase_ids {
                let Some(height) =
                    quantity_value(layers, candidate, StableContourQuantity::Height, point)?
                else {
                    continue;
                };
                if height > maximum {
                    maximum = height;
                    owner = Some((candidate, height));
                }
            }
            return Ok(owner
                .filter(|(candidate, height)| {
                    *candidate == phase && *height >= maximum - options.stability_tolerance
                })
                .map(|_| point));
        }
        let Some(first) = shifted(point, DIFFERENCE_STEP, 0.0) else {
            return Ok(None);
        };
        let Some(second) = shifted(point, 0.0, DIFFERENCE_STEP) else {
            return Ok(None);
        };
        let Some(value_b) = quantity_value(layers, phase, quantity, first)? else {
            return Ok(None);
        };
        let Some(value_c) = quantity_value(layers, phase, quantity, second)? else {
            return Ok(None);
        };
        let gradient = [
            (value_b - value) / DIFFERENCE_STEP,
            (value_c - value) / DIFFERENCE_STEP,
        ];
        let norm_squared = gradient[0] * gradient[0] + gradient[1] * gradient[1];
        if !norm_squared.is_finite() || norm_squared <= f64::EPSILON {
            return Ok(None);
        }
        let delta = [
            -residual * gradient[0] / norm_squared,
            -residual * gradient[1] / norm_squared,
        ];
        let mut scale = 1.0;
        let mut next = None;
        while scale >= 1.0 / 128.0 {
            if let Some(candidate) = shifted(point, delta[0] * scale, delta[1] * scale) {
                next = Some(candidate);
                break;
            }
            scale *= 0.5;
        }
        let Some(next) = next else { return Ok(None) };
        point = next;
    }
    Ok(None)
}

/// Validate the semantic A-to-B incidence graph.  This is intentionally kept
/// after phase-local path assembly: a transfer is not a geometric coincidence,
/// it is one A half-edge and one B half-edge at one canonical junction.
pub(crate) fn validate_transfer_incidence(
    junctions: &mut [StableContourJunction],
    half_edges: &[StableContourHalfEdge],
    _level: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<(), StableContourError> {
    for junction in junctions
        .iter_mut()
        .filter(|junction| junction.kind == StableContourJunctionKind::RegularTransfer)
    {
        let expected = match junction.phases.as_slice() {
            [first, second] => [*first, *second],
            _ => {
                // Retain a complete numerical root as an explicit diagnostic
                // rather than fabricating a phase transfer from malformed
                // incidence. Projection rendering can still show the other
                // independently valid contour components.
                junction.kind = StableContourJunctionKind::Degenerate;
                diagnostics.contour_transfer_incidence_failures += 1;
                continue;
            }
        };
        let incident = half_edges
            .iter()
            .filter(|edge| edge.junction == junction.id)
            .collect::<Vec<_>>();
        let first = incident
            .iter()
            .filter(|edge| edge.phase == expected[0])
            .count();
        let second = incident
            .iter()
            .filter(|edge| edge.phase == expected[1])
            .count();
        let foreign = incident
            .iter()
            .any(|edge| edge.phase != expected[0] && edge.phase != expected[1]);
        if foreign || first != 1 || second != 1 {
            // A sampled phase-local path can legitimately fail to reach a
            // continuously verified root at this resolution. It is not a
            // stable-boundary corruption, so expose a typed degenerate event
            // and preserve the remaining raw contour graph. Callers that need
            // strict validation can reject this classification explicitly.
            junction.kind = StableContourJunctionKind::Degenerate;
            diagnostics.contour_transfer_incidence_failures += 1;
        }
    }
    Ok(())
}
