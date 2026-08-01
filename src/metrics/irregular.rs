//! Delaunay-specific geometry plus shared-field adapters for irregular meshes.

use std::collections::{BTreeSet, VecDeque};

use crate::{
    InterpolatedIrregularTernaryField, IrregularFieldEvaluationError, IrregularMeshEdge,
    IrregularMeshTriangle, IrregularTernaryMesh, IrregularTernaryScalarField, IrregularTriangleId,
    IrregularVertexId,
    simplex::{logical_distance, logical_from_composition},
};

use super::{
    GradientJump, LocalQuadraticError, LocalQuadraticEstimate, LocalQuadraticOptions,
    ScalarFieldDistributionInput, ScalarFieldDistributionMetrics, TernaryGradient,
    fit_local_quadratic,
};

/// Delaunay-geometry measurements for one irregular triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularTriangleGeometryMetrics {
    /// Stable Delaunay triangle identifier.
    pub triangle: IrregularMeshTriangle,
    /// Positive area in the canonical logical plane.
    pub area: f64,
    /// Logical edge lengths in local `(0,1)`, `(1,2)`, `(2,0)` order.
    pub edge_lengths: [f64; 3],
    /// Triangle perimeter.
    pub perimeter: f64,
    /// Smallest internal angle in radians.
    pub minimum_angle: f64,
    /// Largest internal angle in radians.
    pub maximum_angle: f64,
    /// Inradius divided by circumradius; one for an equilateral triangle.
    pub radius_ratio: f64,
    /// `4*sqrt(3)*area/sum(edge_length^2)`; one for equilateral geometry.
    pub mean_ratio: f64,
    /// Longest-edge altitude aspect ratio; one for equilateral geometry.
    pub altitude_aspect_ratio: f64,
    /// Square-root covariance eigenvalue ratio; one for isotropic triangles.
    pub shape_anisotropy: f64,
    /// Deterministically signed major-axis direction, absent for isotropic triangles.
    pub major_axis_direction: Option<[f64; 2]>,
}

/// Delaunay-geometry measurements for one unique irregular edge.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularEdgeGeometryMetrics {
    /// Canonical edge topology.
    pub edge: IrregularMeshEdge,
    /// Logical endpoint distance.
    pub length: f64,
    /// Ratio of adjacent triangle areas, absent at the hull.
    pub adjacent_area_ratio: Option<f64>,
    /// Ratio of adjacent characteristic sizes, absent at the hull.
    pub adjacent_size_ratio: Option<f64>,
}

/// Delaunay-geometry measurements around one irregular sample vertex.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularVertexGeometryMetrics {
    /// Stable mesh vertex identifier.
    pub vertex: IrregularVertexId,
    /// Number of incident unique edges.
    pub valence: usize,
    /// Barycentric dual area: one third of each incident triangle area.
    pub barycentric_dual_area: f64,
    /// Nearest incident-edge neighbour distance.
    pub nearest_neighbour_distance: Option<f64>,
    /// Minimum edge-graph hops to any convex-hull vertex.
    pub boundary_graph_distance: usize,
    /// Coefficient of variation of incident triangle areas, absent at zero mean.
    pub incident_area_coefficient_of_variation: Option<f64>,
}

/// Whole-mesh Delaunay topology and geometry summary.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularMeshSummary {
    /// Dense mesh vertex count.
    pub vertex_count: usize,
    /// Unique Delaunay edge count.
    pub edge_count: usize,
    /// Delaunay triangle count.
    pub triangle_count: usize,
    /// Convex-hull edge count.
    pub hull_edge_count: usize,
    /// Triangle count with at least one hull edge.
    pub hull_triangle_count: usize,
    /// Convex-hull area, summed from the triangulation.
    pub convex_hull_area: f64,
    /// Convex-hull perimeter.
    pub convex_hull_perimeter: f64,
    /// Distribution of triangle areas.
    pub triangle_areas: super::DistributionSummary,
    /// Distribution of irregular edge lengths.
    pub edge_lengths: super::DistributionSummary,
    /// Distribution of minimum triangle angles.
    pub minimum_angles: super::DistributionSummary,
    /// Distribution of maximum triangle angles.
    pub maximum_angles: super::DistributionSummary,
    /// Distribution of vertex valences.
    pub vertex_valences: super::DistributionSummary,
    /// Distribution of nearest-neighbour sample distances.
    pub nearest_neighbour_distances: super::DistributionSummary,
    /// Distribution of graph distance to the convex hull.
    pub boundary_graph_distances: super::DistributionSummary,
}

/// Backend-neutral records derived from an immutable Delaunay mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularMeshMetrics {
    /// Stable triangle-indexed metric records.
    pub triangles: Vec<IrregularTriangleGeometryMetrics>,
    /// Stable edge-indexed metric records.
    pub edges: Vec<IrregularEdgeGeometryMetrics>,
    /// Stable vertex-indexed metric records.
    pub vertices: Vec<IrregularVertexGeometryMetrics>,
    /// Whole-mesh geometry and topology summary.
    pub summary: IrregularMeshSummary,
}

/// Prepared-field measurements for one irregular triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularTriangleFieldMetrics {
    /// Stable Delaunay triangle identifier.
    pub triangle: IrregularMeshTriangle,
    /// Logical triangle area, matching the mesh geometry metric.
    pub area: f64,
    /// Maximum local logical edge length.
    pub characteristic_size: f64,
    /// Analytic prepared-interpolant gradient inside the triangle.
    pub gradient: TernaryGradient,
    /// Invariant logical gradient magnitude.
    pub gradient_norm: f64,
}

/// Sampled-value measurements for one irregular mesh edge.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularEdgeFieldMetrics {
    /// Canonical Delaunay edge topology.
    pub edge: IrregularMeshEdge,
    /// Signed endpoint difference in canonical vertex-ID orientation.
    pub difference: f64,
    /// Signed difference per unit logical edge length.
    pub secant_slope: f64,
}

/// Shared one-sided gradient jump across one irregular interior edge.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularGradientJump {
    /// Canonical Delaunay edge topology.
    pub edge: IrregularMeshEdge,
    /// Triangle on the left of the canonical lower-ID to higher-ID direction.
    pub left_triangle: IrregularMeshTriangle,
    /// Triangle on the right of the canonical lower-ID to higher-ID direction.
    pub right_triangle: IrregularMeshTriangle,
    /// Common logical jump record.
    pub jump: GradientJump,
}

/// Field-versus-local-geometry alignment for one irregular Delaunay triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct TriangleFieldAlignmentMetrics {
    /// Stable Delaunay triangle identifier.
    pub triangle: IrregularTriangleId,
    /// Shape-tensor anisotropy from the Delaunay geometry record.
    pub geometry_anisotropy: f64,
    /// Delaunay major axis, if geometrically defined.
    pub geometry_direction: Option<[f64; 2]>,
    /// Principal direction from averaged successful vertex Hessian fits.
    pub curvature_direction: Option<[f64; 2]>,
    /// Absolute direction cosine when both directions are defined.
    pub alignment: Option<f64>,
    /// Number of vertex estimates contributing to the local curvature direction.
    pub curvature_sample_count: usize,
}

/// Shared scalar-field metrics measured through a prepared irregular evaluator.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularFieldMetrics {
    /// Prepared gradients indexed by triangle ID.
    pub triangles: Vec<IrregularTriangleFieldMetrics>,
    /// Vertex-value edge differences and secants indexed by edge ID.
    pub edges: Vec<IrregularEdgeFieldMetrics>,
    /// One-sided derivative jumps for interior Delaunay edges.
    pub gradient_jumps: Vec<IrregularGradientJump>,
    /// Comparable scalar-field distributions with explicit unweighted semantics.
    pub distributions: ScalarFieldDistributionMetrics,
}

impl IrregularTernaryMesh {
    /// Compute deterministic Delaunay geometry and topology metrics.
    ///
    /// These records deliberately apply only to scattered-sample triangulations;
    /// regular grids have analytically uniform geometry and use the shared field
    /// metrics through their separate controlled-reference adapter.
    pub fn metrics(&self) -> IrregularMeshMetrics {
        let triangles = self
            .triangles()
            .map(|triangle| triangle_geometry(self, triangle))
            .collect::<Vec<_>>();
        let triangle_areas = triangles
            .iter()
            .map(|metric| metric.area)
            .collect::<Vec<_>>();
        let edges = self
            .edges()
            .map(|edge| {
                let endpoint = edge
                    .vertices
                    .map(|vertex| self.composition(vertex).expect("mesh vertex"));
                let length = logical_distance(endpoint[0], endpoint[1]);
                let adjacent = edge
                    .triangles
                    .map(|triangle| triangle.map(|id| &triangles[id.0]));
                let adjacent_area_ratio = match adjacent {
                    [Some(left), Some(right)] => Some(ratio(left.area, right.area)),
                    _ => None,
                };
                let adjacent_size_ratio = match adjacent {
                    [Some(left), Some(right)] => Some(ratio(
                        left.edge_lengths.into_iter().fold(0.0_f64, f64::max),
                        right.edge_lengths.into_iter().fold(0.0_f64, f64::max),
                    )),
                    _ => None,
                };
                IrregularEdgeGeometryMetrics {
                    edge,
                    length,
                    adjacent_area_ratio,
                    adjacent_size_ratio,
                }
            })
            .collect::<Vec<_>>();
        let boundary_distances = boundary_graph_distances(self);
        let vertices = self
            .vertex_ids()
            .map(|vertex| {
                let incident = self.incident_triangles(vertex).expect("mesh vertex");
                let incident_areas = incident
                    .iter()
                    .map(|triangle| triangles[triangle.0].area)
                    .collect::<Vec<_>>();
                let edges_for_vertex = self.incident_edges(vertex).expect("mesh vertex");
                let nearest_neighbour_distance = edges_for_vertex
                    .iter()
                    .map(|edge| edges[edge.0].length)
                    .min_by(f64::total_cmp);
                let mean = incident_areas.iter().sum::<f64>() / incident_areas.len() as f64;
                let incident_area_coefficient_of_variation = (mean != 0.0).then(|| {
                    (incident_areas
                        .iter()
                        .map(|area| (area - mean).powi(2))
                        .sum::<f64>()
                        / incident_areas.len() as f64)
                        .sqrt()
                        / mean
                });
                IrregularVertexGeometryMetrics {
                    vertex,
                    valence: edges_for_vertex.len(),
                    barycentric_dual_area: incident_areas.iter().sum::<f64>() / 3.0,
                    nearest_neighbour_distance,
                    boundary_graph_distance: boundary_distances[vertex.0],
                    incident_area_coefficient_of_variation,
                }
            })
            .collect::<Vec<_>>();
        let hull_edge_count = edges.iter().filter(|edge| edge.edge.is_boundary()).count();
        let hull_triangles = self
            .triangles()
            .filter(|triangle| {
                triangle
                    .edges
                    .into_iter()
                    .any(|edge| edges[edge.0].edge.is_boundary())
            })
            .count();
        let summary = IrregularMeshSummary {
            vertex_count: self.vertex_count(),
            edge_count: self.edge_count(),
            triangle_count: self.triangle_count(),
            hull_edge_count,
            hull_triangle_count: hull_triangles,
            convex_hull_area: triangle_areas.iter().sum(),
            convex_hull_perimeter: edges
                .iter()
                .filter(|edge| edge.edge.is_boundary())
                .map(|edge| edge.length)
                .sum(),
            triangle_areas: summary(&triangle_areas),
            edge_lengths: summary(&edges.iter().map(|edge| edge.length).collect::<Vec<_>>()),
            minimum_angles: summary(
                &triangles
                    .iter()
                    .map(|triangle| triangle.minimum_angle)
                    .collect::<Vec<_>>(),
            ),
            maximum_angles: summary(
                &triangles
                    .iter()
                    .map(|triangle| triangle.maximum_angle)
                    .collect::<Vec<_>>(),
            ),
            vertex_valences: summary(
                &vertices
                    .iter()
                    .map(|vertex| vertex.valence as f64)
                    .collect::<Vec<_>>(),
            ),
            nearest_neighbour_distances: summary(
                &vertices
                    .iter()
                    .filter_map(|vertex| vertex.nearest_neighbour_distance)
                    .collect::<Vec<_>>(),
            ),
            boundary_graph_distances: summary(
                &vertices
                    .iter()
                    .map(|vertex| vertex.boundary_graph_distance as f64)
                    .collect::<Vec<_>>(),
            ),
        };
        IrregularMeshMetrics {
            triangles,
            edges,
            vertices,
            summary,
        }
    }
}
impl<'a> InterpolatedIrregularTernaryField<'a> {
    /// Create a derived quantity adapter without rebuilding cubic intervals.
    pub fn derived(
        &self,
        quantity: super::DerivedFieldQuantity,
    ) -> super::DerivedIrregularTernaryField<'_, 'a> {
        super::DerivedIrregularTernaryField::new(self, quantity)
    }

    /// Analyze this prepared irregular field over its immutable Delaunay topology.
    pub fn metrics(&self) -> Result<IrregularFieldMetrics, IrregularFieldEvaluationError> {
        irregular_field_metrics(self)
    }
}

impl IrregularTernaryScalarField {
    /// Estimate sampled-field curvature at a mesh vertex using deterministic graph rings.
    pub fn local_quadratic_estimate(
        &self,
        vertex: IrregularVertexId,
        options: LocalQuadraticOptions,
    ) -> Result<LocalQuadraticEstimate, LocalQuadraticError> {
        if options.max_ring == 0
            || !options.max_condition_estimate.is_finite()
            || options.max_condition_estimate < 1.0
        {
            return Err(LocalQuadraticError::InvalidOptions);
        }
        let mesh = self.mesh();
        let centre = mesh
            .composition(vertex)
            .map(logical_from_composition)
            .map_err(|_| LocalQuadraticError::InsufficientSamples {
                actual: 0,
                required: 6,
            })?;
        let mut seen = BTreeSet::from([vertex]);
        let mut frontier = BTreeSet::from([vertex]);
        let mut last_count = 1;
        for ring in 1..=options.max_ring {
            let mut next = BTreeSet::new();
            for current in &frontier {
                for edge in mesh
                    .incident_edges(*current)
                    .map_err(|_| LocalQuadraticError::NonFinite)?
                {
                    let topology = mesh
                        .edge(*edge)
                        .map_err(|_| LocalQuadraticError::NonFinite)?;
                    let neighbour = if topology.vertices[0] == *current {
                        topology.vertices[1]
                    } else {
                        topology.vertices[0]
                    };
                    if seen.insert(neighbour) {
                        next.insert(neighbour);
                    }
                }
            }
            frontier = next;
            let observations = seen
                .iter()
                .map(|candidate| {
                    (
                        logical_from_composition(
                            mesh.composition(*candidate).expect("mesh vertex"),
                        ),
                        self.values()[candidate.0],
                    )
                })
                .collect::<Vec<_>>();
            last_count = observations.len();
            if observations.len() >= 6 {
                match fit_local_quadratic(centre, &observations, options, ring) {
                    Err(LocalQuadraticError::RankDeficient) if ring < options.max_ring => continue,
                    result => return result,
                }
            }
            if frontier.is_empty() {
                break;
            }
        }
        Err(LocalQuadraticError::InsufficientSamples {
            actual: last_count,
            required: 6,
        })
    }

    /// Correlate Delaunay shape axes with locally fitted sampled-field curvature.
    pub fn triangle_field_alignment(
        &self,
        options: LocalQuadraticOptions,
    ) -> Vec<TriangleFieldAlignmentMetrics> {
        let geometry = self.mesh().metrics();
        let estimates = self
            .mesh()
            .vertex_ids()
            .map(|vertex| self.local_quadratic_estimate(vertex, options).ok())
            .collect::<Vec<_>>();
        geometry
            .triangles
            .iter()
            .map(|metric| {
                let local = metric
                    .triangle
                    .vertices
                    .into_iter()
                    .filter_map(|vertex| estimates[vertex.0].as_ref())
                    .collect::<Vec<_>>();
                let curvature_sample_count = local.len();
                let curvature_direction = (curvature_sample_count > 0)
                    .then(|| {
                        let hessian = [
                            [
                                local
                                    .iter()
                                    .map(|estimate| estimate.hessian_logical_xy[0][0])
                                    .sum::<f64>()
                                    / curvature_sample_count as f64,
                                local
                                    .iter()
                                    .map(|estimate| estimate.hessian_logical_xy[0][1])
                                    .sum::<f64>()
                                    / curvature_sample_count as f64,
                            ],
                            [
                                local
                                    .iter()
                                    .map(|estimate| estimate.hessian_logical_xy[0][1])
                                    .sum::<f64>()
                                    / curvature_sample_count as f64,
                                local
                                    .iter()
                                    .map(|estimate| estimate.hessian_logical_xy[1][1])
                                    .sum::<f64>()
                                    / curvature_sample_count as f64,
                            ],
                        ];
                        principal_hessian_direction(hessian)
                    })
                    .flatten();
                let alignment = metric.major_axis_direction.zip(curvature_direction).map(
                    |(geometry, curvature)| {
                        geometry[0]
                            .mul_add(curvature[0], geometry[1] * curvature[1])
                            .abs()
                    },
                );
                TriangleFieldAlignmentMetrics {
                    triangle: metric.triangle.id,
                    geometry_anisotropy: metric.shape_anisotropy,
                    geometry_direction: metric.major_axis_direction,
                    curvature_direction,
                    alignment,
                    curvature_sample_count,
                }
            })
            .collect()
    }
}

fn irregular_field_metrics(
    evaluator: &InterpolatedIrregularTernaryField<'_>,
) -> Result<IrregularFieldMetrics, IrregularFieldEvaluationError> {
    let field = evaluator.field();
    let mesh = evaluator.mesh();
    let geometry = mesh.metrics();
    let mut triangles = Vec::with_capacity(mesh.triangle_count());
    let mut gradient_norms = Vec::with_capacity(mesh.triangle_count());
    for triangle in mesh.triangles() {
        let (_, gradient_ab) = evaluator.evaluate_in_triangle(triangle, [1.0 / 3.0; 3])?;
        let gradient = TernaryGradient::from_reduced_ab(gradient_ab);
        let shape = &geometry.triangles[triangle.id.0];
        triangles.push(IrregularTriangleFieldMetrics {
            triangle,
            area: shape.area,
            characteristic_size: shape.edge_lengths.into_iter().fold(0.0_f64, f64::max),
            gradient,
            gradient_norm: gradient.norm(),
        });
        gradient_norms.push(gradient.norm());
    }
    let edges = mesh
        .edges()
        .map(|edge| {
            let difference =
                field.values()[edge.vertices[1].0] - field.values()[edge.vertices[0].0];
            let length = geometry.edges[edge.id.0].length;
            IrregularEdgeFieldMetrics {
                edge,
                difference,
                secant_slope: difference / length,
            }
        })
        .collect::<Vec<_>>();
    let mut jumps = Vec::new();
    for edge in mesh.edges().filter(|edge| !edge.is_boundary()) {
        let [first, second] = edge.triangles;
        let (first, second) = (
            first.expect("interior edge"),
            second.expect("interior edge"),
        );
        let first_triangle = mesh.triangle(first).expect("mesh triangle");
        let second_triangle = mesh.triangle(second).expect("mesh triangle");
        let endpoint = edge
            .vertices
            .map(|vertex| mesh.composition(vertex).expect("mesh vertex"));
        let logical = endpoint.map(logical_from_composition);
        let tangent = [logical[1][0] - logical[0][0], logical[1][1] - logical[0][1]];
        let side = |triangle: IrregularMeshTriangle| {
            let third = triangle
                .vertices
                .into_iter()
                .find(|vertex| *vertex != edge.vertices[0] && *vertex != edge.vertices[1])
                .expect("incident triangle has third vertex");
            let third = logical_from_composition(mesh.composition(third).expect("mesh vertex"));
            tangent[0] * (third[1] - logical[0][1]) - tangent[1] * (third[0] - logical[0][0])
        };
        let (left_triangle, right_triangle) = if side(first_triangle) > 0.0 {
            (first_triangle, second_triangle)
        } else {
            (second_triangle, first_triangle)
        };
        let barycentric_for = |triangle: IrregularMeshTriangle| {
            let mut weights = [0.0; 3];
            for (index, vertex) in triangle.vertices.into_iter().enumerate() {
                if vertex == edge.vertices[0] || vertex == edge.vertices[1] {
                    weights[index] = 0.5;
                }
            }
            weights
        };
        let (_, left_gradient) =
            evaluator.evaluate_in_triangle(left_triangle, barycentric_for(left_triangle))?;
        let (_, right_gradient) =
            evaluator.evaluate_in_triangle(right_triangle, barycentric_for(right_triangle))?;
        let position = [
            0.5 * (endpoint[0][0] + endpoint[1][0]),
            0.5 * (endpoint[0][1] + endpoint[1][1]),
            0.5 * (endpoint[0][2] + endpoint[1][2]),
        ];
        let jump = GradientJump::from_gradients(
            position,
            TernaryGradient::from_reduced_ab(left_gradient),
            TernaryGradient::from_reduced_ab(right_gradient),
            tangent,
        )
        .ok_or(IrregularFieldEvaluationError::NonFiniteEvaluation)?;
        jumps.push(IrregularGradientJump {
            edge,
            left_triangle,
            right_triangle,
            jump,
        });
    }
    let mut hessian_norms = Vec::new();
    let mut laplacians = Vec::new();
    let mut anisotropies = Vec::new();
    let mut unavailable = 0;
    for vertex in mesh.vertex_ids() {
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
    let edge_differences = edges.iter().map(|edge| edge.difference).collect::<Vec<_>>();
    let edge_secant_slopes = edges
        .iter()
        .map(|edge| edge.secant_slope)
        .collect::<Vec<_>>();
    let jump_magnitudes = jumps
        .iter()
        .map(|jump| jump.jump.magnitude)
        .collect::<Vec<_>>();
    let distributions = ScalarFieldDistributionMetrics::from_input(ScalarFieldDistributionInput {
        sample_values: field.values(),
        edge_differences: &edge_differences,
        edge_secant_slopes: &edge_secant_slopes,
        gradient_norms: &gradient_norms,
        hessian_norms: &hessian_norms,
        laplacians: &laplacians,
        curvature_anisotropies: &anisotropies,
        gradient_jump_magnitudes: &jump_magnitudes,
        unavailable_local_estimate_count: unavailable,
        non_finite_evaluation_count: 0,
    })
    .expect("prepared field metrics are finite");
    Ok(IrregularFieldMetrics {
        triangles,
        edges,
        gradient_jumps: jumps,
        distributions,
    })
}
fn triangle_geometry(
    mesh: &IrregularTernaryMesh,
    triangle: IrregularMeshTriangle,
) -> IrregularTriangleGeometryMetrics {
    let compositions = triangle
        .vertices
        .map(|vertex| mesh.composition(vertex).expect("mesh triangle vertex"));
    let points = compositions.map(logical_from_composition);
    let edge_lengths = [
        distance(points[0], points[1]),
        distance(points[1], points[2]),
        distance(points[2], points[0]),
    ];
    let perimeter = edge_lengths.iter().sum::<f64>();
    let area = 0.5
        * ((points[1][0] - points[0][0]) * (points[2][1] - points[0][1])
            - (points[1][1] - points[0][1]) * (points[2][0] - points[0][0]))
            .abs();
    let angles = [
        angle(edge_lengths[0], edge_lengths[2], edge_lengths[1]),
        angle(edge_lengths[0], edge_lengths[1], edge_lengths[2]),
        angle(edge_lengths[1], edge_lengths[2], edge_lengths[0]),
    ];
    let longest = edge_lengths.into_iter().fold(0.0_f64, f64::max);
    let inradius = 2.0 * area / perimeter;
    let circumradius = edge_lengths.iter().product::<f64>() / (4.0 * area);
    let radius_ratio = (inradius / circumradius).clamp(0.0, 1.0);
    let mean_ratio = (4.0 * 3.0_f64.sqrt() * area
        / edge_lengths
            .into_iter()
            .map(|length| length * length)
            .sum::<f64>())
    .clamp(0.0, 1.0);
    let altitude_aspect_ratio = longest / (2.0 * area / longest);
    let (shape_anisotropy, major_axis_direction) = shape_tensor(points);
    IrregularTriangleGeometryMetrics {
        triangle,
        area,
        edge_lengths,
        perimeter,
        minimum_angle: angles.into_iter().fold(f64::INFINITY, f64::min),
        maximum_angle: angles.into_iter().fold(0.0_f64, f64::max),
        radius_ratio,
        mean_ratio,
        altitude_aspect_ratio,
        shape_anisotropy,
        major_axis_direction,
    }
}

fn boundary_graph_distances(mesh: &IrregularTernaryMesh) -> Vec<usize> {
    let mut distances = vec![usize::MAX; mesh.vertex_count()];
    let mut queue = VecDeque::new();
    for edge in mesh.boundary_edges() {
        for vertex in edge.vertices {
            if distances[vertex.0] == usize::MAX {
                distances[vertex.0] = 0;
                queue.push_back(vertex);
            }
        }
    }
    while let Some(vertex) = queue.pop_front() {
        let next_distance = distances[vertex.0] + 1;
        for edge in mesh.incident_edges(vertex).expect("mesh vertex") {
            let topology = mesh.edge(*edge).expect("mesh edge");
            let neighbour = if topology.vertices[0] == vertex {
                topology.vertices[1]
            } else {
                topology.vertices[0]
            };
            if distances[neighbour.0] == usize::MAX {
                distances[neighbour.0] = next_distance;
                queue.push_back(neighbour);
            }
        }
    }
    distances
}

fn summary(values: &[f64]) -> super::DistributionSummary {
    super::DistributionSummary::from_values(values).expect("mesh geometry is finite")
}
fn ratio(left: f64, right: f64) -> f64 {
    let ratio = left / right;
    if ratio < 1.0 { 1.0 / ratio } else { ratio }
}
fn distance(left: [f64; 2], right: [f64; 2]) -> f64 {
    (left[0] - right[0]).hypot(left[1] - right[1])
}
fn angle(left: f64, right: f64, opposite: f64) -> f64 {
    ((left * left + right * right - opposite * opposite) / (2.0 * left * right))
        .clamp(-1.0, 1.0)
        .acos()
}
fn shape_tensor(points: [[f64; 2]; 3]) -> (f64, Option<[f64; 2]>) {
    let centre = [
        points.iter().map(|point| point[0]).sum::<f64>() / 3.0,
        points.iter().map(|point| point[1]).sum::<f64>() / 3.0,
    ];
    let xx = points
        .iter()
        .map(|point| (point[0] - centre[0]).powi(2))
        .sum::<f64>()
        / 3.0;
    let xy = points
        .iter()
        .map(|point| (point[0] - centre[0]) * (point[1] - centre[1]))
        .sum::<f64>()
        / 3.0;
    let yy = points
        .iter()
        .map(|point| (point[1] - centre[1]).powi(2))
        .sum::<f64>()
        / 3.0;
    let direction = principal_hessian_direction([[xx, xy], [xy, yy]]);
    let half_difference = 0.5 * (xx - yy);
    let radius = half_difference.hypot(xy);
    let minimum = 0.5 * (xx + yy) - radius;
    let maximum = 0.5 * (xx + yy) + radius;
    let anisotropy = if minimum <= 1.0e-14 {
        1.0
    } else {
        (maximum / minimum).sqrt()
    };
    (
        anisotropy,
        direction.filter(|_| (maximum - minimum).abs() > 1.0e-13),
    )
}
fn principal_hessian_direction(hessian: [[f64; 2]; 2]) -> Option<[f64; 2]> {
    let midpoint = 0.5 * (hessian[0][0] + hessian[1][1]);
    let radius = (0.5 * (hessian[0][0] - hessian[1][1])).hypot(hessian[0][1]);
    let selected = midpoint + radius;
    let mut direction = [hessian[0][1], selected - hessian[0][0]];
    if direction[0].hypot(direction[1]) <= 1.0e-14 {
        direction = [selected - hessian[1][1], hessian[0][1]];
    }
    let norm = direction[0].hypot(direction[1]);
    (norm > 1.0e-14).then(|| {
        let mut direction = [direction[0] / norm, direction[1] / norm];
        if direction[0] < 0.0 || (direction[0] == 0.0 && direction[1] < 0.0) {
            direction = [-direction[0], -direction[1]];
        }
        direction
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IrregularFieldInterpolation;

    fn scattered_samples() -> Vec<[f64; 3]> {
        vec![
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.57, 0.28, 0.15],
            [0.18, 0.61, 0.21],
            [0.23, 0.16, 0.61],
            [0.31, 0.42, 0.27],
        ]
    }
    #[test]
    fn affine_metrics_have_exact_shared_gradient_and_zero_jumps() {
        let mesh = IrregularTernaryMesh::new(scattered_samples()).unwrap();
        let field =
            IrregularTernaryScalarField::from_fn(mesh, |[a, b, c]| 2.0 * a - 3.0 * b + c).unwrap();
        let evaluator =
            InterpolatedIrregularTernaryField::new(&field, IrregularFieldInterpolation::Linear)
                .unwrap();
        let metrics = evaluator.metrics().unwrap();
        assert!(
            metrics
                .gradient_jumps
                .iter()
                .all(|jump| jump.jump.magnitude < 1.0e-10)
        );
        assert!(metrics.triangles.iter().all(|triangle| {
            let actual = triangle.gradient.reduced_ab();
            (actual[0] - 1.0).abs() < 1.0e-10 && (actual[1] + 4.0).abs() < 1.0e-10
        }));
    }

    #[test]
    fn mesh_metrics_report_hull_and_interior_graph_distance() {
        let mesh = IrregularTernaryMesh::new(scattered_samples()).unwrap();
        let metrics = mesh.metrics();
        assert_eq!(metrics.summary.hull_edge_count, 3);
        assert!(
            metrics
                .vertices
                .iter()
                .any(|vertex| vertex.boundary_graph_distance > 0)
        );
        assert!(metrics.summary.convex_hull_area > 0.0);
    }
}
