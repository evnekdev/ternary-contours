use std::collections::BTreeMap;

use crate::{GridVertexId, TernaryCoordinate, simplex::global_gradient_ab};

use super::{
    StableContourDiagnostics, StableContourError, StableContourJunctionKind, StableContourQuantity,
    StablePhaseId,
    clip::{close, interpolate},
    partition::{UmbrellaStableCell, point_from_barycentric},
    sample::{UmbrellaSamples, dot},
};

#[derive(Clone, Debug)]
pub(crate) struct LocalEndpoint {
    pub point: TernaryCoordinate,
    pub tied_phases: Vec<StablePhaseId>,
    pub junction_kind: Option<StableContourJunctionKind>,
    pub source: EndpointSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EndpointSource {
    UmbrellaEdge { edge: (GridVertexId, GridVertexId) },
    StableBoundary,
    Invariant,
    Interior,
}

#[derive(Clone, Debug)]
pub(crate) struct LocalStableSegment {
    pub phase: StablePhaseId,
    pub triangle: usize,
    pub start: LocalEndpoint,
    pub end: LocalEndpoint,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalParameterEvent {
    pub point: [f64; 3],
    pub parameter: f64,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn extract_level_segments(
    cells: &[UmbrellaStableCell],
    samples: &UmbrellaSamples,
    phase_ids: &[StablePhaseId],
    quantity: StableContourQuantity,
    level: f64,
    value_tolerance: f64,
    stability_tolerance: f64,
    geometry_tolerance: f64,
    parameter_tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<Vec<LocalStableSegment>, StableContourError> {
    let edge_owners = edge_owners(samples)?;
    let mut segments = Vec::new();
    for cell in cells {
        for polygon in &cell.polygons {
            let target_values = match quantity {
                StableContourQuantity::Height => {
                    samples.triangle_height_values(polygon.phase_index, cell.triangle.vertices)
                }
                StableContourQuantity::Secondary => samples
                    .triangle_secondary_values(polygon.phase_index, cell.triangle.vertices)
                    .expect("secondary samples exist in secondary mode"),
            };
            let minimum = target_values.into_iter().fold(f64::INFINITY, f64::min);
            let maximum = target_values.into_iter().fold(f64::NEG_INFINITY, f64::max);
            if level < minimum - value_tolerance || level > maximum + value_tolerance {
                continue;
            }
            let intersection = intersect_polygon(
                cell,
                &polygon.barycentric,
                target_values,
                level,
                value_tolerance,
                geometry_tolerance,
                parameter_tolerance,
                &edge_owners,
                polygon.phase,
                phase_ids,
                samples,
                stability_tolerance,
                diagnostics,
            )?;
            let Some([start_barycentric, end_barycentric]) = intersection else {
                continue;
            };
            if close(start_barycentric, end_barycentric, geometry_tolerance) {
                diagnostics.isolated_target_points += 1;
                continue;
            }
            let start = endpoint(
                cell,
                start_barycentric,
                polygon.phase,
                phase_ids,
                samples,
                quantity,
                stability_tolerance,
                geometry_tolerance,
            );
            let end = endpoint(
                cell,
                end_barycentric,
                polygon.phase,
                phase_ids,
                samples,
                quantity,
                stability_tolerance,
                geometry_tolerance,
            );
            segments.push(LocalStableSegment {
                phase: polygon.phase,
                triangle: cell.triangle.id,
                start,
                end,
            });
        }
    }
    segments.sort_by(|left, right| {
        left.phase
            .cmp(&right.phase)
            .then_with(|| left.triangle.cmp(&right.triangle))
            .then_with(|| lex_cmp(left.start.point, right.start.point))
            .then_with(|| lex_cmp(left.end.point, right.end.point))
    });
    diagnostics.local_stable_segments += segments.len();
    Ok(segments)
}

#[allow(clippy::too_many_arguments)]
fn intersect_polygon(
    cell: &UmbrellaStableCell,
    polygon: &[[f64; 3]],
    target_values: [f64; 3],
    level: f64,
    value_tolerance: f64,
    geometry_tolerance: f64,
    parameter_tolerance: f64,
    edge_owners: &BTreeMap<(GridVertexId, GridVertexId), usize>,
    owner_phase: StablePhaseId,
    phase_ids: &[StablePhaseId],
    samples: &UmbrellaSamples,
    stability_tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<Option<[[f64; 3]; 2]>, StableContourError> {
    let scalar_values: Vec<_> = polygon
        .iter()
        .copied()
        .map(|point| dot(target_values, point))
        .collect();
    if scalar_values
        .iter()
        .all(|value| (*value - level).abs() <= value_tolerance)
    {
        return coincident_error(
            cell,
            polygon[0],
            polygon[1],
            level,
            owner_phase,
            phase_ids,
            samples,
            stability_tolerance,
            diagnostics,
        );
    }
    let tangent = line_tangent(cell, target_values)?;

    let mut crossings = Vec::with_capacity(4);
    for edge_index in 0..polygon.len() {
        let next = (edge_index + 1) % polygon.len();
        let left = polygon[edge_index];
        let right = polygon[next];
        let left_difference = scalar_values[edge_index] - level;
        let right_difference = scalar_values[next] - level;
        let left_on = left_difference.abs() <= value_tolerance;
        let right_on = right_difference.abs() <= value_tolerance;
        if left_on && right_on {
            let midpoint = interpolate(left, right, 0.5);
            if tied_phases(cell, midpoint, phase_ids, samples, stability_tolerance).len() < 2
                && let Some(edge) = umbrella_edge(cell, midpoint, geometry_tolerance)
            {
                if edge_owners.get(&edge).copied() == Some(cell.triangle.id) {
                    return ordered_pair(
                        cell,
                        left,
                        right,
                        tangent,
                        geometry_tolerance,
                        parameter_tolerance,
                        owner_phase,
                    )
                    .map(Some);
                }
                return Ok(None);
            }
            return coincident_error(
                cell,
                left,
                right,
                level,
                owner_phase,
                phase_ids,
                samples,
                stability_tolerance,
                diagnostics,
            );
        }
        if left_on {
            push_unique(&mut crossings, left, geometry_tolerance);
        }
        if right_on {
            push_unique(&mut crossings, right, geometry_tolerance);
        }
        if !left_on
            && !right_on
            && left_difference.is_sign_positive() != right_difference.is_sign_positive()
        {
            let denominator = left_difference - right_difference;
            if denominator == 0.0 || !denominator.is_finite() {
                return Err(StableContourError::NonFiniteStableGeometry {
                    triangle: cell.triangle.id,
                });
            }
            let Some(parameter) =
                bounded_edge_root(left_difference, right_difference, parameter_tolerance)
            else {
                continue;
            };
            push_unique(
                &mut crossings,
                interpolate(left, right, parameter),
                geometry_tolerance,
            );
        }
    }
    if crossings.is_empty() {
        return Ok(None);
    }
    let raw_events = crossings
        .into_iter()
        .map(|point| LocalParameterEvent {
            parameter: line_parameter(cell, point, tangent),
            point,
        })
        .collect();
    let crossings = forward_events(
        raw_events,
        geometry_tolerance,
        parameter_tolerance,
        cell.triangle.id,
        owner_phase,
    )?;
    if crossings.len() == 1 {
        diagnostics.isolated_target_points += 1;
        return Ok(None);
    }
    Ok(Some([
        crossings[0].point,
        crossings[crossings.len() - 1].point,
    ]))
}

pub(crate) fn bounded_edge_root(
    left_difference: f64,
    right_difference: f64,
    parameter_tolerance: f64,
) -> Option<f64> {
    let denominator = left_difference - right_difference;
    if denominator == 0.0 || !denominator.is_finite() {
        return None;
    }
    let parameter = left_difference / denominator;
    parameter
        .is_finite()
        .then_some(parameter)
        .filter(|parameter| (-parameter_tolerance..=1.0 + parameter_tolerance).contains(parameter))
        .map(|parameter| parameter.clamp(0.0, 1.0))
}

fn ordered_pair(
    cell: &UmbrellaStableCell,
    left: [f64; 3],
    right: [f64; 3],
    tangent: [f64; 2],
    geometry_tolerance: f64,
    parameter_tolerance: f64,
    phase: StablePhaseId,
) -> Result<[[f64; 3]; 2], StableContourError> {
    let events = forward_events(
        vec![
            LocalParameterEvent {
                point: left,
                parameter: line_parameter(cell, left, tangent),
            },
            LocalParameterEvent {
                point: right,
                parameter: line_parameter(cell, right, tangent),
            },
        ],
        geometry_tolerance,
        parameter_tolerance,
        cell.triangle.id,
        phase,
    )?;
    if events.len() != 2 {
        return Err(StableContourError::NonMonotoneLocalEvents {
            triangle: cell.triangle.id,
            phase,
            previous_parameter: events.first().map_or(0.0, |event| event.parameter),
            next_parameter: events.last().map_or(0.0, |event| event.parameter),
        });
    }
    Ok([events[0].point, events[1].point])
}

pub(crate) fn forward_events(
    mut events: Vec<LocalParameterEvent>,
    geometry_tolerance: f64,
    parameter_tolerance: f64,
    triangle: usize,
    phase: StablePhaseId,
) -> Result<Vec<LocalParameterEvent>, StableContourError> {
    events.sort_by(|left, right| {
        left.parameter
            .total_cmp(&right.parameter)
            .then_with(|| barycentric_lex_cmp(left.point, right.point))
    });
    let mut accepted: Vec<LocalParameterEvent> = Vec::with_capacity(events.len());
    for event in events {
        if !event.parameter.is_finite() || event.point.into_iter().any(|value| !value.is_finite()) {
            return Err(StableContourError::NonFiniteStableGeometry { triangle });
        }
        if let Some(previous) = accepted.last()
            && event.parameter <= previous.parameter + parameter_tolerance
        {
            if close(previous.point, event.point, geometry_tolerance) {
                continue;
            }
            return Err(StableContourError::NonMonotoneLocalEvents {
                triangle,
                phase,
                previous_parameter: previous.parameter,
                next_parameter: event.parameter,
            });
        }
        accepted.push(event);
    }
    Ok(accepted)
}

fn line_tangent(
    cell: &UmbrellaStableCell,
    target_values: [f64; 3],
) -> Result<[f64; 2], StableContourError> {
    let gradient = global_gradient_ab(
        cell.vertices,
        [
            target_values[0] - target_values[2],
            target_values[1] - target_values[2],
        ],
    )
    .ok_or(StableContourError::NonFiniteStableGeometry {
        triangle: cell.triangle.id,
    })?;
    let norm = gradient[0].hypot(gradient[1]);
    if !norm.is_finite() || norm == 0.0 {
        return Err(StableContourError::NonFiniteStableGeometry {
            triangle: cell.triangle.id,
        });
    }
    let mut tangent = [gradient[1] / norm, -gradient[0] / norm];
    if tangent[0] < 0.0 || (tangent[0] == 0.0 && tangent[1] < 0.0) {
        tangent = [-tangent[0], -tangent[1]];
    }
    Ok(tangent)
}

fn line_parameter(cell: &UmbrellaStableCell, barycentric: [f64; 3], tangent: [f64; 2]) -> f64 {
    let [a, b, _] = point_from_barycentric(cell, barycentric).as_array();
    tangent[0] * a + tangent[1] * b
}

#[allow(clippy::too_many_arguments)]
fn coincident_error(
    cell: &UmbrellaStableCell,
    start: [f64; 3],
    end: [f64; 3],
    level: f64,
    owner_phase: StablePhaseId,
    phase_ids: &[StablePhaseId],
    samples: &UmbrellaSamples,
    stability_tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<Option<[[f64; 3]; 2]>, StableContourError> {
    diagnostics.coincident_tie_segments += 1;
    let midpoint = interpolate(start, end, 0.5);
    let mut phases = tied_phases(cell, midpoint, phase_ids, samples, stability_tolerance);
    if phases.is_empty() {
        phases.push(owner_phase);
    }
    Err(StableContourError::CoincidentTargetSegment {
        level,
        triangle: cell.triangle.id,
        phases,
        start: point_from_barycentric(cell, start),
        end: point_from_barycentric(cell, end),
    })
}

#[allow(clippy::too_many_arguments)]
fn endpoint(
    cell: &UmbrellaStableCell,
    barycentric: [f64; 3],
    owner_phase: StablePhaseId,
    phase_ids: &[StablePhaseId],
    samples: &UmbrellaSamples,
    quantity: StableContourQuantity,
    stability_tolerance: f64,
    geometry_tolerance: f64,
) -> LocalEndpoint {
    let tied_phases = tied_phases(cell, barycentric, phase_ids, samples, stability_tolerance);
    debug_assert!(tied_phases.contains(&owner_phase));
    let junction_kind = if tied_phases.len() >= 2 {
        match quantity {
            StableContourQuantity::Height if tied_phases.len() == 2 => {
                Some(StableContourJunctionKind::Univariant)
            }
            StableContourQuantity::Height => Some(StableContourJunctionKind::Invariant),
            StableContourQuantity::Secondary => {
                Some(StableContourJunctionKind::StableBoundaryContact)
            }
        }
    } else {
        None
    };
    let source = if tied_phases.len() >= 3 {
        EndpointSource::Invariant
    } else if tied_phases.len() == 2 {
        EndpointSource::StableBoundary
    } else if let Some(edge) = umbrella_edge(cell, barycentric, geometry_tolerance) {
        EndpointSource::UmbrellaEdge { edge }
    } else {
        EndpointSource::Interior
    };
    LocalEndpoint {
        point: point_from_barycentric(cell, barycentric),
        tied_phases,
        junction_kind,
        source,
    }
}

fn tied_phases(
    cell: &UmbrellaStableCell,
    barycentric: [f64; 3],
    phase_ids: &[StablePhaseId],
    samples: &UmbrellaSamples,
    tolerance: f64,
) -> Vec<StablePhaseId> {
    let values: Vec<_> = (0..samples.phase_count)
        .map(|phase| {
            dot(
                samples.triangle_height_values(phase, cell.triangle.vertices),
                barycentric,
            )
        })
        .collect();
    let maximum = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    phase_ids
        .iter()
        .copied()
        .zip(values)
        .filter(|(_, value)| *value >= maximum - tolerance)
        .map(|(phase, _)| phase)
        .collect()
}

fn umbrella_edge(
    cell: &UmbrellaStableCell,
    barycentric: [f64; 3],
    tolerance: f64,
) -> Option<(GridVertexId, GridVertexId)> {
    let opposite = barycentric
        .iter()
        .position(|weight| weight.abs() <= tolerance)?;
    let (left, right) = match opposite {
        0 => (cell.triangle.vertices[1], cell.triangle.vertices[2]),
        1 => (cell.triangle.vertices[2], cell.triangle.vertices[0]),
        2 => (cell.triangle.vertices[0], cell.triangle.vertices[1]),
        _ => unreachable!(),
    };
    Some(edge_key(left, right))
}

fn edge_owners(
    samples: &UmbrellaSamples,
) -> Result<BTreeMap<(GridVertexId, GridVertexId), usize>, StableContourError> {
    let mut owners = BTreeMap::new();
    for triangle in samples.grid.elementary_triangles()? {
        for (left, right) in [(0, 1), (1, 2), (2, 0)] {
            owners
                .entry(edge_key(triangle.vertices[left], triangle.vertices[right]))
                .or_insert(triangle.id);
        }
    }
    Ok(owners)
}

fn edge_key(left: GridVertexId, right: GridVertexId) -> (GridVertexId, GridVertexId) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

fn push_unique(points: &mut Vec<[f64; 3]>, point: [f64; 3], tolerance: f64) {
    if points
        .iter()
        .all(|existing| !close(*existing, point, tolerance))
    {
        points.push(point);
    }
}

fn barycentric_lex_cmp(left: [f64; 3], right: [f64; 3]) -> std::cmp::Ordering {
    for (left, right) in left.into_iter().zip(right) {
        let order = left.total_cmp(&right);
        if !order.is_eq() {
            return order;
        }
    }
    std::cmp::Ordering::Equal
}

pub(crate) fn lex_cmp(left: TernaryCoordinate, right: TernaryCoordinate) -> std::cmp::Ordering {
    for (left, right) in left.as_array().into_iter().zip(right.as_array()) {
        let order = left.total_cmp(&right);
        if !order.is_eq() {
            return order;
        }
    }
    std::cmp::Ordering::Equal
}

pub(crate) fn points_close(
    left: TernaryCoordinate,
    right: TernaryCoordinate,
    tolerance: f64,
) -> bool {
    left.as_array()
        .into_iter()
        .zip(right.as_array())
        .all(|(left, right)| (left - right).abs() <= tolerance)
}
