use std::collections::{BTreeMap, BTreeSet};

use crate::{TernaryCoordinate, simplex::logical_from_composition};

use super::{
    StableContourDiagnostics, StableContourError, StableContourHalfEdge, StableContourHalfEdgeId,
    StableContourJunction, StableContourJunctionKind, StableContourPath,
    StableContourPathGeometryState, StableContourQuantity, StableJunctionId, StablePhaseId,
    segments::{EndpointSource, LocalEndpoint, LocalStableSegment, lex_cmp, points_close},
};

#[derive(Clone)]
struct JunctionCandidate {
    point: TernaryCoordinate,
    phases: Vec<StablePhaseId>,
    kind: StableContourJunctionKind,
}

#[derive(Clone, Copy)]
struct WorkEndpoint {
    point: TernaryCoordinate,
    junction: Option<StableJunctionId>,
    break_path: bool,
}

#[derive(Clone, Copy)]
struct WorkSegment {
    triangle: usize,
    start: WorkEndpoint,
    end: WorkEndpoint,
}

#[derive(Clone, Copy)]
struct Node {
    point: TernaryCoordinate,
    junction: Option<StableJunctionId>,
    break_path: bool,
}

pub(crate) fn assemble_level(
    segments: Vec<LocalStableSegment>,
    quantity: StableContourQuantity,
    level: f64,
    tolerance: f64,
    parameter_tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<(Vec<StableContourPath>, Vec<StableContourJunction>), StableContourError> {
    let junctions = build_junctions(&segments, quantity, level, tolerance)?;
    assemble_level_with_junctions_impl(
        segments,
        level,
        tolerance,
        parameter_tolerance,
        junctions,
        None,
        diagnostics,
    )
}

/// Assemble sampled phase-local contour segments using continuously verified
/// stable-boundary junctions.  `sampling_subdivisions` supplies only a bounded
/// attachment search radius; root identity remains branch+phase-pair+full
/// precision composition.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble_level_with_junctions(
    segments: Vec<LocalStableSegment>,
    _quantity: StableContourQuantity,
    level: f64,
    tolerance: f64,
    parameter_tolerance: f64,
    junctions: Vec<StableContourJunction>,
    sampling_subdivisions: usize,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<(Vec<StableContourPath>, Vec<StableContourJunction>), StableContourError> {
    let attachment = tolerance.max(1.5 / sampling_subdivisions.max(1) as f64);
    assemble_level_with_junctions_impl(
        segments,
        level,
        tolerance,
        parameter_tolerance,
        junctions,
        Some(attachment),
        diagnostics,
    )
}

fn assemble_level_with_junctions_impl(
    segments: Vec<LocalStableSegment>,
    level: f64,
    tolerance: f64,
    parameter_tolerance: f64,
    junctions: Vec<StableContourJunction>,
    attachment_tolerance: Option<f64>,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<(Vec<StableContourPath>, Vec<StableContourJunction>), StableContourError> {
    let mut by_phase = BTreeMap::<StablePhaseId, Vec<WorkSegment>>::new();
    for segment in segments {
        let start = work_endpoint(
            &segment.start,
            &junctions,
            attachment_tolerance.unwrap_or(tolerance),
            attachment_tolerance.is_some(),
        );
        let end = work_endpoint(
            &segment.end,
            &junctions,
            attachment_tolerance.unwrap_or(tolerance),
            attachment_tolerance.is_some(),
        );
        if !points_close(start.point, end.point, tolerance) {
            by_phase
                .entry(segment.phase)
                .or_default()
                .push(WorkSegment {
                    triangle: segment.triangle,
                    start,
                    end,
                });
        }
    }

    let mut paths = Vec::new();
    for (phase, phase_segments) in by_phase {
        paths.extend(assemble_phase(
            phase,
            phase_segments,
            level,
            tolerance,
            parameter_tolerance,
            diagnostics,
        )?);
    }
    paths.sort_by(|left, right| {
        left.phase
            .cmp(&right.phase)
            .then_with(|| lex_cmp(left.points[0], right.points[0]))
            .then_with(|| left.points.len().cmp(&right.points.len()))
    });
    diagnostics.phase_labelled_paths += paths.len();
    diagnostics.closed_paths += paths.iter().filter(|path| path.closed).count();
    diagnostics.open_paths += paths.iter().filter(|path| !path.closed).count();
    for junction in &junctions {
        match junction.kind {
            StableContourJunctionKind::Univariant => diagnostics.univariant_junctions += 1,
            StableContourJunctionKind::Invariant => diagnostics.invariant_junctions += 1,
            StableContourJunctionKind::StableBoundaryContact
            | StableContourJunctionKind::OneSidedSecondaryContact => {
                diagnostics.stable_boundary_contacts += 1;
            }
            StableContourJunctionKind::RegularTransfer => diagnostics.univariant_junctions += 1,
            StableContourJunctionKind::InvariantLevelCoincidence => {
                diagnostics.invariant_junctions += 1
            }
            StableContourJunctionKind::TangentBoundaryContact
            | StableContourJunctionKind::DomainTruncated
            | StableContourJunctionKind::Degenerate => {}
        }
    }
    Ok((paths, junctions))
}

fn build_junctions(
    segments: &[LocalStableSegment],
    quantity: StableContourQuantity,
    level: f64,
    tolerance: f64,
) -> Result<Vec<StableContourJunction>, StableContourError> {
    let mut candidates: Vec<JunctionCandidate> = Vec::new();
    for endpoint in segments
        .iter()
        .flat_map(|segment| [&segment.start, &segment.end])
        .filter(|endpoint| endpoint.junction_kind.is_some())
    {
        if let Some(existing) = candidates
            .iter_mut()
            .find(|candidate| points_close(candidate.point, endpoint.point, tolerance))
        {
            if lex_cmp(endpoint.point, existing.point).is_lt() {
                existing.point = endpoint.point;
            }
            existing.phases.extend(endpoint.tied_phases.iter().copied());
            existing.phases.sort_unstable();
            existing.phases.dedup();
            existing.kind = junction_kind(quantity, existing.phases.len());
        } else {
            candidates.push(JunctionCandidate {
                point: endpoint.point,
                phases: endpoint.tied_phases.clone(),
                kind: endpoint.junction_kind.unwrap(),
            });
        }
    }
    if quantity == StableContourQuantity::Height
        && let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.phases.len() > 3)
    {
        return Err(StableContourError::OverdeterminedInvariantLevel {
            level,
            point: candidate.point,
            phases: candidate.phases.clone(),
        });
    }
    candidates.sort_by(|left, right| {
        lex_cmp(left.point, right.point).then_with(|| left.phases.cmp(&right.phases))
    });
    Ok(candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| StableContourJunction {
            id: StableJunctionId(index),
            point: candidate.point,
            phases: candidate.phases,
            kind: candidate.kind,
            branch: None,
            invariant: None,
            verification: None,
        })
        .collect())
}

fn junction_kind(
    quantity: StableContourQuantity,
    tied_phase_count: usize,
) -> StableContourJunctionKind {
    match quantity {
        StableContourQuantity::Height if tied_phase_count == 2 => {
            StableContourJunctionKind::Univariant
        }
        StableContourQuantity::Height => StableContourJunctionKind::Invariant,
        StableContourQuantity::Secondary => StableContourJunctionKind::StableBoundaryContact,
    }
}

fn work_endpoint(
    endpoint: &LocalEndpoint,
    junctions: &[StableContourJunction],
    tolerance: f64,
    continuous_junctions: bool,
) -> WorkEndpoint {
    let junction = endpoint.junction_kind.and_then(|_| {
        let matches = junctions
            .iter()
            .filter(|junction| {
                points_close(junction.point, endpoint.point, tolerance)
                    && endpoint
                        .tied_phases
                        .iter()
                        .all(|phase| junction.phases.contains(phase))
            })
            .map(|junction| junction.id)
            .collect::<Vec<_>>();
        // A broad sampling-cell search may locate candidates, but it is never
        // final root identity.  Refuse to bind an affine endpoint when more
        // than one continuously verified root is compatible; this prevents
        // close roots in one cell from being silently merged by insertion
        // order or a geometric bucket.
        if matches.len() == 1 {
            Some(matches[0])
        } else {
            None
        }
    });
    let point = junction
        .map(|id| junctions[id.0].point)
        .unwrap_or(endpoint.point);
    let source_break = if continuous_junctions {
        false
    } else {
        match endpoint.source {
            EndpointSource::StableBoundary | EndpointSource::Invariant => true,
            EndpointSource::SamplingEdge { edge } => {
                let _canonical_edge = edge;
                false
            }
            EndpointSource::Interior => false,
        }
    };
    WorkEndpoint {
        point,
        junction,
        break_path: junction.is_some() || source_break,
    }
}

fn assemble_phase(
    phase: StablePhaseId,
    segments: Vec<WorkSegment>,
    level: f64,
    tolerance: f64,
    parameter_tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<Vec<StableContourPath>, StableContourError> {
    let segments = canonicalize_physical_edges(phase, segments, level, tolerance, diagnostics)?;
    let mut nodes = Vec::<Node>::new();
    let mut buckets = BTreeMap::<[i64; 3], Vec<usize>>::new();
    let mut edges = Vec::<(usize, usize, usize)>::new();
    for segment in segments {
        let start = node_for(&mut nodes, &mut buckets, segment.start, tolerance);
        let end = node_for(&mut nodes, &mut buckets, segment.end, tolerance);
        if start == end {
            diagnostics.path_assembly_ambiguities += 1;
            return Err(StableContourError::DirectedTraversalCycle {
                level,
                phase,
                triangle: segment.triangle,
            });
        }
        edges.push((start, end, segment.triangle));
    }
    let mut adjacency = vec![Vec::<(usize, usize)>::new(); nodes.len()];
    for (edge_index, &(left, right, _)) in edges.iter().enumerate() {
        adjacency[left].push((right, edge_index));
        adjacency[right].push((left, edge_index));
    }
    if let Some(degree) = adjacency.iter().map(Vec::len).find(|degree| *degree > 2) {
        diagnostics.path_assembly_ambiguities += 1;
        return Err(StableContourError::AmbiguousPathAssembly {
            level,
            phase,
            degree,
        });
    }

    let mut used = vec![false; edges.len()];
    let mut paths = Vec::new();
    let mut starts: Vec<_> = adjacency
        .iter()
        .enumerate()
        .filter(|(_, adjacent)| adjacent.len() == 1)
        .map(|(index, _)| index)
        .collect();
    starts.sort_by(|&left, &right| lex_cmp(nodes[left].point, nodes[right].point));
    for start in starts {
        if adjacency[start].iter().all(|(_, edge)| used[*edge]) {
            continue;
        }
        paths.push(walk(
            phase,
            start,
            &nodes,
            &edges,
            &adjacency,
            &mut used,
            false,
            level,
            parameter_tolerance,
            diagnostics,
        )?);
    }
    while let Some(edge_index) = used.iter().position(|used| !*used) {
        let (left, right, _) = edges[edge_index];
        let start = if lex_cmp(nodes[left].point, nodes[right].point).is_le() {
            left
        } else {
            right
        };
        paths.push(walk(
            phase,
            start,
            &nodes,
            &edges,
            &adjacency,
            &mut used,
            true,
            level,
            parameter_tolerance,
            diagnostics,
        )?);
    }
    Ok(paths)
}

/// Canonicalize producer-oriented segment records into unique undirected
/// physical contour edges. Stable contour extraction occurs independently in
/// neighbouring sampling cells, so the same continuous edge may legitimately
/// arrive in either direction. Producer orientation is therefore not evidence
/// of a path retrace.
fn canonicalize_physical_edges(
    phase: StablePhaseId,
    segments: Vec<WorkSegment>,
    level: f64,
    tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<Vec<WorkSegment>, StableContourError> {
    let mut unique = Vec::<WorkSegment>::new();
    for segment in segments {
        diagnostics.physical_contour_segments_emitted += 1;
        let coincident = unique
            .iter()
            .position(|existing| physical_edge_geometry_matches(*existing, segment, tolerance));
        let Some(index) = coincident else {
            unique.push(segment);
            continue;
        };
        let existing = unique[index];
        if physical_edge_semantics_match(existing, segment, tolerance) {
            diagnostics.reverse_compatible_contour_duplicates_merged += 1;
            continue;
        }
        diagnostics.path_assembly_ambiguities += 1;
        diagnostics.incompatible_coincident_contour_edges += 1;
        return Err(StableContourError::IncompatiblePhysicalContourEdge {
            context: Box::new(super::IncompatiblePhysicalContourEdgeContext {
                level,
                phase,
                triangle: segment.triangle,
                existing_triangle: existing.triangle,
                start: segment.start.point,
                end: segment.end.point,
                existing_start: existing.start.point,
                existing_end: existing.end.point,
                start_junction: segment.start.junction,
                end_junction: segment.end.junction,
                existing_start_junction: existing.start.junction,
                existing_end_junction: existing.end.junction,
            }),
        });
    }
    Ok(unique)
}

fn physical_edge_geometry_matches(left: WorkSegment, right: WorkSegment, tolerance: f64) -> bool {
    (points_close(left.start.point, right.start.point, tolerance)
        && points_close(left.end.point, right.end.point, tolerance))
        || (points_close(left.start.point, right.end.point, tolerance)
            && points_close(left.end.point, right.start.point, tolerance))
}

fn physical_edge_semantics_match(left: WorkSegment, right: WorkSegment, tolerance: f64) -> bool {
    let endpoint_matches = |left: WorkEndpoint, right: WorkEndpoint| {
        left.junction == right.junction && left.break_path == right.break_path
    };
    if points_close(left.start.point, right.start.point, tolerance)
        && points_close(left.end.point, right.end.point, tolerance)
    {
        endpoint_matches(left.start, right.start) && endpoint_matches(left.end, right.end)
    } else {
        endpoint_matches(left.start, right.end) && endpoint_matches(left.end, right.start)
    }
}

#[allow(clippy::too_many_arguments)]
fn walk(
    phase: StablePhaseId,
    start: usize,
    nodes: &[Node],
    edges: &[(usize, usize, usize)],
    adjacency: &[Vec<(usize, usize)>],
    used: &mut [bool],
    expect_closed: bool,
    level: f64,
    parameter_tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<StableContourPath, StableContourError> {
    let mut node_path = vec![start];
    let mut current = start;
    let mut previous: Option<usize> = None;
    let mut directed_states = BTreeSet::new();
    loop {
        let next = adjacency[current]
            .iter()
            .filter(|(_, edge)| !used[*edge])
            .min_by(|(left, _), (right, _)| lex_cmp(nodes[*left].point, nodes[*right].point))
            .copied();
        let Some((node, edge)) = next else { break };
        if let Some(previous_node) = previous
            // A continuously corrected transfer junction may move off the
            // sampled endpoint that seeded it. The final local segment can
            // then appear to turn backwards in affine sampling geometry even
            // though it terminates at a verified phase-transfer event. Its
            // semantic endpoint, not producer orientation, is authoritative.
            && nodes[current].junction.is_none()
            && nodes[node].junction.is_none()
            && forward_tangent_alignment(
                nodes[previous_node].point,
                nodes[current].point,
                nodes[node].point,
            ) <= parameter_tolerance
        {
            diagnostics.path_assembly_ambiguities += 1;
            return Err(StableContourError::NonForwardPathAssembly {
                context: Box::new(super::NonForwardPathAssemblyContext {
                    level,
                    phase,
                    point: nodes[current].point,
                    previous: Some(nodes[previous_node].point),
                    next: Some(nodes[node].point),
                    triangle: Some(edges[edge].2),
                    previous_junction: nodes[previous_node].junction,
                    current_junction: nodes[current].junction,
                    next_junction: nodes[node].junction,
                }),
            });
        }
        if !directed_states.insert((current, node)) {
            diagnostics.path_assembly_ambiguities += 1;
            return Err(StableContourError::DirectedTraversalCycle {
                level,
                phase,
                triangle: edges[edge].2,
            });
        }
        used[edge] = true;
        previous = Some(current);
        current = node;
        if current == start {
            break;
        }
        node_path.push(current);
        if node_path.len() > used.len() + 1 {
            diagnostics.path_assembly_ambiguities += 1;
            return Err(StableContourError::DirectedTraversalCycle {
                level,
                phase,
                triangle: edges[edge].2,
            });
        }
    }
    let closed = current == start;
    if expect_closed != closed || (closed && node_path.len() < 3) || node_path.len() < 2 {
        diagnostics.path_assembly_ambiguities += 1;
        return Err(StableContourError::AmbiguousPathAssembly {
            level,
            phase,
            degree: adjacency[current].len(),
        });
    }
    if !closed && lex_cmp(nodes[*node_path.last().unwrap()].point, nodes[start].point).is_lt() {
        node_path.reverse();
    }
    if closed {
        let minimum = (0..node_path.len())
            .min_by(|&left, &right| {
                lex_cmp(nodes[node_path[left]].point, nodes[node_path[right]].point)
            })
            .unwrap();
        node_path.rotate_left(minimum);
        if node_path.len() > 2
            && lex_cmp(
                nodes[node_path[node_path.len() - 1]].point,
                nodes[node_path[1]].point,
            )
            .is_lt()
        {
            node_path[1..].reverse();
        }
    }
    let points = node_path
        .iter()
        .map(|&node| nodes[node].point)
        .collect::<Vec<_>>();
    let start_junction = (!closed).then(|| nodes[node_path[0]].junction).flatten();
    let end_junction = (!closed)
        .then(|| nodes[*node_path.last().unwrap()].junction)
        .flatten();
    Ok(StableContourPath {
        phase,
        points,
        closed,
        start_junction,
        end_junction,
        geometry_state: StableContourPathGeometryState::Raw,
    })
}

/// Build stable, phase-labelled endpoint incidence records after path assembly.
/// A path owns at most one half-edge at each non-closed end.  Consumers can
/// validate cross-phase transfer semantics without guessing from coordinates.
pub(crate) fn half_edges(paths: &[StableContourPath]) -> Vec<StableContourHalfEdge> {
    let mut edges = Vec::new();
    for (path_index, path) in paths.iter().enumerate() {
        if path.closed {
            continue;
        }
        if let Some(junction) = path.start_junction {
            edges.push(StableContourHalfEdge {
                id: StableContourHalfEdgeId(edges.len()),
                phase: path.phase,
                path_index,
                at_start: true,
                junction,
            });
        }
        if let Some(junction) = path.end_junction {
            edges.push(StableContourHalfEdge {
                id: StableContourHalfEdgeId(edges.len()),
                phase: path.phase,
                path_index,
                at_start: false,
                junction,
            });
        }
    }
    edges.sort_by(|left, right| {
        left.junction
            .cmp(&right.junction)
            .then_with(|| left.phase.cmp(&right.phase))
            .then_with(|| left.path_index.cmp(&right.path_index))
            .then_with(|| left.at_start.cmp(&right.at_start))
    });
    for (index, edge) in edges.iter_mut().enumerate() {
        edge.id = StableContourHalfEdgeId(index);
    }
    edges
}

fn forward_tangent_alignment(
    previous: TernaryCoordinate,
    current: TernaryCoordinate,
    next: TernaryCoordinate,
) -> f64 {
    let previous = logical_from_composition(previous.as_array());
    let current = logical_from_composition(current.as_array());
    let next = logical_from_composition(next.as_array());
    let incoming = [current[0] - previous[0], current[1] - previous[1]];
    let outgoing = [next[0] - current[0], next[1] - current[1]];
    let incoming_norm = incoming[0].hypot(incoming[1]);
    let outgoing_norm = outgoing[0].hypot(outgoing[1]);
    if incoming_norm == 0.0 || outgoing_norm == 0.0 {
        return f64::NEG_INFINITY;
    }
    (incoming[0] * outgoing[0] + incoming[1] * outgoing[1]) / (incoming_norm * outgoing_norm)
}

fn node_for(
    nodes: &mut Vec<Node>,
    buckets: &mut BTreeMap<[i64; 3], Vec<usize>>,
    endpoint: WorkEndpoint,
    tolerance: f64,
) -> usize {
    if endpoint.break_path {
        let index = nodes.len();
        nodes.push(Node {
            point: endpoint.point,
            junction: endpoint.junction,
            break_path: true,
        });
        return index;
    }
    let key = bucket_key(endpoint.point, tolerance);
    let mut matched = None;
    for da in -1_i64..=1 {
        for db in -1_i64..=1 {
            for dc in -1_i64..=1 {
                let neighbour = [
                    key[0].saturating_add(da),
                    key[1].saturating_add(db),
                    key[2].saturating_add(dc),
                ];
                if let Some(indices) = buckets.get(&neighbour) {
                    for &index in indices {
                        if !nodes[index].break_path
                            && points_close(nodes[index].point, endpoint.point, tolerance)
                        {
                            matched =
                                Some(matched.map_or(index, |current: usize| current.min(index)));
                        }
                    }
                }
            }
        }
    }
    if let Some(index) = matched {
        return index;
    }
    let index = nodes.len();
    nodes.push(Node {
        point: endpoint.point,
        junction: None,
        break_path: false,
    });
    buckets.entry(key).or_default().push(index);
    index
}

fn bucket_key(point: TernaryCoordinate, tolerance: f64) -> [i64; 3] {
    point.as_array().map(|component| {
        let scaled = (component / tolerance).round();
        if scaled <= i64::MIN as f64 {
            i64::MIN
        } else if scaled >= i64::MAX as f64 {
            i64::MAX
        } else {
            scaled as i64
        }
    })
}
