//! Delaunay-backed irregular two-dimensional ternary meshes and scalar fields.
//!
//! `delaunay` supplies construction and robust point location. Its handles and
//! types remain private implementation details of this module.

#![forbid(unsafe_code)]

use core::fmt;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::atomic::{AtomicU64, Ordering},
};

use delaunay::{
    DelaunayTriangulation, DelaunayTriangulationBuilder,
    algorithms::LocateResult,
    geometry::{Point, kernel::AdaptiveKernel},
    tds::{SimplexKey, Vertex, VertexKey},
};

mod cubic;
pub use cubic::*;

use crate::{
    POINT_LOCATION_TOLERANCE,
    simplex::{
        barycentric_ab, canonical_barycentric, global_gradient_ab, logical_from_composition,
        valid_barycentric,
    },
};

/// Tolerance for duplicate irregular samples in the logical equilateral plane.
pub const IRREGULAR_VERTEX_TOLERANCE: f64 = POINT_LOCATION_TOLERANCE;

static NEXT_MESH_ID: AtomicU64 = AtomicU64::new(1);
type BackendTriangulation = DelaunayTriangulation<AdaptiveKernel<f64>, usize, (), 2>;

/// Stable identifier for an immutable irregular mesh vertex.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IrregularVertexId(pub usize);
/// Stable identifier for an immutable undirected irregular mesh edge.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IrregularEdgeId(pub usize);
/// Stable identifier for an immutable Delaunay triangle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IrregularTriangleId(pub usize);

/// One canonical undirected mesh edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrregularMeshEdge {
    /// Dense edge identifier.
    pub id: IrregularEdgeId,
    /// Endpoints ordered by identifier.
    pub vertices: [IrregularVertexId; 2],
    /// One incident triangle at the hull and two in the interior.
    pub triangles: [Option<IrregularTriangleId>; 2],
}
impl IrregularMeshEdge {
    /// Whether this is a convex-hull edge.
    pub const fn is_boundary(self) -> bool {
        self.triangles[1].is_none()
    }
    /// Number of incident triangles.
    pub fn incident_triangle_count(self) -> usize {
        usize::from(self.triangles[0].is_some()) + usize::from(self.triangles[1].is_some())
    }
}

/// One counter-clockwise mesh triangle in local vertex order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrregularMeshTriangle {
    /// Dense triangle identifier.
    pub id: IrregularTriangleId,
    /// Counter-clockwise vertex IDs, rotated so the smallest comes first.
    pub vertices: [IrregularVertexId; 3],
    /// Edge IDs in `(0, 1)`, `(1, 2)`, `(2, 0)` order.
    pub edges: [IrregularEdgeId; 3],
}

/// Boundary classification for an irregular-mesh query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum IrregularPointBoundaryLocation {
    /// Strictly inside a triangle.
    Interior,
    /// On an edge shared by two triangles.
    InteriorEdge { edge: IrregularEdgeId },
    /// On a convex-hull edge.
    BoundaryEdge { edge: IrregularEdgeId },
    /// At a mesh vertex.
    Vertex { vertex: IrregularVertexId },
}

/// A normalized composition located in an immutable irregular mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocatedIrregularTriangle {
    /// Deterministically selected containing triangle.
    pub triangle: IrregularMeshTriangle,
    /// Barycentric weights matching `triangle.vertices`.
    pub barycentric: [f64; 3],
    /// Boundary classification after tolerance snapping.
    pub boundary: IrregularPointBoundaryLocation,
    mesh_identity: u64,
}

/// Failure while constructing or inspecting an irregular mesh.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum IrregularMeshError {
    /// Fewer than three samples were supplied.
    TooFewSamples { actual: usize },
    /// A sample component was not finite.
    NonFiniteComposition {
        sample: usize,
        component: usize,
        value: f64,
    },
    /// A sample did not sum to one within the location tolerance.
    InvalidCompositionSum { sample: usize, sum: f64 },
    /// A sample lies outside the semantic ternary simplex.
    OutsideSimplex {
        sample: usize,
        composition: [f64; 3],
    },
    /// Two samples are coincident or near-coincident.
    DuplicateComposition { first: usize, second: usize },
    /// The backend rejected the input geometry.
    TriangulationFailed { message: String },
    /// A vertex ID does not belong to this mesh.
    InvalidVertex { vertex: IrregularVertexId },
    /// An edge ID does not belong to this mesh.
    InvalidEdge { edge: IrregularEdgeId },
    /// A triangle ID does not belong to this mesh.
    InvalidTriangle { triangle: IrregularTriangleId },
    /// Backend output could not be represented as a planar triangle mesh.
    InvalidTopology { message: String },
}
impl fmt::Display for IrregularMeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewSamples { actual } => write!(
                f,
                "irregular mesh requires at least three samples; received {actual}"
            ),
            Self::NonFiniteComposition {
                sample,
                component,
                value,
            } => write!(
                f,
                "sample {sample} composition component {component} is not finite: {value:?}"
            ),
            Self::InvalidCompositionSum { sample, sum } => write!(
                f,
                "sample {sample} components must sum to one within {POINT_LOCATION_TOLERANCE:e}; received {sum:?}"
            ),
            Self::OutsideSimplex {
                sample,
                composition,
            } => write!(
                f,
                "sample {sample} composition {composition:?} lies outside the ternary simplex"
            ),
            Self::DuplicateComposition { first, second } => write!(
                f,
                "samples {first} and {second} are duplicate or near-duplicate compositions"
            ),
            Self::TriangulationFailed { message } => {
                write!(f, "Delaunay triangulation construction failed: {message}")
            }
            Self::InvalidVertex { vertex } => write!(
                f,
                "vertex identifier {vertex:?} does not belong to this mesh"
            ),
            Self::InvalidEdge { edge } => {
                write!(f, "edge identifier {edge:?} does not belong to this mesh")
            }
            Self::InvalidTriangle { triangle } => write!(
                f,
                "triangle identifier {triangle:?} does not belong to this mesh"
            ),
            Self::InvalidTopology { message } => {
                write!(f, "Delaunay topology could not be represented: {message}")
            }
        }
    }
}
impl std::error::Error for IrregularMeshError {}

/// Failure while locating a composition in an irregular mesh.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum IrregularPointLocationError {
    /// A query component was not finite.
    NonFiniteComposition { component: usize, value: f64 },
    /// A query did not sum to one within the location tolerance.
    InvalidCompositionSum { sum: f64 },
    /// A query lies outside the ternary simplex.
    OutsideSimplex { composition: [f64; 3] },
    /// A query lies outside the mesh convex hull.
    OutsideConvexHull { composition: [f64; 3] },
    /// A supplied walking hint is not a mesh triangle.
    InvalidHint { triangle: IrregularTriangleId },
    /// A backend location result could not be used safely.
    BackendFailure { message: String },
}
impl fmt::Display for IrregularPointLocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteComposition { component, value } => write!(
                f,
                "composition component {component} is not finite: {value:?}"
            ),
            Self::InvalidCompositionSum { sum } => write!(
                f,
                "composition components must sum to one within {POINT_LOCATION_TOLERANCE:e}; received {sum:?}"
            ),
            Self::OutsideSimplex { composition } => write!(
                f,
                "composition {composition:?} lies outside the ternary simplex"
            ),
            Self::OutsideConvexHull { composition } => write!(
                f,
                "composition {composition:?} lies outside the irregular mesh convex hull"
            ),
            Self::InvalidHint { triangle } => {
                write!(f, "location hint {triangle:?} does not belong to this mesh")
            }
            Self::BackendFailure { message } => {
                write!(f, "Delaunay point location failed: {message}")
            }
        }
    }
}
impl std::error::Error for IrregularPointLocationError {}

/// Failure while constructing an irregular scalar field.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum IrregularFieldError {
    /// Values do not match the mesh vertex count.
    ValueCountMismatch { expected: usize, actual: usize },
    /// A scalar value was not finite.
    NonFiniteValue {
        vertex: IrregularVertexId,
        value: f64,
    },
    /// A vertex does not belong to this field's mesh.
    InvalidVertex { vertex: IrregularVertexId },
}
impl fmt::Display for IrregularFieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValueCountMismatch { expected, actual } => write!(
                f,
                "irregular field requires {expected} values; received {actual}"
            ),
            Self::NonFiniteValue { vertex, value } => write!(
                f,
                "scalar value at vertex {vertex:?} is not finite: {value:?}"
            ),
            Self::InvalidVertex { vertex } => write!(
                f,
                "vertex identifier {vertex:?} does not belong to this field"
            ),
        }
    }
}
impl std::error::Error for IrregularFieldError {}

/// Failure while evaluating a prepared irregular field.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum IrregularFieldEvaluationError {
    /// Point validation or location failed.
    PointLocation(IrregularPointLocationError),
    /// Cubic-alpha evaluation was selected without the `irregular-cubic-alpha` feature.
    CubicFeatureUnavailable,
    /// Construction of the self-consistent irregular cubic-alpha field failed.
    CubicConstruction(Box<IrregularCubicAlphaBuildError>),
    /// A cached location belongs to another mesh.
    IncompatibleLocation,
    /// A cached location is malformed.
    InvalidLocation { triangle: IrregularTriangleId },
    /// Finite input samples produced a non-finite interpolated value or gradient.
    NonFiniteEvaluation,
    /// Caller-owned batch output has the wrong length.
    OutputSizeMismatch { expected: usize, actual: usize },
}
impl fmt::Display for IrregularFieldEvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PointLocation(error) => write!(f, "point location failed: {error}"),
            Self::CubicFeatureUnavailable => f.write_str(
                "irregular cubic-alpha field evaluation requires the `irregular-cubic-alpha` feature",
            ),
            Self::CubicConstruction(error) => {
                write!(f, "irregular cubic-alpha field construction failed: {error}")
            }
            Self::IncompatibleLocation => {
                f.write_str("location belongs to an incompatible irregular mesh")
            }
            Self::InvalidLocation { triangle } => write!(
                f,
                "location does not describe canonical triangle {triangle:?}"
            ),
            Self::NonFiniteEvaluation => {
                f.write_str("interpolation produced a non-finite value or gradient")
            }
            Self::OutputSizeMismatch { expected, actual } => write!(
                f,
                "batch output requires {expected} values; received {actual}"
            ),
        }
    }
}
impl std::error::Error for IrregularFieldEvaluationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PointLocation(error) => Some(error),
            Self::CubicConstruction(error) => Some(error),
            _ => None,
        }
    }
}
impl From<IrregularPointLocationError> for IrregularFieldEvaluationError {
    fn from(value: IrregularPointLocationError) -> Self {
        Self::PointLocation(value)
    }
}

/// Immutable topology for scattered ternary composition samples.
///
/// Inputs retain semantic A/B/C order. The private triangulation backend uses
/// `A=(0,0)`, `B=(1,0)`, and `C=(1/2,sqrt(3)/2)`, so component permutations are
/// Euclidean symmetries of the logical plane. The supported domain is the
/// convex hull of samples only.
#[derive(Clone, Debug)]
pub struct IrregularTernaryMesh {
    compositions: Vec<[f64; 3]>,
    vertex_lookup: HashMap<[u64; 3], IrregularVertexId>,
    edges: Vec<IrregularMeshEdge>,
    triangles: Vec<IrregularMeshTriangle>,
    vertex_triangles: Vec<Vec<IrregularTriangleId>>,
    vertex_edges: Vec<Vec<IrregularEdgeId>>,
    backend: BackendTriangulation,
    backend_triangles: HashMap<SimplexKey, IrregularTriangleId>,
    triangle_backend_keys: Vec<SimplexKey>,
    identity: u64,
}

#[derive(Clone, Copy)]
struct CandidateTriangle {
    vertices: [IrregularVertexId; 3],
    backend_key: SimplexKey,
}

impl IrregularTernaryMesh {
    pub(crate) const fn has_same_identity(&self, other: &Self) -> bool {
        self.identity == other.identity
    }

    /// Build an immutable two-dimensional Delaunay mesh from semantic A/B/C samples.
    ///
    /// Samples must be finite, normalized within [`POINT_LOCATION_TOLERANCE`],
    /// and inside the simplex. The mesh has no holes, constrained edges, or
    /// non-convex-domain support.
    pub fn new(samples: impl IntoIterator<Item = [f64; 3]>) -> Result<Self, IrregularMeshError> {
        let compositions = samples
            .into_iter()
            .enumerate()
            .map(|(sample, composition)| normalize_sample(sample, composition))
            .collect::<Result<Vec<_>, _>>()?;
        if compositions.len() < 3 {
            return Err(IrregularMeshError::TooFewSamples {
                actual: compositions.len(),
            });
        }
        reject_duplicate_samples(&compositions)?;
        let vertex_lookup = compositions
            .iter()
            .copied()
            .enumerate()
            .map(|(index, composition)| (composition_key(composition), IrregularVertexId(index)))
            .collect();
        let vertices = compositions
            .iter()
            .copied()
            .enumerate()
            .map(|(index, composition)| {
                Vertex::<usize, 2>::try_new_with_data(logical_from_composition(composition), index)
                    .map_err(|error| IrregularMeshError::TriangulationFailed {
                        message: error.to_string(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let backend = DelaunayTriangulationBuilder::new(&vertices)
            .build()
            .map_err(|error| IrregularMeshError::TriangulationFailed {
                message: error.to_string(),
            })?;
        backend
            .validate()
            .map_err(|error| IrregularMeshError::TriangulationFailed {
                message: error.to_string(),
            })?;
        if backend.number_of_simplices() == 0 {
            return Err(IrregularMeshError::TriangulationFailed {
                message: "samples do not span a two-dimensional domain".to_owned(),
            });
        }

        let mut candidates = backend
            .simplices()
            .map(|(backend_key, simplex)| {
                let ids = simplex
                    .vertices()
                    .iter()
                    .map(|&key| {
                        backend
                            .vertex(key)
                            .and_then(|vertex| vertex.data())
                            .copied()
                            .map(IrregularVertexId)
                            .ok_or_else(|| IrregularMeshError::InvalidTopology {
                                message: "backend simplex vertex lacks input identity".to_owned(),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let vertices: [IrregularVertexId; 3] =
                    ids.try_into()
                        .map_err(|_| IrregularMeshError::InvalidTopology {
                            message: "backend simplex is not a triangle".to_owned(),
                        })?;
                Ok(CandidateTriangle {
                    vertices: canonical_counterclockwise_triangle(vertices, &compositions)?,
                    backend_key,
                })
            })
            .collect::<Result<Vec<_>, IrregularMeshError>>()?;
        candidates.sort_unstable_by_key(|candidate| candidate.vertices.map(|vertex| vertex.0));
        if candidates
            .windows(2)
            .any(|pair| pair[0].vertices == pair[1].vertices)
        {
            return Err(IrregularMeshError::InvalidTopology {
                message: "backend returned duplicate triangles".to_owned(),
            });
        }

        let edge_keys = candidates
            .iter()
            .flat_map(|candidate| triangle_edge_keys(candidate.vertices))
            .collect::<BTreeSet<_>>();
        let mut edge_lookup = BTreeMap::new();
        let mut edges = Vec::with_capacity(edge_keys.len());
        for (index, key) in edge_keys.into_iter().enumerate() {
            let id = IrregularEdgeId(index);
            edge_lookup.insert(key, id);
            edges.push(IrregularMeshEdge {
                id,
                vertices: [IrregularVertexId(key[0]), IrregularVertexId(key[1])],
                triangles: [None, None],
            });
        }

        let mut triangles = Vec::with_capacity(candidates.len());
        let mut vertex_triangles = vec![Vec::new(); compositions.len()];
        let mut backend_triangles = HashMap::with_capacity(candidates.len());
        let mut triangle_backend_keys = Vec::with_capacity(candidates.len());
        for (index, candidate) in candidates.into_iter().enumerate() {
            let id = IrregularTriangleId(index);
            let keys = triangle_edge_keys(candidate.vertices);
            let triangle_edges = keys.map(|key| {
                edge_lookup
                    .get(&key)
                    .copied()
                    .ok_or_else(|| IrregularMeshError::InvalidTopology {
                        message: "triangle edge has no stable identifier".to_owned(),
                    })
            });
            let [first_edge, second_edge, third_edge] = triangle_edges;
            let triangle_edges = [first_edge?, second_edge?, third_edge?];
            for edge in triangle_edges {
                let slots = &mut edges[edge.0].triangles;
                if slots[0].is_none() {
                    slots[0] = Some(id);
                } else if slots[1].is_none() {
                    slots[1] = Some(id);
                } else {
                    return Err(IrregularMeshError::InvalidTopology {
                        message: "an edge has more than two incident triangles".to_owned(),
                    });
                }
            }
            for vertex in candidate.vertices {
                vertex_triangles[vertex.0].push(id);
            }
            triangles.push(IrregularMeshTriangle {
                id,
                vertices: candidate.vertices,
                edges: triangle_edges,
            });
            backend_triangles.insert(candidate.backend_key, id);
            triangle_backend_keys.push(candidate.backend_key);
        }
        if edges.iter().any(|edge| edge.triangles[0].is_none()) {
            return Err(IrregularMeshError::InvalidTopology {
                message: "an edge has no incident triangle".to_owned(),
            });
        }
        let mut vertex_edges = vec![Vec::new(); compositions.len()];
        for edge in &edges {
            vertex_edges[edge.vertices[0].0].push(edge.id);
            vertex_edges[edge.vertices[1].0].push(edge.id);
        }
        Ok(Self {
            compositions,
            vertex_lookup,
            edges,
            triangles,
            vertex_triangles,
            vertex_edges,
            backend,
            backend_triangles,
            triangle_backend_keys,
            identity: NEXT_MESH_ID.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// Number of mesh vertices.
    pub fn vertex_count(&self) -> usize {
        self.compositions.len()
    }
    /// Number of unique undirected mesh edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
    /// Number of Delaunay triangles.
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }
    /// Iterate over dense vertex identifiers.
    pub fn vertex_ids(&self) -> impl ExactSizeIterator<Item = IrregularVertexId> + '_ {
        (0..self.vertex_count()).map(IrregularVertexId)
    }
    /// Iterate over canonical mesh edges.
    pub fn edges(&self) -> impl ExactSizeIterator<Item = IrregularMeshEdge> + '_ {
        self.edges.iter().copied()
    }
    /// Iterate over convex-hull edges in canonical edge-ID order.
    ///
    /// These are the edges needing explicit boundary treatment in a future
    /// irregular cubic-alpha reconstruction.
    pub fn boundary_edges(&self) -> impl Iterator<Item = IrregularMeshEdge> + '_ {
        self.edges().filter(|edge| edge.is_boundary())
    }
    /// Iterate over mesh triangles in stable ID order.
    pub fn triangles(&self) -> impl ExactSizeIterator<Item = IrregularMeshTriangle> + '_ {
        self.triangles.iter().copied()
    }
    /// Return one vertex composition.
    pub fn composition(&self, vertex: IrregularVertexId) -> Result<[f64; 3], IrregularMeshError> {
        self.compositions
            .get(vertex.0)
            .copied()
            .ok_or(IrregularMeshError::InvalidVertex { vertex })
    }
    /// Return one edge.
    pub fn edge(&self, edge: IrregularEdgeId) -> Result<IrregularMeshEdge, IrregularMeshError> {
        self.edges
            .get(edge.0)
            .copied()
            .ok_or(IrregularMeshError::InvalidEdge { edge })
    }
    /// Return one triangle.
    pub fn triangle(
        &self,
        triangle: IrregularTriangleId,
    ) -> Result<IrregularMeshTriangle, IrregularMeshError> {
        self.triangles
            .get(triangle.0)
            .copied()
            .ok_or(IrregularMeshError::InvalidTriangle { triangle })
    }
    /// Return one triangle's semantic vertex compositions in local order.
    pub fn triangle_compositions(
        &self,
        triangle: IrregularTriangleId,
    ) -> Result<[[f64; 3]; 3], IrregularMeshError> {
        Ok(self
            .triangle(triangle)?
            .vertices
            .map(|vertex| self.compositions[vertex.0]))
    }
    /// Return triangle IDs incident to one vertex in stable order.
    pub fn incident_triangles(
        &self,
        vertex: IrregularVertexId,
    ) -> Result<&[IrregularTriangleId], IrregularMeshError> {
        self.vertex_triangles
            .get(vertex.0)
            .map(Vec::as_slice)
            .ok_or(IrregularMeshError::InvalidVertex { vertex })
    }
    /// Return canonical edge IDs incident to one vertex in stable edge-ID order.
    pub fn incident_edges(
        &self,
        vertex: IrregularVertexId,
    ) -> Result<&[IrregularEdgeId], IrregularMeshError> {
        self.vertex_edges
            .get(vertex.0)
            .map(Vec::as_slice)
            .ok_or(IrregularMeshError::InvalidVertex { vertex })
    }

    /// Locate a composition with backend facet walking.
    pub fn locate(
        &self,
        composition: [f64; 3],
    ) -> Result<LocatedIrregularTriangle, IrregularPointLocationError> {
        self.locate_with_hint(composition, None)
    }

    /// Locate a composition with an optional previous triangle as a spatial hint.
    ///
    /// A hint only affects walking work. Shared-edge and vertex ownership always
    /// resolves to the lowest stable incident triangle ID.
    pub fn locate_with_hint(
        &self,
        composition: [f64; 3],
        hint: Option<IrregularTriangleId>,
    ) -> Result<LocatedIrregularTriangle, IrregularPointLocationError> {
        let composition = normalize_query(composition)?;
        let backend_hint = match hint {
            Some(triangle) => Some(
                *self
                    .triangle_backend_keys
                    .get(triangle.0)
                    .ok_or(IrregularPointLocationError::InvalidHint { triangle })?,
            ),
            None => None,
        };
        if let Some(vertex) = self
            .vertex_lookup
            .get(&composition_key(composition))
            .copied()
        {
            let owner = *self.vertex_triangles[vertex.0].first().ok_or_else(|| {
                IrregularPointLocationError::BackendFailure {
                    message: "located vertex has no incident triangle".to_owned(),
                }
            })?;
            let triangle = self.triangles[owner.0];
            let barycentric = canonical_barycentric(
                barycentric_ab(
                    self.triangle_compositions(owner)
                        .map_err(mesh_to_location_error)?,
                    composition,
                )
                .ok_or_else(|| IrregularPointLocationError::BackendFailure {
                    message: "owner triangle is degenerate".to_owned(),
                })?,
                POINT_LOCATION_TOLERANCE,
            )
            .ok_or_else(|| IrregularPointLocationError::BackendFailure {
                message: "owner triangle produced invalid barycentric coordinates".to_owned(),
            })?;
            return Ok(LocatedIrregularTriangle {
                triangle,
                barycentric,
                boundary: IrregularPointBoundaryLocation::Vertex { vertex },
                mesh_identity: self.identity,
            });
        }
        let point = Point::try_new(logical_from_composition(composition)).map_err(|error| {
            IrregularPointLocationError::BackendFailure {
                message: error.to_string(),
            }
        })?;
        let result = self.backend.locate(&point, backend_hint).map_err(|error| {
            IrregularPointLocationError::BackendFailure {
                message: error.to_string(),
            }
        })?;
        let (seed, declared_edge, declared_vertex) = match result {
            LocateResult::InsideSimplex(simplex) => (self.backend_triangle(simplex)?, None, None),
            LocateResult::OnFacet(simplex, facet) => (
                self.backend_triangle(simplex)?,
                Some(self.backend_facet_edge(simplex, usize::from(facet))?),
                None,
            ),
            LocateResult::OnEdge(simplex) => (self.backend_triangle(simplex)?, None, None),
            LocateResult::OnVertex(vertex) => (
                self.backend_triangle_for_vertex(vertex)?,
                None,
                Some(self.backend_vertex(vertex)?),
            ),
            LocateResult::Outside => {
                return self
                    .boundary_location_from_outside(composition)
                    .ok_or(IrregularPointLocationError::OutsideConvexHull { composition });
            }
        };
        let seed_triangle = self.triangles[seed.0];
        let seed_barycentric = canonical_barycentric(
            barycentric_ab(
                self.triangle_compositions(seed)
                    .map_err(mesh_to_location_error)?,
                composition,
            )
            .ok_or_else(|| IrregularPointLocationError::BackendFailure {
                message: "selected backend triangle is degenerate".to_owned(),
            })?,
            POINT_LOCATION_TOLERANCE,
        )
        .ok_or_else(|| IrregularPointLocationError::BackendFailure {
            message: "backend location produced invalid barycentric coordinates".to_owned(),
        })?;
        let vertex = declared_vertex.or_else(|| {
            seed_barycentric
                .iter()
                .position(|weight| *weight == 1.0)
                .map(|index| seed_triangle.vertices[index])
        });
        let edge = if vertex.is_none() {
            declared_edge.or_else(|| {
                seed_barycentric
                    .iter()
                    .position(|weight| *weight == 0.0)
                    .and_then(|index| self.edge_for_opposite_vertex(seed_triangle, index))
            })
        } else {
            None
        };
        let owner = if let Some(vertex) = vertex {
            *self.vertex_triangles[vertex.0].first().ok_or_else(|| {
                IrregularPointLocationError::BackendFailure {
                    message: "located vertex has no incident triangle".to_owned(),
                }
            })?
        } else if let Some(edge) = edge {
            self.edges[edge.0].triangles[0].ok_or_else(|| {
                IrregularPointLocationError::BackendFailure {
                    message: "located edge has no incident triangle".to_owned(),
                }
            })?
        } else {
            seed
        };
        let triangle = self.triangles[owner.0];
        let barycentric = canonical_barycentric(
            barycentric_ab(
                self.triangle_compositions(owner)
                    .map_err(mesh_to_location_error)?,
                composition,
            )
            .ok_or_else(|| IrregularPointLocationError::BackendFailure {
                message: "owner triangle is degenerate".to_owned(),
            })?,
            POINT_LOCATION_TOLERANCE,
        )
        .ok_or_else(|| IrregularPointLocationError::BackendFailure {
            message: "owner triangle produced invalid barycentric coordinates".to_owned(),
        })?;
        let boundary = match (vertex, edge) {
            (Some(vertex), _) => IrregularPointBoundaryLocation::Vertex { vertex },
            (None, Some(edge)) if self.edges[edge.0].is_boundary() => {
                IrregularPointBoundaryLocation::BoundaryEdge { edge }
            }
            (None, Some(edge)) => IrregularPointBoundaryLocation::InteriorEdge { edge },
            (None, None) => IrregularPointBoundaryLocation::Interior,
        };
        Ok(LocatedIrregularTriangle {
            triangle,
            barycentric,
            boundary,
            mesh_identity: self.identity,
        })
    }

    fn boundary_location_from_outside(
        &self,
        composition: [f64; 3],
    ) -> Option<LocatedIrregularTriangle> {
        self.edges
            .iter()
            .filter(|edge| edge.is_boundary())
            .find_map(|edge| {
                let owner = edge.triangles[0]?;
                let triangle = self.triangles[owner.0];
                let barycentric = canonical_barycentric(
                    barycentric_ab(self.triangle_compositions(owner).ok()?, composition)?,
                    POINT_LOCATION_TOLERANCE,
                )?;
                let local_edge = triangle
                    .edges
                    .iter()
                    .position(|candidate| *candidate == edge.id)?;
                let opposite_vertex = (local_edge + 2) % 3;
                (barycentric[opposite_vertex] == 0.0).then_some(LocatedIrregularTriangle {
                    triangle,
                    barycentric,
                    boundary: IrregularPointBoundaryLocation::BoundaryEdge { edge: edge.id },
                    mesh_identity: self.identity,
                })
            })
    }

    fn backend_triangle(
        &self,
        simplex: SimplexKey,
    ) -> Result<IrregularTriangleId, IrregularPointLocationError> {
        self.backend_triangles
            .get(&simplex)
            .copied()
            .ok_or_else(|| IrregularPointLocationError::BackendFailure {
                message: "backend point location returned an unknown simplex".to_owned(),
            })
    }
    fn backend_vertex(
        &self,
        vertex: VertexKey,
    ) -> Result<IrregularVertexId, IrregularPointLocationError> {
        self.backend
            .vertex(vertex)
            .and_then(|vertex| vertex.data())
            .copied()
            .map(IrregularVertexId)
            .ok_or_else(|| IrregularPointLocationError::BackendFailure {
                message: "backend point location returned an unknown vertex".to_owned(),
            })
    }
    fn backend_triangle_for_vertex(
        &self,
        vertex: VertexKey,
    ) -> Result<IrregularTriangleId, IrregularPointLocationError> {
        let vertex = self.backend_vertex(vertex)?;
        self.vertex_triangles[vertex.0]
            .first()
            .copied()
            .ok_or_else(|| IrregularPointLocationError::BackendFailure {
                message: "backend vertex has no incident triangle".to_owned(),
            })
    }
    fn backend_facet_edge(
        &self,
        simplex: SimplexKey,
        facet: usize,
    ) -> Result<IrregularEdgeId, IrregularPointLocationError> {
        let simplex = self.backend.simplex(simplex).ok_or_else(|| {
            IrregularPointLocationError::BackendFailure {
                message: "backend returned a stale simplex".to_owned(),
            }
        })?;
        if facet >= simplex.vertices().len() {
            return Err(IrregularPointLocationError::BackendFailure {
                message: "backend returned an invalid facet index".to_owned(),
            });
        }
        let vertices = simplex
            .vertices()
            .iter()
            .enumerate()
            .filter_map(|(index, &vertex)| (index != facet).then_some(vertex))
            .map(|vertex| self.backend_vertex(vertex))
            .collect::<Result<Vec<_>, _>>()?;
        let [first, second]: [IrregularVertexId; 2] =
            vertices
                .try_into()
                .map_err(|_| IrregularPointLocationError::BackendFailure {
                    message: "two-dimensional backend facet did not have two vertices".to_owned(),
                })?;
        self.edge_by_vertices(first, second).ok_or_else(|| {
            IrregularPointLocationError::BackendFailure {
                message: "backend facet does not map to a mesh edge".to_owned(),
            }
        })
    }
    fn edge_for_opposite_vertex(
        &self,
        triangle: IrregularMeshTriangle,
        opposite: usize,
    ) -> Option<IrregularEdgeId> {
        let (first, second) = match opposite {
            0 => (triangle.vertices[1], triangle.vertices[2]),
            1 => (triangle.vertices[2], triangle.vertices[0]),
            2 => (triangle.vertices[0], triangle.vertices[1]),
            _ => return None,
        };
        self.edge_by_vertices(first, second)
    }
    fn edge_by_vertices(
        &self,
        first: IrregularVertexId,
        second: IrregularVertexId,
    ) -> Option<IrregularEdgeId> {
        let key = edge_key(first, second);
        self.edges
            .binary_search_by_key(&key, |edge| [edge.vertices[0].0, edge.vertices[1].0])
            .ok()
            .map(IrregularEdgeId)
    }
}

/// Finite scalar samples in dense [`IrregularVertexId`] order.
#[derive(Clone, Debug)]
pub struct IrregularTernaryScalarField {
    mesh: IrregularTernaryMesh,
    values: Vec<f64>,
}
impl IrregularTernaryScalarField {
    /// Construct a finite scalar field that owns its immutable mesh.
    pub fn new(mesh: IrregularTernaryMesh, values: Vec<f64>) -> Result<Self, IrregularFieldError> {
        if values.len() != mesh.vertex_count() {
            return Err(IrregularFieldError::ValueCountMismatch {
                expected: mesh.vertex_count(),
                actual: values.len(),
            });
        }
        if let Some((index, value)) = values
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(IrregularFieldError::NonFiniteValue {
                vertex: IrregularVertexId(index),
                value,
            });
        }
        Ok(Self { mesh, values })
    }
    /// Evaluate a finite function at every mesh vertex and construct its field.
    pub fn from_fn(
        mesh: IrregularTernaryMesh,
        mut function: impl FnMut([f64; 3]) -> f64,
    ) -> Result<Self, IrregularFieldError> {
        let values = mesh
            .vertex_ids()
            .map(|vertex| function(mesh.compositions[vertex.0]))
            .collect();
        Self::new(mesh, values)
    }
    /// Return this field's immutable mesh.
    pub fn mesh(&self) -> &IrregularTernaryMesh {
        &self.mesh
    }
    /// Return scalar values in dense vertex-ID order.
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    /// Return one scalar vertex value.
    pub fn value(&self, vertex: IrregularVertexId) -> Result<f64, IrregularFieldError> {
        self.values
            .get(vertex.0)
            .copied()
            .ok_or(IrregularFieldError::InvalidVertex { vertex })
    }
}

/// An irregular field sample with value, global gradient, and cached location.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularFieldSample {
    /// Interpolated scalar value (linear or cubic-alpha, as selected).
    pub value: f64,
    /// Global `(df/da, df/db)` with `c = 1-a-b`.
    pub gradient_ab: [f64; 2],
    /// The deterministic containing triangle and barycentric location.
    pub location: LocatedIrregularTriangle,
}

impl IrregularFieldSample {
    /// Return this sample's gradient in shared invariant ternary coordinates.
    pub const fn gradient(&self) -> crate::TernaryGradient {
        crate::TernaryGradient::from_reduced_ab(self.gradient_ab)
    }

    /// Return the gradient in canonical logical `(x, y)` coordinates.
    pub fn gradient_logical_xy(&self) -> [f64; 2] {
        self.gradient().logical_xy()
    }

    /// Return the invariant gradient magnitude per unit logical distance.
    pub fn gradient_norm(&self) -> f64 {
        self.gradient().norm()
    }
}

/// Prepared piecewise-linear evaluator for an irregular ternary scalar field.
///
/// Linear gradients are constant within a triangle. The field is C0 across
/// edges but need not be C1; edge and vertex gradients come from the selected
/// owner triangle and are never averaged.
pub struct PreparedIrregularTernaryField<'a> {
    field: &'a IrregularTernaryScalarField,
}
impl<'a> PreparedIrregularTernaryField<'a> {
    /// Prepare a reusable linear evaluator without rebuilding topology.
    pub const fn new(field: &'a IrregularTernaryScalarField) -> Self {
        Self { field }
    }
    /// Evaluate only the scalar value at a composition in the mesh convex hull.
    pub fn value(&self, composition: [f64; 3]) -> Result<f64, IrregularFieldEvaluationError> {
        Ok(self.evaluate(composition)?.value)
    }
    /// Evaluate the value, global gradient, and containing triangle.
    pub fn evaluate(
        &self,
        composition: [f64; 3],
    ) -> Result<IrregularFieldSample, IrregularFieldEvaluationError> {
        let location = self.field.mesh.locate(composition)?;
        self.evaluate_at_location(&location)
    }
    /// Evaluate only the scalar value at a cached location.
    pub fn value_at_location(
        &self,
        location: &LocatedIrregularTriangle,
    ) -> Result<f64, IrregularFieldEvaluationError> {
        Ok(self.evaluate_at_location(location)?.value)
    }
    /// Evaluate value and gradient without repeating semantic validation or point location.
    pub fn evaluate_at_location(
        &self,
        location: &LocatedIrregularTriangle,
    ) -> Result<IrregularFieldSample, IrregularFieldEvaluationError> {
        let triangle = self.validated_triangle(location)?;
        let values = triangle.vertices.map(|vertex| self.field.values[vertex.0]);
        let value = values
            .into_iter()
            .zip(location.barycentric)
            .map(|(value, weight)| value * weight)
            .sum::<f64>();
        let gradient_ab = global_gradient_ab(
            self.field
                .mesh
                .triangle_compositions(triangle.id)
                .map_err(|_| IrregularFieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
            [values[0] - values[2], values[1] - values[2]],
        )
        .ok_or(IrregularFieldEvaluationError::NonFiniteEvaluation)?;
        if !value.is_finite() || !gradient_ab.into_iter().all(f64::is_finite) {
            return Err(IrregularFieldEvaluationError::NonFiniteEvaluation);
        }
        Ok(IrregularFieldSample {
            value,
            gradient_ab,
            location: *location,
        })
    }
    /// Lazily evaluate a batch while reusing this prepared evaluator.
    pub fn values<'b, I>(
        &'b self,
        compositions: I,
    ) -> impl Iterator<Item = Result<f64, IrregularFieldEvaluationError>> + 'b
    where
        I: IntoIterator<Item = [f64; 3]>,
        I::IntoIter: 'b,
    {
        compositions
            .into_iter()
            .map(|composition| self.value(composition))
    }
    /// Evaluate into caller-owned storage without allocation.
    ///
    /// Values before a failing item are written; remaining output is unchanged.
    pub fn values_into(
        &self,
        compositions: &[[f64; 3]],
        output: &mut [f64],
    ) -> Result<(), IrregularFieldEvaluationError> {
        if compositions.len() != output.len() {
            return Err(IrregularFieldEvaluationError::OutputSizeMismatch {
                expected: compositions.len(),
                actual: output.len(),
            });
        }
        for (composition, value) in compositions.iter().copied().zip(output) {
            *value = self.value(composition)?;
        }
        Ok(())
    }
    fn validated_triangle(
        &self,
        location: &LocatedIrregularTriangle,
    ) -> Result<IrregularMeshTriangle, IrregularFieldEvaluationError> {
        if location.mesh_identity != self.field.mesh.identity {
            return Err(IrregularFieldEvaluationError::IncompatibleLocation);
        }
        let triangle = self
            .field
            .mesh
            .triangles
            .get(location.triangle.id.0)
            .copied()
            .ok_or(IrregularFieldEvaluationError::InvalidLocation {
                triangle: location.triangle.id,
            })?;
        if triangle != location.triangle
            || !valid_barycentric(location.barycentric, POINT_LOCATION_TOLERANCE)
        {
            return Err(IrregularFieldEvaluationError::InvalidLocation {
                triangle: location.triangle.id,
            });
        }
        Ok(triangle)
    }
}

enum CompositionError {
    NonFinite { component: usize, value: f64 },
    InvalidSum { sum: f64 },
    OutsideSimplex { composition: [f64; 3] },
}

fn normalize_sample(sample: usize, composition: [f64; 3]) -> Result<[f64; 3], IrregularMeshError> {
    normalize_composition(composition).map_err(|error| match error {
        CompositionError::NonFinite { component, value } => {
            IrregularMeshError::NonFiniteComposition {
                sample,
                component,
                value,
            }
        }
        CompositionError::InvalidSum { sum } => {
            IrregularMeshError::InvalidCompositionSum { sample, sum }
        }
        CompositionError::OutsideSimplex { composition } => IrregularMeshError::OutsideSimplex {
            sample,
            composition,
        },
    })
}
fn normalize_query(composition: [f64; 3]) -> Result<[f64; 3], IrregularPointLocationError> {
    normalize_composition(composition).map_err(|error| match error {
        CompositionError::NonFinite { component, value } => {
            IrregularPointLocationError::NonFiniteComposition { component, value }
        }
        CompositionError::InvalidSum { sum } => {
            IrregularPointLocationError::InvalidCompositionSum { sum }
        }
        CompositionError::OutsideSimplex { composition } => {
            IrregularPointLocationError::OutsideSimplex { composition }
        }
    })
}
fn normalize_composition(mut composition: [f64; 3]) -> Result<[f64; 3], CompositionError> {
    for (component, value) in composition.iter().copied().enumerate() {
        if !value.is_finite() {
            return Err(CompositionError::NonFinite { component, value });
        }
    }
    let sum = composition.into_iter().sum::<f64>();
    if !sum.is_finite() || (sum - 1.0).abs() > POINT_LOCATION_TOLERANCE {
        return Err(CompositionError::InvalidSum { sum });
    }
    for value in &mut composition {
        *value /= sum;
        if value.abs() <= POINT_LOCATION_TOLERANCE {
            *value = 0.0;
        } else if (1.0 - *value).abs() <= POINT_LOCATION_TOLERANCE {
            *value = 1.0;
        }
    }
    let snapped_sum = composition.into_iter().sum::<f64>();
    for value in &mut composition {
        *value /= snapped_sum;
    }
    if composition
        .into_iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(CompositionError::OutsideSimplex { composition });
    }
    Ok(composition)
}

fn composition_key(composition: [f64; 3]) -> [u64; 3] {
    composition.map(f64::to_bits)
}
#[cfg(test)]
fn composition_from_logical([x, y]: [f64; 2]) -> [f64; 3] {
    let c = y / crate::simplex::EQUILATERAL_HEIGHT;
    let b = x - 0.5 * c;
    [1.0 - b - c, b, c]
}
fn reject_duplicate_samples(compositions: &[[f64; 3]]) -> Result<(), IrregularMeshError> {
    let squared_tolerance = IRREGULAR_VERTEX_TOLERANCE * IRREGULAR_VERTEX_TOLERANCE;
    for (first, first_composition) in compositions.iter().copied().enumerate() {
        let first_point = logical_from_composition(first_composition);
        for (second, second_composition) in compositions.iter().copied().enumerate().skip(first + 1)
        {
            let second_point = logical_from_composition(second_composition);
            let dx = first_point[0] - second_point[0];
            let dy = first_point[1] - second_point[1];
            if dx.mul_add(dx, dy * dy) <= squared_tolerance {
                return Err(IrregularMeshError::DuplicateComposition { first, second });
            }
        }
    }
    Ok(())
}
fn canonical_counterclockwise_triangle(
    mut vertices: [IrregularVertexId; 3],
    compositions: &[[f64; 3]],
) -> Result<[IrregularVertexId; 3], IrregularMeshError> {
    let points = vertices.map(|vertex| logical_from_composition(compositions[vertex.0]));
    let area = (points[1][0] - points[0][0]) * (points[2][1] - points[0][1])
        - (points[1][1] - points[0][1]) * (points[2][0] - points[0][0]);
    if !area.is_finite() || area == 0.0 {
        return Err(IrregularMeshError::InvalidTopology {
            message: "backend returned a degenerate triangle".to_owned(),
        });
    }
    if area < 0.0 {
        vertices.swap(1, 2);
    }
    let first = vertices
        .iter()
        .enumerate()
        .min_by_key(|(_, vertex)| vertex.0)
        .map(|(index, _)| index)
        .expect("triangle has three vertices");
    vertices.rotate_left(first);
    Ok(vertices)
}
fn edge_key(first: IrregularVertexId, second: IrregularVertexId) -> [usize; 2] {
    [first.0.min(second.0), first.0.max(second.0)]
}
fn triangle_edge_keys(vertices: [IrregularVertexId; 3]) -> [[usize; 2]; 3] {
    [
        edge_key(vertices[0], vertices[1]),
        edge_key(vertices[1], vertices[2]),
        edge_key(vertices[2], vertices[0]),
    ]
}
fn mesh_to_location_error(error: IrregularMeshError) -> IrregularPointLocationError {
    IrregularPointLocationError::BackendFailure {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: f64 = 2.0e-10;

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() <= TOLERANCE, "{left:?} != {right:?}");
    }

    fn fixture_samples() -> [[f64; 3]; 7] {
        [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.57, 0.28, 0.15],
            [0.18, 0.61, 0.21],
            [0.23, 0.16, 0.61],
            [0.31, 0.42, 0.27],
        ]
    }

    fn fixture_mesh() -> IrregularTernaryMesh {
        IrregularTernaryMesh::new(fixture_samples()).expect("fixture samples triangulate")
    }

    fn affine([a, b, c]: [f64; 3]) -> f64 {
        2.25 * a - 3.5 * b + 0.75 * c + 1.125
    }

    fn reconstruct(location: LocatedIrregularTriangle, mesh: &IrregularTernaryMesh) -> [f64; 3] {
        let compositions = mesh.triangle_compositions(location.triangle.id).unwrap();
        [
            compositions
                .iter()
                .zip(location.barycentric)
                .map(|(point, weight)| point[0] * weight)
                .sum(),
            compositions
                .iter()
                .zip(location.barycentric)
                .map(|(point, weight)| point[1] * weight)
                .sum(),
            compositions
                .iter()
                .zip(location.barycentric)
                .map(|(point, weight)| point[2] * weight)
                .sum(),
        ]
    }

    fn exhaustive_location(
        mesh: &IrregularTernaryMesh,
        composition: [f64; 3],
    ) -> Option<(IrregularTriangleId, [f64; 3])> {
        mesh.triangles()
            .filter_map(|triangle| {
                let barycentric = canonical_barycentric(
                    barycentric_ab(mesh.triangle_compositions(triangle.id).ok()?, composition)?,
                    POINT_LOCATION_TOLERANCE,
                )?;
                Some((triangle.id, barycentric))
            })
            .min_by_key(|(triangle, _)| *triangle)
    }

    #[test]
    fn topology_is_dense_canonical_and_ready_for_edge_alpha() {
        let mesh = fixture_mesh();
        assert_eq!(mesh.vertex_count(), fixture_samples().len());
        assert!(mesh.triangle_count() > 0);
        assert_eq!(
            mesh.vertex_ids().collect::<Vec<_>>(),
            (0..mesh.vertex_count())
                .map(IrregularVertexId)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            mesh.edges().map(|edge| edge.id).collect::<Vec<_>>(),
            (0..mesh.edge_count())
                .map(IrregularEdgeId)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            mesh.triangles()
                .map(|triangle| triangle.id)
                .collect::<Vec<_>>(),
            (0..mesh.triangle_count())
                .map(IrregularTriangleId)
                .collect::<Vec<_>>()
        );

        for edge in mesh.edges() {
            assert!(edge.vertices[0] < edge.vertices[1]);
            assert!(matches!(edge.incident_triangle_count(), 1 | 2));
            assert_eq!(edge.is_boundary(), edge.incident_triangle_count() == 1);
            for triangle in edge.triangles.into_iter().flatten() {
                assert!(mesh.triangle(triangle).unwrap().edges.contains(&edge.id));
            }
        }
        for triangle in mesh.triangles() {
            assert_eq!(
                triangle.vertices[0],
                *triangle.vertices.iter().min().unwrap()
            );
            for edge in triangle.edges {
                assert!(
                    mesh.edge(edge)
                        .unwrap()
                        .triangles
                        .contains(&Some(triangle.id))
                );
            }
        }

        let repeat = fixture_mesh();
        assert_eq!(
            mesh.edges().collect::<Vec<_>>(),
            repeat.edges().collect::<Vec<_>>()
        );
        assert_eq!(
            mesh.triangles().collect::<Vec<_>>(),
            repeat.triangles().collect::<Vec<_>>()
        );
    }

    #[test]
    fn cocircular_input_follows_the_backend_deterministic_topology_policy() {
        let samples = [
            [
                0.476_794_919_243_112_2,
                0.176_794_919_243_112_23,
                0.346_410_161_513_775_5,
            ],
            [
                0.176_794_919_243_112_2,
                0.476_794_919_243_112_3,
                0.346_410_161_513_775_5,
            ],
            [
                0.413_397_459_621_556_1,
                0.413_397_459_621_556_1,
                0.173_205_080_756_887_76,
            ],
            [
                0.240_192_378_864_668_4,
                0.240_192_378_864_668_4,
                0.519_615_242_270_663_2,
            ],
        ];
        let first = IrregularTernaryMesh::new(samples).unwrap();
        let second = IrregularTernaryMesh::new(samples).unwrap();
        assert_eq!(first.triangle_count(), 2);
        assert_eq!(
            first.edges().collect::<Vec<_>>(),
            second.edges().collect::<Vec<_>>()
        );
        assert_eq!(
            first.triangles().collect::<Vec<_>>(),
            second.triangles().collect::<Vec<_>>()
        );
    }
    #[test]
    fn locates_vertices_centres_edges_and_edge_sides_deterministically() {
        let mesh = fixture_mesh();
        for vertex in mesh.vertex_ids() {
            let composition = mesh.composition(vertex).unwrap();
            let location = mesh.locate(composition).unwrap();
            assert_eq!(
                location.boundary,
                IrregularPointBoundaryLocation::Vertex { vertex }
            );
            assert_eq!(
                location.triangle.id,
                mesh.incident_triangles(vertex).unwrap()[0]
            );
            assert!(location.barycentric.contains(&1.0));
            for (actual, expected) in reconstruct(location, &mesh).into_iter().zip(composition) {
                close(actual, expected);
            }
        }

        for triangle in mesh.triangles() {
            let vertices = mesh.triangle_compositions(triangle.id).unwrap();
            let centre = [
                (vertices[0][0] + vertices[1][0] + vertices[2][0]) / 3.0,
                (vertices[0][1] + vertices[1][1] + vertices[2][1]) / 3.0,
                (vertices[0][2] + vertices[1][2] + vertices[2][2]) / 3.0,
            ];
            let location = mesh.locate(centre).unwrap();
            assert_eq!(location.triangle.id, triangle.id);
            assert_eq!(location.boundary, IrregularPointBoundaryLocation::Interior);
            for (actual, expected) in reconstruct(location, &mesh).into_iter().zip(centre) {
                close(actual, expected);
            }
        }

        for edge in mesh.edges() {
            let first = mesh.composition(edge.vertices[0]).unwrap();
            let second = mesh.composition(edge.vertices[1]).unwrap();
            let midpoint = [
                (first[0] + second[0]) / 2.0,
                (first[1] + second[1]) / 2.0,
                (first[2] + second[2]) / 2.0,
            ];
            let location = mesh.locate(midpoint).unwrap();
            assert_eq!(location.triangle.id, edge.triangles[0].unwrap());
            let expected_boundary = if edge.is_boundary() {
                IrregularPointBoundaryLocation::BoundaryEdge { edge: edge.id }
            } else {
                IrregularPointBoundaryLocation::InteriorEdge { edge: edge.id }
            };
            assert_eq!(location.boundary, expected_boundary);

            for incident in edge.triangles.into_iter().flatten() {
                let vertices = mesh.triangle_compositions(incident).unwrap();
                let centre = [
                    (vertices[0][0] + vertices[1][0] + vertices[2][0]) / 3.0,
                    (vertices[0][1] + vertices[1][1] + vertices[2][1]) / 3.0,
                    (vertices[0][2] + vertices[1][2] + vertices[2][2]) / 3.0,
                ];
                let nearby = [
                    midpoint[0] * 0.999_999 + centre[0] * 0.000_001,
                    midpoint[1] * 0.999_999 + centre[1] * 0.000_001,
                    midpoint[2] * 0.999_999 + centre[2] * 0.000_001,
                ];
                assert_eq!(mesh.locate(nearby).unwrap().triangle.id, incident);
            }
        }
    }

    #[test]
    fn backend_location_matches_exhaustive_barycentric_reference() {
        let mesh = fixture_mesh();
        let mut state = 0x5eed_cafe_f00d_beefu64;
        let next = |state: &mut u64| {
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            ((*state >> 11) as f64) / ((1u64 << 53) as f64)
        };
        for triangle in mesh.triangles() {
            let vertices = mesh.triangle_compositions(triangle.id).unwrap();
            for _ in 0..128 {
                let weights = [
                    0.05 + next(&mut state),
                    0.05 + next(&mut state),
                    0.05 + next(&mut state),
                ];
                let sum = weights.into_iter().sum::<f64>();
                let weights = weights.map(|weight| weight / sum);
                let composition = [
                    vertices
                        .iter()
                        .zip(weights)
                        .map(|(point, weight)| point[0] * weight)
                        .sum(),
                    vertices
                        .iter()
                        .zip(weights)
                        .map(|(point, weight)| point[1] * weight)
                        .sum(),
                    vertices
                        .iter()
                        .zip(weights)
                        .map(|(point, weight)| point[2] * weight)
                        .sum(),
                ];
                let location = mesh.locate(composition).unwrap();
                let (reference_triangle, reference_barycentric) =
                    exhaustive_location(&mesh, composition).expect("inside source triangle");
                assert_eq!(location.triangle.id, reference_triangle);
                for (actual, expected) in
                    location.barycentric.into_iter().zip(reference_barycentric)
                {
                    close(actual, expected);
                }
                for (actual, expected) in reconstruct(location, &mesh).into_iter().zip(composition)
                {
                    close(actual, expected);
                }
            }
        }
    }

    #[test]
    fn validates_samples_and_rejects_outside_hull_queries() {
        assert!(matches!(
            IrregularTernaryMesh::new(Vec::<[f64; 3]>::new()),
            Err(IrregularMeshError::TooFewSamples { actual: 0 })
        ));
        assert!(matches!(
            IrregularTernaryMesh::new([[f64::NAN, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]),
            Err(IrregularMeshError::NonFiniteComposition { .. })
        ));
        assert!(matches!(
            IrregularTernaryMesh::new([
                [f64::INFINITY, 0.0, 1.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0]
            ]),
            Err(IrregularMeshError::NonFiniteComposition { .. })
        ));
        assert!(matches!(
            IrregularTernaryMesh::new([[0.5, 0.5, 0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]),
            Err(IrregularMeshError::InvalidCompositionSum { .. })
        ));
        assert!(matches!(
            IrregularTernaryMesh::new([[-0.1, 0.4, 0.7], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]),
            Err(IrregularMeshError::OutsideSimplex { .. })
        ));
        assert!(matches!(
            IrregularTernaryMesh::new([
                [0.3, 0.3, 0.4],
                [0.3 + 2.0e-11, 0.3, 0.4 - 2.0e-11],
                [1.0, 0.0, 0.0]
            ]),
            Err(IrregularMeshError::DuplicateComposition { .. })
        ));
        assert!(matches!(
            IrregularTernaryMesh::new([[1.0, 0.0, 0.0], [0.5, 0.5, 0.0], [0.0, 1.0, 0.0]]),
            Err(IrregularMeshError::TriangulationFailed { .. })
        ));

        let hull =
            IrregularTernaryMesh::new([[0.8, 0.1, 0.1], [0.1, 0.8, 0.1], [0.1, 0.1, 0.8]]).unwrap();
        assert!(matches!(
            hull.locate([1.0, 0.0, 0.0]),
            Err(IrregularPointLocationError::OutsideConvexHull { .. })
        ));
        assert!(matches!(
            hull.locate([f64::NAN, 0.0, 1.0]),
            Err(IrregularPointLocationError::NonFiniteComposition { .. })
        ));
        assert!(matches!(
            hull.locate([1.1, 0.0, 0.0]),
            Err(IrregularPointLocationError::InvalidCompositionSum { .. })
        ));
        assert!(matches!(
            hull.locate([-0.1, 0.4, 0.7]),
            Err(IrregularPointLocationError::OutsideSimplex { .. })
        ));
        assert!(matches!(
            hull.locate_with_hint([0.4, 0.3, 0.3], Some(IrregularTriangleId(99))),
            Err(IrregularPointLocationError::InvalidHint { .. })
        ));

        let snapped = fixture_mesh()
            .locate([
                -0.5 * POINT_LOCATION_TOLERANCE,
                0.5 * POINT_LOCATION_TOLERANCE,
                1.0,
            ])
            .unwrap();
        assert_eq!(
            snapped.boundary,
            IrregularPointBoundaryLocation::Vertex {
                vertex: IrregularVertexId(2)
            }
        );
    }

    #[test]
    fn linear_field_reproduces_affine_values_and_gradients() {
        let field = IrregularTernaryScalarField::from_fn(fixture_mesh(), affine).unwrap();
        let evaluator = PreparedIrregularTernaryField::new(&field);
        for vertex in field.mesh().vertex_ids() {
            let composition = field.mesh().composition(vertex).unwrap();
            close(
                evaluator.value(composition).unwrap(),
                field.value(vertex).unwrap(),
            );
        }
        for triangle in field.mesh().triangles() {
            let vertices = field.mesh().triangle_compositions(triangle.id).unwrap();
            let query = [
                vertices[0][0] * 0.2 + vertices[1][0] * 0.3 + vertices[2][0] * 0.5,
                vertices[0][1] * 0.2 + vertices[1][1] * 0.3 + vertices[2][1] * 0.5,
                vertices[0][2] * 0.2 + vertices[1][2] * 0.3 + vertices[2][2] * 0.5,
            ];
            let sample = evaluator.evaluate(query).unwrap();
            close(sample.value, affine(query));
            close(sample.gradient_ab[0], 1.5);
            close(sample.gradient_ab[1], -4.25);
            assert_eq!(
                evaluator.evaluate_at_location(&sample.location).unwrap(),
                sample
            );
        }
    }

    #[test]
    fn batch_evaluation_preserves_order_and_reports_failures() {
        let field = IrregularTernaryScalarField::from_fn(fixture_mesh(), affine).unwrap();
        let evaluator = PreparedIrregularTernaryField::new(&field);
        let inputs = [[0.4, 0.3, 0.3], [0.2, 0.5, 0.3], [0.25, 0.25, 0.5]];
        let expected = inputs.map(|composition| evaluator.value(composition).unwrap());
        let collected = evaluator
            .values(inputs)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(collected.len(), inputs.len());
        for (actual, expected) in collected.into_iter().zip(expected) {
            close(actual, expected);
        }
        assert_eq!(
            evaluator.values([]).collect::<Result<Vec<_>, _>>().unwrap(),
            Vec::<f64>::new()
        );

        let mut output = [f64::NAN; 3];
        evaluator.values_into(&inputs, &mut output).unwrap();
        for (actual, expected) in output.into_iter().zip(expected) {
            close(actual, expected);
        }
        assert!(matches!(
            evaluator.values_into(&inputs, &mut output[..2]),
            Err(IrregularFieldEvaluationError::OutputSizeMismatch {
                expected: 3,
                actual: 2
            })
        ));
        let mut partial = [f64::NAN; 3];
        assert!(matches!(
            evaluator.values_into(
                &[[0.4, 0.3, 0.3], [1.1, 0.0, 0.0], [0.2, 0.5, 0.3]],
                &mut partial
            ),
            Err(IrregularFieldEvaluationError::PointLocation(
                IrregularPointLocationError::InvalidCompositionSum { .. }
            ))
        ));
        assert!(partial[0].is_finite());
        assert!(partial[2].is_nan());
    }

    #[test]
    fn locations_cannot_cross_mesh_identity_boundaries() {
        let first = IrregularTernaryScalarField::from_fn(fixture_mesh(), affine).unwrap();
        let second = IrregularTernaryScalarField::from_fn(fixture_mesh(), affine).unwrap();
        let location = first.mesh().locate([0.4, 0.3, 0.3]).unwrap();
        assert!(matches!(
            PreparedIrregularTernaryField::new(&second).value_at_location(&location),
            Err(IrregularFieldEvaluationError::IncompatibleLocation)
        ));
    }

    #[test]
    fn equilateral_embedding_preserves_component_permutation_symmetry() {
        let original = fixture_mesh();
        let permuted =
            IrregularTernaryMesh::new(fixture_samples().map(|[a, b, c]| [b, c, a])).unwrap();
        assert_eq!(original.vertex_count(), permuted.vertex_count());
        assert_eq!(original.edge_count(), permuted.edge_count());
        assert_eq!(original.triangle_count(), permuted.triangle_count());

        let field =
            IrregularTernaryScalarField::from_fn(original, |[a, b, c]| a * a + b * b + c * c)
                .unwrap();
        let permuted_field =
            IrregularTernaryScalarField::from_fn(permuted, |[a, b, c]| a * a + b * b + c * c)
                .unwrap();
        let query = [0.41, 0.34, 0.25];
        let permuted_query = [query[1], query[2], query[0]];
        close(
            PreparedIrregularTernaryField::new(&field)
                .value(query)
                .unwrap(),
            PreparedIrregularTernaryField::new(&permuted_field)
                .value(permuted_query)
                .unwrap(),
        );
    }

    #[test]
    fn logical_coordinate_round_trip_is_semantic() {
        for composition in fixture_samples() {
            for (actual, expected) in
                composition_from_logical(logical_from_composition(composition))
                    .into_iter()
                    .zip(composition)
            {
                close(actual, expected);
            }
        }
    }
}
