// Prepared interpolation and final-contour response records.
//
// These summaries consume already prepared evaluators or completed contour
// geometry. They do not rebuild alpha intervals, virtual stencils, point
// locations, or contour topology.

#[cfg(feature = "cubic-alpha")]
use std::collections::BTreeMap;

use crate::{ContourPath, ContourSet, TernaryCoordinate, simplex::logical_distance};
#[cfg(feature = "cubic-alpha")]
use crate::{
    GridTriangle, GridVertexId, InterpolatedTernaryField, simplex::logical_from_composition,
};

/// Measured final geometry for one open or closed contour component.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourPathResponseMetrics {
    /// Whether the final component is periodic.
    pub closed: bool,
    /// Number of retained semantic points.
    pub point_count: usize,
    /// Arc length in canonical logical ternary geometry.
    pub logical_length: f64,
}

/// Final response summary for one requested contour level.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourLevelResponseMetrics {
    /// Requested scalar level.
    pub level: f64,
    /// Final component measurements in deterministic path order.
    pub paths: Vec<ContourPathResponseMetrics>,
    /// Number of open final components.
    pub open_path_count: usize,
    /// Number of closed final components.
    pub closed_path_count: usize,
    /// Total logical arc length of final paths.
    pub total_logical_length: f64,
    /// Final retained point count.
    pub total_point_count: usize,
    /// Source triangle count when final diagnostics provide it.
    pub source_triangle_count: Option<usize>,
    /// Adaptive microtriangles evaluated when final diagnostics provide it.
    pub evaluated_microtriangle_count: Option<usize>,
    /// Adaptive microtriangles refined when final diagnostics provide it.
    pub refined_microtriangle_count: Option<usize>,
    /// Largest final spacing coefficient of variation when diagnostics provide it.
    pub maximum_spacing_coefficient_of_variation: Option<f64>,
}

impl ContourSet {
    /// Summarize final regular contour geometry without changing contour output.
    pub fn response_metrics(&self) -> Vec<ContourLevelResponseMetrics> {
        self.levels
            .iter()
            .map(|level| contour_level_metrics(level.value, &level.paths, None))
            .collect()
    }
}

/// Shared-edge value and tangential-derivative consistency of a prepared regular cubic field.
#[cfg(feature = "cubic-alpha")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegularCubicEdgeContinuityMetrics {
    /// Number of interior regular lattice edges sampled at their midpoint.
    pub interior_edge_count: usize,
    /// Largest absolute mismatch in shared-edge scalar value.
    pub maximum_value_mismatch: f64,
    /// Largest absolute mismatch in logical tangential derivative.
    pub maximum_tangential_gradient_mismatch: f64,
}

#[cfg(feature = "cubic-alpha")]
impl<'a> InterpolatedTernaryField<'a> {
    /// Check C0 values and shared-edge tangential derivatives of cached cubic intervals.
    ///
    /// Returns `None` for a prepared linear field. Normal derivatives are not
    /// compared because cubic-alpha fields are C0, not generally C1.
    pub fn cubic_edge_continuity_metrics(&self) -> Option<RegularCubicEdgeContinuityMetrics> {
        self.cubic_diagnostics()?;
        let grid = self.field().grid();
        let mut edges = BTreeMap::<[usize; 2], Vec<(GridTriangle, [usize; 2])>>::new();
        for triangle in grid.elementary_triangles().ok()? {
            for local in [[0, 1], [1, 2], [2, 0]] {
                let first = triangle.vertices[local[0]];
                let second = triangle.vertices[local[1]];
                edges
                    .entry([first.0.min(second.0), first.0.max(second.0)])
                    .or_default()
                    .push((triangle, local));
            }
        }
        let mut count = 0;
        let mut maximum_value_mismatch = 0.0_f64;
        let mut maximum_tangential_gradient_mismatch = 0.0_f64;
        for (key, uses) in edges {
            if uses.len() != 2 {
                continue;
            }
            let midpoint_weights = |edge: [usize; 2]| {
                let mut weights = [0.0; 3];
                weights[edge[0]] = 0.5;
                weights[edge[1]] = 0.5;
                weights
            };
            let (first_value, first_gradient) = self
                .evaluate_in_triangle(uses[0].0, midpoint_weights(uses[0].1))
                .ok()?;
            let (second_value, second_gradient) = self
                .evaluate_in_triangle(uses[1].0, midpoint_weights(uses[1].1))
                .ok()?;
            let endpoint = [GridVertexId(key[0]), GridVertexId(key[1])]
                .map(|vertex| grid.composition(vertex).ok());
            let [Some(first), Some(second)] = endpoint else {
                return None;
            };
            let first = logical_from_composition(first);
            let second = logical_from_composition(second);
            let tangent = [second[0] - first[0], second[1] - first[1]];
            let length = tangent[0].hypot(tangent[1]);
            if length == 0.0 || !length.is_finite() {
                return None;
            }
            let tangent = [tangent[0] / length, tangent[1] / length];
            let first = super::TernaryGradient::from_reduced_ab(first_gradient).logical_xy();
            let second = super::TernaryGradient::from_reduced_ab(second_gradient).logical_xy();
            let first_tangent = first[0].mul_add(tangent[0], first[1] * tangent[1]);
            let second_tangent = second[0].mul_add(tangent[0], second[1] * tangent[1]);
            count += 1;
            maximum_value_mismatch = maximum_value_mismatch.max((first_value - second_value).abs());
            maximum_tangential_gradient_mismatch =
                maximum_tangential_gradient_mismatch.max((first_tangent - second_tangent).abs());
        }
        Some(RegularCubicEdgeContinuityMetrics {
            interior_edge_count: count,
            maximum_value_mismatch,
            maximum_tangential_gradient_mismatch,
        })
    }
}

fn contour_level_metrics(
    level: f64,
    paths: &[ContourPath],
    diagnostics: Option<(usize, usize, usize, Option<f64>)>,
) -> ContourLevelResponseMetrics {
    let paths = paths.iter().map(path_metrics).collect::<Vec<_>>();
    ContourLevelResponseMetrics {
        level,
        open_path_count: paths.iter().filter(|path| !path.closed).count(),
        closed_path_count: paths.iter().filter(|path| path.closed).count(),
        total_logical_length: paths.iter().map(|path| path.logical_length).sum(),
        total_point_count: paths.iter().map(|path| path.point_count).sum(),
        source_triangle_count: diagnostics.map(|diagnostics| diagnostics.0),
        evaluated_microtriangle_count: diagnostics.map(|diagnostics| diagnostics.1),
        refined_microtriangle_count: diagnostics.map(|diagnostics| diagnostics.2),
        maximum_spacing_coefficient_of_variation: diagnostics.and_then(|diagnostics| diagnostics.3),
        paths,
    }
}

fn path_metrics(path: &ContourPath) -> ContourPathResponseMetrics {
    let mut points = path
        .points
        .iter()
        .copied()
        .map(TernaryCoordinate::as_array)
        .collect::<Vec<_>>();
    if path.closed && points.len() > 1 {
        points.push(points[0]);
    }
    ContourPathResponseMetrics {
        closed: path.closed,
        point_count: path.points.len(),
        logical_length: points
            .windows(2)
            .map(|pair| logical_distance(pair[0], pair[1]))
            .sum(),
    }
}
// Prepared cubic-alpha response records.
//
// These summaries consume already prepared evaluators. They never rebuild
// intervals, virtual stencil locations, or contour topology.

/// Per-edge response of a converged irregular cubic-alpha field.
#[cfg(feature = "irregular-cubic-alpha")]
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularAlphaEdgeMetrics {
    /// Stable canonical mesh edge identifier.
    pub edge: IrregularEdgeId,
    /// Logical endpoint distance.
    pub length: f64,
    /// Whether both virtual endpoint locations were available at preparation.
    pub complete_stencil: bool,
    /// Whether this is a convex-hull edge.
    pub boundary_edge: bool,
    /// Final interval coefficient at canonical endpoint zero.
    pub alpha0: f64,
    /// Final interval coefficient multiplying normalized interval coordinate.
    pub alpha1: f64,
    /// Euclidean norm of the two final alpha coefficients.
    pub coefficient_norm: f64,
}

/// Compact aggregate response of a prepared irregular cubic-alpha evaluator.
#[cfg(feature = "irregular-cubic-alpha")]
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularAlphaResponseMetrics {
    /// Stable edge-indexed response records.
    pub edges: Vec<IrregularAlphaEdgeMetrics>,
    /// Count of complete virtual stencils.
    pub complete_stencil_edge_count: usize,
    /// Count of linear-fallback edges.
    pub linear_fallback_edge_count: usize,
    /// Distribution of final alpha-coefficient norms.
    pub coefficient_norms: super::DistributionSummary,
}

#[cfg(feature = "irregular-cubic-alpha")]
impl<'a> InterpolatedIrregularTernaryField<'a> {
    /// Summarize final cubic-alpha edge coefficients without retaining stencils.
    ///
    /// Returns `None` for a prepared linear field. The compact completion flags
    /// distinguish fully virtual-stencilled intervals from explicit linear
    /// boundary fallbacks; Muggianu/Kohler remain interpolation choices, not
    /// separate response families.
    pub fn irregular_alpha_response_metrics(&self) -> Option<IrregularAlphaResponseMetrics> {
        let intervals = self.cubic_alpha_intervals()?;
        let complete = self.cubic_alpha_stencil_complete()?;
        let mesh = self.mesh();
        let edges = mesh
            .edges()
            .map(|edge| alpha_edge_metric(mesh, edge.id, intervals[edge.id.0], complete[edge.id.0]))
            .collect::<Vec<_>>();
        let coefficient_norms = edges
            .iter()
            .map(|edge| edge.coefficient_norm)
            .collect::<Vec<_>>();
        Some(IrregularAlphaResponseMetrics {
            complete_stencil_edge_count: edges.iter().filter(|edge| edge.complete_stencil).count(),
            linear_fallback_edge_count: edges.iter().filter(|edge| !edge.complete_stencil).count(),
            coefficient_norms: super::DistributionSummary::from_values(&coefficient_norms)
                .expect("prepared alpha intervals are finite"),
            edges,
        })
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
fn alpha_edge_metric(
    mesh: &IrregularTernaryMesh,
    edge: IrregularEdgeId,
    interval: crate::interpolation::AlphaInterval,
    complete_stencil: bool,
) -> IrregularAlphaEdgeMetrics {
    let topology = mesh.edge(edge).expect("dense mesh edge");
    let endpoint = topology
        .vertices
        .map(|vertex| mesh.composition(vertex).expect("mesh vertex"));
    IrregularAlphaEdgeMetrics {
        edge,
        length: logical_distance(endpoint[0], endpoint[1]),
        complete_stencil,
        boundary_edge: topology.is_boundary(),
        alpha0: interval.alpha0,
        alpha1: interval.alpha1,
        coefficient_norm: interval.alpha0.hypot(interval.alpha1),
    }
}

#[cfg(feature = "irregular-delaunay")]
use crate::IrregularContourSet;
#[cfg(feature = "irregular-cubic-alpha")]
use crate::{InterpolatedIrregularTernaryField, IrregularEdgeId, IrregularTernaryMesh};

#[cfg(feature = "irregular-delaunay")]
impl IrregularContourSet {
    /// Summarize final irregular contour geometry and associate existing diagnostics.
    pub fn response_metrics(&self) -> Vec<ContourLevelResponseMetrics> {
        self.levels
            .iter()
            .zip(&self.diagnostics().levels)
            .map(|(level, diagnostics)| {
                contour_level_metrics(
                    level.value,
                    &level.paths,
                    Some((
                        diagnostics.source_triangles,
                        diagnostics.evaluated_microtriangles,
                        diagnostics.refined_microtriangles,
                        diagnostics.spacing_cv_after,
                    )),
                )
            })
            .collect()
    }
}

/// Shared-edge value and tangential-derivative consistency of a prepared irregular cubic field.
#[cfg(feature = "irregular-cubic-alpha")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularCubicEdgeContinuityMetrics {
    /// Number of interior Delaunay edges sampled at their midpoint.
    pub interior_edge_count: usize,
    /// Largest absolute mismatch in shared-edge scalar value.
    pub maximum_value_mismatch: f64,
    /// Largest absolute mismatch in logical tangential derivative.
    pub maximum_tangential_gradient_mismatch: f64,
}

#[cfg(feature = "irregular-cubic-alpha")]
impl<'a> InterpolatedIrregularTernaryField<'a> {
    /// Check C0 values and shared-edge tangential derivatives of cached irregular intervals.
    ///
    /// Returns `None` for a prepared linear field. It intentionally does not
    /// compare normal derivatives, which need not agree across a cubic-alpha
    /// elementary-triangle boundary.
    pub fn cubic_edge_continuity_metrics(&self) -> Option<IrregularCubicEdgeContinuityMetrics> {
        self.cubic_alpha_intervals()?;
        let mesh = self.mesh();
        let mut count = 0;
        let mut maximum_value_mismatch = 0.0_f64;
        let mut maximum_tangential_gradient_mismatch = 0.0_f64;
        for edge in mesh.edges().filter(|edge| !edge.is_boundary()) {
            let [Some(first_id), Some(second_id)] = edge.triangles else {
                return None;
            };
            let first = mesh.triangle(first_id).ok()?;
            let second = mesh.triangle(second_id).ok()?;
            let weights = |triangle: crate::IrregularMeshTriangle| {
                let mut weights = [0.0; 3];
                for (index, vertex) in triangle.vertices.into_iter().enumerate() {
                    if vertex == edge.vertices[0] || vertex == edge.vertices[1] {
                        weights[index] = 0.5;
                    }
                }
                weights
            };
            let (first_value, first_gradient) =
                self.evaluate_in_triangle(first, weights(first)).ok()?;
            let (second_value, second_gradient) =
                self.evaluate_in_triangle(second, weights(second)).ok()?;
            let endpoint = edge.vertices.map(|vertex| mesh.composition(vertex).ok());
            let [Some(first), Some(second)] = endpoint else {
                return None;
            };
            let first = logical_from_composition(first);
            let second = logical_from_composition(second);
            let tangent = [second[0] - first[0], second[1] - first[1]];
            let length = tangent[0].hypot(tangent[1]);
            if length == 0.0 || !length.is_finite() {
                return None;
            }
            let tangent = [tangent[0] / length, tangent[1] / length];
            let first = super::TernaryGradient::from_reduced_ab(first_gradient).logical_xy();
            let second = super::TernaryGradient::from_reduced_ab(second_gradient).logical_xy();
            let first_tangent = first[0].mul_add(tangent[0], first[1] * tangent[1]);
            let second_tangent = second[0].mul_add(tangent[0], second[1] * tangent[1]);
            count += 1;
            maximum_value_mismatch = maximum_value_mismatch.max((first_value - second_value).abs());
            maximum_tangential_gradient_mismatch =
                maximum_tangential_gradient_mismatch.max((first_tangent - second_tangent).abs());
        }
        Some(IrregularCubicEdgeContinuityMetrics {
            interior_edge_count: count,
            maximum_value_mismatch,
            maximum_tangential_gradient_mismatch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "cubic-alpha")]
    use crate::FieldInterpolation;
    use crate::{ContourOptions, RegularTernaryScalarField};

    #[test]
    fn final_regular_contour_lengths_are_reported_without_mutation() {
        let field = RegularTernaryScalarField::from_fn(8, |[a, b, _]| a - b).unwrap();
        let contours = ContourSet::compute(&field, &[0.0], ContourOptions::linear()).unwrap();
        let metrics = contours.response_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].level, 0.0);
        assert_eq!(metrics[0].open_path_count + metrics[0].closed_path_count, 1);
        assert!(metrics[0].total_logical_length > 0.0);
    }

    #[cfg(feature = "cubic-alpha")]
    #[test]
    fn regular_cubic_shared_edges_are_c0_and_tangent_continuous() {
        let field = RegularTernaryScalarField::from_fn(5, |[a, b, c]| {
            a.powi(3) - 0.4 * b.powi(2) + 0.2 * c + a * b
        })
        .unwrap();
        let evaluator = InterpolatedTernaryField::new(
            &field,
            FieldInterpolation::CubicAlpha(crate::CubicAlphaBuildOptions::default()),
        )
        .unwrap();
        let response = evaluator.cubic_edge_continuity_metrics().unwrap();
        assert!(response.interior_edge_count > 0);
        assert!(response.maximum_value_mismatch < 1.0e-11);
        assert!(response.maximum_tangential_gradient_mismatch < 1.0e-10);
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn irregular_alpha_response_is_compact_and_checks_shared_edges() {
        use crate::{
            InterpolatedIrregularTernaryField, IrregularCubicAlphaOptions,
            IrregularFieldInterpolation, IrregularTernaryMesh, IrregularTernaryScalarField,
        };
        let mesh = IrregularTernaryMesh::new([
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.57, 0.28, 0.15],
            [0.18, 0.61, 0.21],
            [0.23, 0.16, 0.61],
            [0.31, 0.42, 0.27],
        ])
        .unwrap();
        let field = IrregularTernaryScalarField::from_fn(mesh, |[a, b, c]| a * a - b * c + 0.3 * a)
            .unwrap();
        let evaluator = InterpolatedIrregularTernaryField::new(
            &field,
            IrregularFieldInterpolation::CubicAlpha(IrregularCubicAlphaOptions::default()),
        )
        .unwrap();
        let alpha = evaluator.irregular_alpha_response_metrics().unwrap();
        assert_eq!(alpha.edges.len(), field.mesh().edge_count());
        assert_eq!(
            alpha.complete_stencil_edge_count + alpha.linear_fallback_edge_count,
            alpha.edges.len()
        );
        let continuity = evaluator.cubic_edge_continuity_metrics().unwrap();
        assert!(continuity.maximum_value_mismatch < 1.0e-10);
        assert!(continuity.maximum_tangential_gradient_mismatch < 1.0e-9);
    }
}
