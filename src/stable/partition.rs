use std::collections::BTreeSet;

use crate::{GridTriangle, TernaryCoordinate};

use super::{
    StableContourDiagnostics, StableContourError, StablePhaseId,
    clip::{clip_half_plane, composition, polygon_area},
    sample::{UmbrellaSamples, dot},
};

#[derive(Clone, Debug)]
pub(crate) struct StablePhasePolygon {
    pub phase_index: usize,
    pub phase: StablePhaseId,
    pub barycentric: Vec<[f64; 3]>,
}

#[derive(Clone, Debug)]
pub(crate) struct UmbrellaStableCell {
    pub triangle: GridTriangle,
    pub vertices: [[f64; 3]; 3],
    pub polygons: Vec<StablePhasePolygon>,
}

#[derive(Clone, Copy)]
struct PhaseBounds {
    phase_index: usize,
    phase: StablePhaseId,
    values: [f64; 3],
    minimum: f64,
    maximum: f64,
}

pub(crate) fn build_stable_partition(
    samples: &UmbrellaSamples,
    phase_ids: &[StablePhaseId],
    stability_tolerance: f64,
    geometry_tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<Vec<UmbrellaStableCell>, StableContourError> {
    let triangles = samples.grid.elementary_triangles()?;
    let mut cells = Vec::with_capacity(triangles.len());
    for triangle in triangles {
        let vertices = triangle_vertices(samples, triangle)?;
        let bounds: Vec<_> = phase_ids
            .iter()
            .copied()
            .enumerate()
            .map(|(phase_index, phase)| {
                let values = samples.triangle_height_values(phase_index, triangle.vertices);
                PhaseBounds {
                    phase_index,
                    phase,
                    values,
                    minimum: values.into_iter().fold(f64::INFINITY, f64::min),
                    maximum: values.into_iter().fold(f64::NEG_INFINITY, f64::max),
                }
            })
            .collect();
        if bounds.iter().any(|bound| {
            !bound.minimum.is_finite()
                || !bound.maximum.is_finite()
                || bound.values.into_iter().any(|value| !value.is_finite())
        }) {
            return Err(StableContourError::NonFiniteStableGeometry {
                triangle: triangle.id,
            });
        }
        diagnostics.total_phase_triangle_candidates += bounds.len();
        let envelope_floor = bounds
            .iter()
            .map(|bound| bound.minimum)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut candidates: Vec<_> = bounds
            .iter()
            .copied()
            .filter(|bound| bound.maximum >= envelope_floor - stability_tolerance)
            .collect();
        diagnostics.phases_removed_by_envelope_floor += bounds.len() - candidates.len();
        candidates.sort_by(|left, right| {
            right
                .maximum
                .total_cmp(&left.maximum)
                .then_with(|| left.phase.cmp(&right.phase))
        });

        let mut polygons = Vec::new();
        let mut scratch = Vec::new();
        for owner in &candidates {
            if candidates.iter().any(|competitor| {
                competitor.phase_index != owner.phase_index
                    && competitor.minimum > owner.maximum + stability_tolerance
            }) {
                diagnostics.empty_stable_polygons += 1;
                continue;
            }
            let mut polygon = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            for competitor in &candidates {
                if competitor.phase_index == owner.phase_index {
                    continue;
                }
                if competitor.maximum < owner.minimum - stability_tolerance {
                    diagnostics.pair_comparisons_skipped_by_range += 1;
                    continue;
                }
                diagnostics.polygon_clipping_operations += 1;
                let differences = [
                    owner.values[0] - competitor.values[0],
                    owner.values[1] - competitor.values[1],
                    owner.values[2] - competitor.values[2],
                ];
                if !clip_half_plane(
                    &mut polygon,
                    &mut scratch,
                    differences,
                    stability_tolerance,
                    geometry_tolerance,
                ) {
                    return Err(StableContourError::StablePolygonFailure {
                        triangle: triangle.id,
                        phase: owner.phase,
                    });
                }
                if polygon.len() < 3 {
                    break;
                }
            }
            let area = polygon_area(vertices, &polygon);
            if !area.is_finite() {
                return Err(StableContourError::NonFiniteStableGeometry {
                    triangle: triangle.id,
                });
            }
            if polygon.len() < 3 || area <= geometry_tolerance * geometry_tolerance {
                diagnostics.empty_stable_polygons += 1;
                continue;
            }
            diagnostics.nonempty_stable_polygons += 1;
            if !wins_any_triangle_vertex(owner.phase_index, &bounds, stability_tolerance) {
                diagnostics.interior_stable_polygons_without_vertex_winner += 1;
            }
            polygons.push(StablePhasePolygon {
                phase_index: owner.phase_index,
                phase: owner.phase,
                barycentric: polygon,
            });
        }
        polygons.sort_by_key(|polygon| polygon.phase);
        reject_positive_area_ties(
            triangle,
            &candidates,
            &polygons,
            stability_tolerance,
            diagnostics,
        )?;
        collect_boundary_diagnostics(
            &polygons,
            &bounds,
            stability_tolerance,
            geometry_tolerance,
            diagnostics,
        );
        cells.push(UmbrellaStableCell {
            triangle,
            vertices,
            polygons,
        });
    }
    Ok(cells)
}

fn triangle_vertices(
    samples: &UmbrellaSamples,
    triangle: GridTriangle,
) -> Result<[[f64; 3]; 3], StableContourError> {
    Ok([
        samples.grid.composition(triangle.vertices[0])?,
        samples.grid.composition(triangle.vertices[1])?,
        samples.grid.composition(triangle.vertices[2])?,
    ])
}

fn wins_any_triangle_vertex(phase_index: usize, bounds: &[PhaseBounds], tolerance: f64) -> bool {
    (0..3).any(|vertex| {
        let maximum = bounds
            .iter()
            .map(|bound| bound.values[vertex])
            .fold(f64::NEG_INFINITY, f64::max);
        bounds[phase_index].values[vertex] >= maximum - tolerance
    })
}

fn reject_positive_area_ties(
    triangle: GridTriangle,
    candidates: &[PhaseBounds],
    polygons: &[StablePhasePolygon],
    tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<(), StableContourError> {
    for (left_index, left) in candidates.iter().enumerate() {
        for right in candidates.iter().skip(left_index + 1) {
            if left
                .values
                .into_iter()
                .zip(right.values)
                .all(|(left, right)| (left - right).abs() <= tolerance)
                && polygons
                    .iter()
                    .any(|polygon| polygon.phase_index == left.phase_index)
                && polygons
                    .iter()
                    .any(|polygon| polygon.phase_index == right.phase_index)
            {
                diagnostics.co_stable_regions += 1;
                return Err(StableContourError::PositiveAreaHeightTie {
                    triangle: triangle.id,
                    phases: if left.phase < right.phase {
                        [left.phase, right.phase]
                    } else {
                        [right.phase, left.phase]
                    },
                });
            }
        }
    }
    Ok(())
}

fn collect_boundary_diagnostics(
    polygons: &[StablePhasePolygon],
    bounds: &[PhaseBounds],
    tolerance: f64,
    geometry_tolerance: f64,
    diagnostics: &mut StableContourDiagnostics,
) {
    let mut edges = BTreeSet::new();
    let mut invariants = BTreeSet::new();
    for polygon in polygons {
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
            let tied = tied_phases(bounds, midpoint, tolerance);
            if tied.len() == 2 {
                let mut endpoints = [
                    bucket(left, geometry_tolerance),
                    bucket(right, geometry_tolerance),
                ];
                endpoints.sort();
                edges.insert((tied, endpoints));
            }
        }
        for &vertex in &polygon.barycentric {
            let tied = tied_phases(bounds, vertex, tolerance);
            if tied.len() >= 3 {
                invariants.insert((tied, bucket(vertex, geometry_tolerance)));
            }
        }
    }
    diagnostics.univariant_edges += edges.len();
    diagnostics.invariant_vertices += invariants.len();
}

fn tied_phases(
    bounds: &[PhaseBounds],
    barycentric: [f64; 3],
    tolerance: f64,
) -> Vec<StablePhaseId> {
    let values: Vec<_> = bounds
        .iter()
        .map(|bound| dot(bound.values, barycentric))
        .collect();
    let maximum = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    bounds
        .iter()
        .zip(values)
        .filter(|(_, value)| *value >= maximum - tolerance)
        .map(|(bound, _)| bound.phase)
        .collect()
}

fn bucket(point: [f64; 3], tolerance: f64) -> [i64; 3] {
    point.map(|value| {
        let scaled = (value / tolerance).round();
        if scaled <= i64::MIN as f64 {
            i64::MIN
        } else if scaled >= i64::MAX as f64 {
            i64::MAX
        } else {
            scaled as i64
        }
    })
}

pub(crate) fn point_from_barycentric(
    cell: &UmbrellaStableCell,
    barycentric: [f64; 3],
) -> TernaryCoordinate {
    composition(cell.vertices, barycentric)
}
