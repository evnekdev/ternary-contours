use std::collections::BTreeSet;

use crate::GridTriangle;

use super::{
    StableContourDiagnostics, StableContourError, StableContourQuantity, StableGridVerification,
    StableVerificationPassDiagnostics,
    sample::{
        PreparedSourceLayer, RegularSamplingGrid, SourceGeometryGroup, SourceLocationHint,
        evaluate_sources_at_point,
    },
};

pub(crate) fn verify_sampling_grid(
    samples: &RegularSamplingGrid,
    quantity: StableContourQuantity,
    verification: StableGridVerification,
    stability_tolerance: f64,
    layers: &[PreparedSourceLayer<'_>],
    groups: &[SourceGeometryGroup<'_>],
    diagnostics: &mut StableContourDiagnostics,
) -> Result<StableVerificationPassDiagnostics, StableContourError> {
    let mut pass = StableVerificationPassDiagnostics {
        subdivisions: samples.grid.subdivisions(),
        ..StableVerificationPassDiagnostics::default()
    };
    if !verification.enabled {
        return Ok(pass);
    }
    let barycentric_points = verification_points(verification);
    let mut hints = vec![SourceLocationHint::default(); groups.len()];
    let mut direct_heights = vec![None; samples.phase_count];
    let mut direct_secondary =
        (quantity == StableContourQuantity::Secondary).then(|| vec![None; samples.phase_count]);
    let mut squared_height_error = 0.0;
    let mut height_error_count = 0usize;
    let mut worst_triangle_score = f64::NEG_INFINITY;

    for triangle in samples.grid.elementary_triangles()? {
        let vertices = triangle_compositions(samples, triangle)?;
        let candidate_set = affine_candidate_set(samples, triangle, stability_tolerance);
        let mut triangle_unresolved = false;
        let mut triangle_score = 0.0_f64;
        for &barycentric in &barycentric_points {
            let composition = weighted_composition(vertices, barycentric);
            evaluate_sources_at_point(
                layers,
                groups,
                composition,
                &mut hints,
                &mut direct_heights,
                direct_secondary.as_deref_mut(),
                diagnostics,
                None,
            )?;
            pass.verification_points += 1;
            let predicted_heights = samples.affine_heights(triangle.vertices, barycentric);
            let mut point_unresolved = false;
            for (&direct, &predicted) in direct_heights.iter().zip(&predicted_heights) {
                match (direct, predicted) {
                    (Some(direct), Some(predicted)) => {
                        let error = (direct - predicted).abs();
                        pass.maximum_height_approximation_error =
                            pass.maximum_height_approximation_error.max(error);
                        squared_height_error += error * error;
                        height_error_count += 1;
                        if error > verification.height_error_tolerance {
                            point_unresolved = true;
                        }
                        triangle_score = triangle_score.max(error);
                    }
                    (None, None) => {}
                    _ => {
                        point_unresolved = true;
                        triangle_score = triangle_score.max(1.0);
                    }
                }
            }
            if let (Some(direct), Some(predicted)) = (
                direct_secondary.as_deref(),
                samples.affine_secondary(triangle.vertices, barycentric),
            ) {
                for (&direct, &predicted) in direct.iter().zip(&predicted) {
                    match (direct, predicted) {
                        (Some(direct), Some(predicted)) => {
                            let error = (direct - predicted).abs();
                            pass.maximum_secondary_approximation_error =
                                pass.maximum_secondary_approximation_error.max(error);
                            if error > verification.secondary_error_tolerance {
                                point_unresolved = true;
                            }
                            triangle_score = triangle_score.max(error);
                        }
                        (None, None) => {}
                        _ => {
                            point_unresolved = true;
                            triangle_score = triangle_score.max(1.0);
                        }
                    }
                }
            }
            let direct_stable = stable_set(&direct_heights, verification.ownership_tolerance);
            let predicted_stable = stable_set(&predicted_heights, verification.ownership_tolerance);
            if direct_stable != predicted_stable {
                pass.ownership_mismatches += 1;
                point_unresolved = true;
                triangle_score = triangle_score.max(1.0);
            }
            for phase in direct_stable {
                if !candidate_set.contains(&phase) {
                    pass.hidden_candidate_discoveries += 1;
                    point_unresolved = true;
                    triangle_score = triangle_score.max(1.0);
                }
            }
            triangle_unresolved |= point_unresolved;
        }
        if triangle_unresolved {
            pass.unresolved_sampling_triangles += 1;
            if triangle_score > worst_triangle_score {
                worst_triangle_score = triangle_score;
                pass.worst_unresolved_triangle = Some(triangle.id);
            }
        }
    }
    if height_error_count != 0 {
        pass.rms_height_approximation_error =
            (squared_height_error / height_error_count as f64).sqrt();
    }
    Ok(pass)
}

fn verification_points(options: StableGridVerification) -> Vec<[f64; 3]> {
    let mut points = Vec::with_capacity(4);
    if options.verify_centroids {
        points.push([1.0 / 3.0; 3]);
    }
    if options.verify_edge_midpoints {
        points.extend([[0.5, 0.5, 0.0], [0.0, 0.5, 0.5], [0.5, 0.0, 0.5]]);
    }
    points
}

fn triangle_compositions(
    samples: &RegularSamplingGrid,
    triangle: GridTriangle,
) -> Result<[[f64; 3]; 3], StableContourError> {
    triangle
        .vertices
        .map(|vertex| samples.grid.composition(vertex))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| StableContourError::StablePolygonFailure {
            triangle: triangle.id,
            phase: super::StablePhaseId(0),
        })
}

fn weighted_composition(vertices: [[f64; 3]; 3], barycentric: [f64; 3]) -> [f64; 3] {
    let mut composition = [0.0; 3];
    for local in 0..3 {
        for component in 0..3 {
            composition[component] += vertices[local][component] * barycentric[local];
        }
    }
    let sum = composition.into_iter().sum::<f64>();
    composition.map(|component| component / sum)
}

fn affine_candidate_set(
    samples: &RegularSamplingGrid,
    triangle: GridTriangle,
    tolerance: f64,
) -> BTreeSet<usize> {
    let bounds: Vec<_> = (0..samples.phase_count)
        .filter_map(|phase| {
            let values = samples.triangle_height_values(phase, triangle.vertices)?;
            Some((
                phase,
                values.into_iter().fold(f64::INFINITY, f64::min),
                values.into_iter().fold(f64::NEG_INFINITY, f64::max),
            ))
        })
        .collect();
    let envelope_floor = bounds
        .iter()
        .map(|(_, minimum, _)| *minimum)
        .fold(f64::NEG_INFINITY, f64::max);
    bounds
        .into_iter()
        .filter(|(_, _, maximum)| *maximum >= envelope_floor - tolerance)
        .map(|(phase, _, _)| phase)
        .collect()
}

fn stable_set(values: &[Option<f64>], tolerance: f64) -> BTreeSet<usize> {
    let maximum = values
        .iter()
        .flatten()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    values
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_some_and(|value| value >= maximum - tolerance))
        .map(|(index, _)| index)
        .collect()
}
