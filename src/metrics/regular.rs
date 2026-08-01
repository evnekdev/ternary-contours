//! Regular-lattice adapters for the shared field-analysis records.

use std::collections::BTreeMap;

use crate::{
    FieldEvaluationError, GridTriangle, GridVertexId, InterpolatedTernaryField,
    RegularTernaryScalarField, simplex::logical_from_composition,
};

use super::{
    GradientJump, LocalQuadraticError, LocalQuadraticEstimate, LocalQuadraticOptions,
    ScalarFieldDistributionInput, ScalarFieldDistributionMetrics, TernaryGradient,
    fit_local_quadratic,
};

/// The two orientation families of elementary triangles in the regular lattice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegularTriangleOrientation {
    /// Triangle with the `i` coordinate increasing along local edge `(0,1)`.
    Upward,
    /// Complementary elementary-triangle orientation.
    Downward,
}

/// Prepared-field metrics for one elementary regular-grid triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct RegularTriangleFieldMetrics {
    /// Canonical triangle identifier and local vertex order.
    pub triangle: GridTriangle,
    /// Uniform-family orientation in the canonical lattice.
    pub orientation: RegularTriangleOrientation,
    /// Logical equilateral triangle area.
    pub area: f64,
    /// Common logical side length.
    pub characteristic_size: f64,
    /// Analytic prepared-interpolant gradient in the triangle interior.
    pub gradient: TernaryGradient,
    /// Invariant logical gradient magnitude.
    pub gradient_norm: f64,
}

/// Shared one-sided gradient jump across a regular interior lattice edge.
#[derive(Clone, Debug, PartialEq)]
pub struct RegularGradientJump {
    /// Canonically ordered edge endpoints.
    pub vertices: [GridVertexId; 2],
    /// Selected triangle on the left side of the canonical endpoint direction.
    pub left_triangle: GridTriangle,
    /// Selected triangle on the right side of the canonical endpoint direction.
    pub right_triangle: GridTriangle,
    /// Common jump record in logical coordinates.
    pub jump: GradientJump,
}

/// Analysis of a prepared regular field using shared field definitions.
///
/// This is not a triangulation-quality API: regular geometry is analytically
/// uniform. The triangle list instead makes the lattice a controlled reference
/// for gradient, curvature, and response comparisons.
#[derive(Clone, Debug, PartialEq)]
pub struct RegularFieldMetrics {
    /// Per-triangle prepared gradients and known regular geometry.
    pub triangles: Vec<RegularTriangleFieldMetrics>,
    /// One record per interior lattice edge.
    pub gradient_jumps: Vec<RegularGradientJump>,
    /// Comparable scalar distributions with explicit unweighted semantics.
    pub distributions: ScalarFieldDistributionMetrics,
}

impl<'a> InterpolatedTernaryField<'a> {
    /// Create a derived quantity adapter without rebuilding prepared intervals.
    pub fn derived(
        &self,
        quantity: super::DerivedFieldQuantity,
    ) -> super::DerivedRegularTernaryField<'_, 'a> {
        super::DerivedRegularTernaryField::new(self, quantity)
    }

    /// Analyze this prepared field over its canonical regular-lattice topology.
    pub fn metrics(&self) -> Result<RegularFieldMetrics, FieldEvaluationError> {
        regular_field_metrics(self)
    }
}

impl RegularTernaryScalarField {
    /// Estimate sampled-field curvature near a regular lattice vertex.
    ///
    /// The fit operates in the canonical logical equilateral plane. It expands
    /// a deterministic lattice neighbourhood through `options.max_ring`; small
    /// grids or rank-deficient boundary samples return a typed error.
    pub fn local_quadratic_estimate(
        &self,
        vertex: GridVertexId,
        options: LocalQuadraticOptions,
    ) -> Result<LocalQuadraticEstimate, LocalQuadraticError> {
        if vertex.0 >= self.vertex_count() {
            return Err(LocalQuadraticError::InsufficientSamples {
                actual: 0,
                required: 6,
            });
        }
        let grid = self.grid();
        let coordinate = grid.lattice_coordinate(vertex).map_err(|_| {
            LocalQuadraticError::InsufficientSamples {
                actual: 0,
                required: 6,
            }
        })?;
        let centre = logical_from_composition(
            grid.composition(vertex)
                .map_err(|_| LocalQuadraticError::NonFinite)?,
        );
        let mut last_count = 0;
        for ring in 1..=options.max_ring {
            let observations = grid
                .indexed_compositions()
                .filter_map(|(candidate, composition)| {
                    let other = grid.lattice_coordinate(candidate).ok()?;
                    let distance = coordinate
                        .i
                        .abs_diff(other.i)
                        .max(coordinate.j.abs_diff(other.j))
                        .max(coordinate.k.abs_diff(other.k));
                    (distance <= ring).then_some((
                        logical_from_composition(composition),
                        self.values()[candidate.0],
                    ))
                })
                .collect::<Vec<_>>();
            last_count = observations.len();
            if observations.len() >= 6 {
                match fit_local_quadratic(centre, &observations, options, ring) {
                    Err(LocalQuadraticError::RankDeficient) if ring < options.max_ring => continue,
                    result => return result,
                }
            }
        }
        Err(LocalQuadraticError::InsufficientSamples {
            actual: last_count,
            required: 6,
        })
    }
}

fn regular_field_metrics(
    evaluator: &InterpolatedTernaryField<'_>,
) -> Result<RegularFieldMetrics, FieldEvaluationError> {
    let field = evaluator.field();
    let grid = field.grid();
    let triangles = grid
        .elementary_triangles()
        .map_err(FieldEvaluationError::CubicConstruction)?;
    let mut result_triangles = Vec::with_capacity(triangles.len());
    let mut gradient_norms = Vec::with_capacity(triangles.len());
    let mut edge_uses = BTreeMap::<[usize; 2], Vec<(GridTriangle, [usize; 2])>>::new();
    for triangle in triangles.iter().copied() {
        let (_, gradient_ab) = evaluator.evaluate_in_triangle(triangle, [1.0 / 3.0; 3])?;
        let gradient = TernaryGradient::from_reduced_ab(gradient_ab);
        let [first_vertex, second_vertex, third_vertex] = triangle.vertices;
        let compositions = [
            grid.composition(first_vertex)
                .map_err(|_| FieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
            grid.composition(second_vertex)
                .map_err(|_| FieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
            grid.composition(third_vertex)
                .map_err(|_| FieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
        ];
        let points = compositions.map(logical_from_composition);
        let area = triangle_area(points);
        let characteristic_size = (points[0][0] - points[1][0]).hypot(points[0][1] - points[1][1]);
        let first = grid
            .lattice_coordinate(triangle.vertices[0])
            .map_err(FieldEvaluationError::CubicConstruction)?;
        let second = grid
            .lattice_coordinate(triangle.vertices[1])
            .map_err(FieldEvaluationError::CubicConstruction)?;
        result_triangles.push(RegularTriangleFieldMetrics {
            triangle,
            orientation: if second.i == first.i + 1 {
                RegularTriangleOrientation::Upward
            } else {
                RegularTriangleOrientation::Downward
            },
            area,
            characteristic_size,
            gradient,
            gradient_norm: gradient.norm(),
        });
        gradient_norms.push(gradient.norm());
        for endpoints in [[0, 1], [1, 2], [2, 0]] {
            let left = triangle.vertices[endpoints[0]];
            let right = triangle.vertices[endpoints[1]];
            let key = [left.0.min(right.0), left.0.max(right.0)];
            edge_uses
                .entry(key)
                .or_default()
                .push((triangle, endpoints));
        }
    }

    let mut edge_differences = Vec::with_capacity(edge_uses.len());
    let mut edge_secants = Vec::with_capacity(edge_uses.len());
    for key in edge_uses.keys().copied() {
        let left = GridVertexId(key[0]);
        let right = GridVertexId(key[1]);
        let difference = field.values()[right.0] - field.values()[left.0];
        let length = crate::simplex::logical_distance(
            grid.composition(left)
                .map_err(FieldEvaluationError::CubicConstruction)?,
            grid.composition(right)
                .map_err(FieldEvaluationError::CubicConstruction)?,
        );
        edge_differences.push(difference);
        edge_secants.push(difference / length);
    }

    let mut jumps = Vec::new();
    for (key, uses) in edge_uses {
        if uses.len() != 2 {
            continue;
        }
        let endpoints = [GridVertexId(key[0]), GridVertexId(key[1])];
        let edge_points = [
            grid.composition(endpoints[0])
                .map_err(|_| FieldEvaluationError::InvalidLocation { triangle: 0 })?,
            grid.composition(endpoints[1])
                .map_err(|_| FieldEvaluationError::InvalidLocation { triangle: 0 })?,
        ];
        let logical = edge_points.map(logical_from_composition);
        let tangent = [logical[1][0] - logical[0][0], logical[1][1] - logical[0][1]];
        let mut sides = uses
            .iter()
            .map(|(triangle, local_edge)| {
                let third = (0..3)
                    .find(|index| *index != local_edge[0] && *index != local_edge[1])
                    .expect("triangle edge has one remaining vertex");
                let third_point = logical_from_composition(
                    grid.composition(triangle.vertices[third])
                        .expect("canonical triangle vertex"),
                );
                let cross = tangent[0] * (third_point[1] - logical[0][1])
                    - tangent[1] * (third_point[0] - logical[0][0]);
                (*triangle, *local_edge, cross)
            })
            .collect::<Vec<_>>();
        sides.sort_by(|left, right| left.2.total_cmp(&right.2));
        let (right_triangle, right_edge, _) = sides[0];
        let (left_triangle, left_edge, _) = sides[1];
        let barycentric = |edge: [usize; 2]| {
            let mut weights = [0.0; 3];
            weights[edge[0]] = 0.5;
            weights[edge[1]] = 0.5;
            weights
        };
        let (_, left_gradient) =
            evaluator.evaluate_in_triangle(left_triangle, barycentric(left_edge))?;
        let (_, right_gradient) =
            evaluator.evaluate_in_triangle(right_triangle, barycentric(right_edge))?;
        let position = [
            0.5 * (edge_points[0][0] + edge_points[1][0]),
            0.5 * (edge_points[0][1] + edge_points[1][1]),
            0.5 * (edge_points[0][2] + edge_points[1][2]),
        ];
        let jump = GradientJump::from_gradients(
            position,
            TernaryGradient::from_reduced_ab(left_gradient),
            TernaryGradient::from_reduced_ab(right_gradient),
            tangent,
        )
        .ok_or(FieldEvaluationError::NonFiniteEvaluation)?;
        jumps.push(RegularGradientJump {
            vertices: endpoints,
            left_triangle,
            right_triangle,
            jump,
        });
    }

    let mut hessian_norms = Vec::new();
    let mut laplacians = Vec::new();
    let mut anisotropies = Vec::new();
    let mut unavailable = 0;
    for (vertex, _) in grid.indexed_compositions() {
        match field.local_quadratic_estimate(vertex, LocalQuadraticOptions::default()) {
            Ok(estimate) => {
                hessian_norms.push(estimate.hessian_norm);
                laplacians.push(estimate.laplacian);
                if let Some(anisotropy) = estimate.anisotropy {
                    anisotropies.push(anisotropy);
                }
            }
            Err(_) => unavailable += 1,
        }
    }
    let jump_magnitudes = jumps
        .iter()
        .map(|jump| jump.jump.magnitude)
        .collect::<Vec<_>>();
    let distributions = ScalarFieldDistributionMetrics::from_input(ScalarFieldDistributionInput {
        sample_values: field.values(),
        edge_differences: &edge_differences,
        edge_secant_slopes: &edge_secants,
        gradient_norms: &gradient_norms,
        hessian_norms: &hessian_norms,
        laplacians: &laplacians,
        curvature_anisotropies: &anisotropies,
        gradient_jump_magnitudes: &jump_magnitudes,
        unavailable_local_estimate_count: unavailable,
        non_finite_evaluation_count: 0,
    })
    .expect("prepared field metrics are finite");
    Ok(RegularFieldMetrics {
        triangles: result_triangles,
        gradient_jumps: jumps,
        distributions,
    })
}

fn triangle_area(points: [[f64; 2]; 3]) -> f64 {
    0.5 * ((points[1][0] - points[0][0]) * (points[2][1] - points[0][1])
        - (points[1][1] - points[0][1]) * (points[2][0] - points[0][0]))
        .abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldInterpolation, RegularTernaryScalarField};

    #[test]
    fn regular_affine_metrics_have_constant_gradient_and_zero_jumps() {
        let field =
            RegularTernaryScalarField::from_fn(5, |[a, b, c]| 2.0 * a - 3.0 * b + c).unwrap();
        let evaluator = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear).unwrap();
        let metrics = evaluator.metrics().unwrap();
        assert!(
            metrics
                .gradient_jumps
                .iter()
                .all(|jump| jump.jump.magnitude < 1.0e-12)
        );
        assert!(
            metrics
                .triangles
                .iter()
                .all(|triangle| (triangle.gradient.reduced_ab()[0] - 1.0).abs() < 1.0e-12)
        );
    }

    #[test]
    fn regular_quadratic_fit_recovers_logical_hessian() {
        let field = RegularTernaryScalarField::from_fn(6, |composition| {
            let [x, y] = logical_from_composition(composition);
            0.5 * 3.0 * x * x + 2.0 * x * y + -0.5 * y * y
        })
        .unwrap();
        let centre = field
            .grid()
            .vertex_id(crate::LatticeCoordinate { i: 2, j: 2, k: 2 })
            .unwrap();
        let estimate = field
            .local_quadratic_estimate(centre, LocalQuadraticOptions::default())
            .unwrap();
        assert!((estimate.hessian_logical_xy[0][0] - 3.0).abs() < 1.0e-10);
        assert!((estimate.hessian_logical_xy[0][1] - 2.0).abs() < 1.0e-10);
        assert!((estimate.hessian_logical_xy[1][1] + 1.0).abs() < 1.0e-10);
    }
}
