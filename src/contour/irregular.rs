//! Backend-independent isolines for irregular Delaunay ternary fields.
//!
//! This module owns numerical contour extraction only. Its inputs and outputs
//! remain semantic A/B/C compositions; neither Delaunay backend handles nor
//! rendering coordinates are exposed.

use core::fmt;

#[cfg(feature = "irregular-cubic-alpha")]
use crate::IrregularMeshTriangle;
use crate::{
    ContourLevel, ContourPath, ContourRegularization, InterpolatedIrregularTernaryField,
    IrregularCubicAlphaBuildError, IrregularCubicAlphaDiagnostics, IrregularCubicAlphaOptions,
    IrregularEdgeId, IrregularFieldEvaluationError, IrregularFieldInterpolation,
    IrregularTernaryScalarField, IrregularTriangleId, TernaryCoordinate,
};

use super::{
    ContourError,
    paths::{ContourSegment, join_segments},
};

/// Bounded, scale-aware adaptive controls for irregular cubic contours.
///
/// The physical threshold is measured in the crate's canonical equilateral
/// logical plane. It prevents a large Delaunay triangle and a tiny neighbour
/// from receiving the same ineffective fixed barycentric resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularAdaptiveContourOptions {
    /// Maximum recursive barycentric microtriangle depth, in `1..=12`.
    pub max_depth: u8,
    /// Maximum sampled departure from local linearity before refinement.
    pub flatness_tolerance: f64,
    /// Largest accepted microtriangle edge in canonical logical coordinates.
    pub maximum_microtriangle_diameter: f64,
}

impl Default for IrregularAdaptiveContourOptions {
    fn default() -> Self {
        Self {
            max_depth: 7,
            flatness_tolerance: 1.0e-5,
            maximum_microtriangle_diameter: 0.025,
        }
    }
}

impl IrregularAdaptiveContourOptions {
    fn validate(self) -> Result<(), IrregularContourError> {
        if self.max_depth == 0
            || self.max_depth > 12
            || !self.flatness_tolerance.is_finite()
            || self.flatness_tolerance <= 0.0
            || !self.maximum_microtriangle_diameter.is_finite()
            || self.maximum_microtriangle_diameter <= 0.0
        {
            return Err(IrregularContourError::InvalidAdaptiveOptions {
                max_depth: self.max_depth,
                flatness_tolerance: self.flatness_tolerance,
                maximum_microtriangle_diameter: self.maximum_microtriangle_diameter,
            });
        }
        Ok(())
    }
}

/// Geometry and post-extraction choices reusable with a prepared field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularContourGeometryOptions {
    /// Finite positive scalar equality tolerance.
    pub value_tolerance: f64,
    /// Finite positive semantic-coordinate cleanup tolerance.
    pub geometry_tolerance: f64,
    /// Adaptive cubic topology controls. Ignored for linear fields.
    pub adaptive: IrregularAdaptiveContourOptions,
    /// Optional equal-arclength redistribution and global level projection.
    pub regularization: Option<ContourRegularization>,
}

impl Default for IrregularContourGeometryOptions {
    fn default() -> Self {
        Self {
            value_tolerance: 1.0e-10,
            geometry_tolerance: 1.0e-8,
            adaptive: IrregularAdaptiveContourOptions::default(),
            regularization: None,
        }
    }
}

impl IrregularContourGeometryOptions {
    fn validate(self) -> Result<(), IrregularContourError> {
        if !self.value_tolerance.is_finite()
            || self.value_tolerance <= 0.0
            || !self.geometry_tolerance.is_finite()
            || self.geometry_tolerance <= 0.0
        {
            return Err(IrregularContourError::InvalidTolerance {
                value_tolerance: self.value_tolerance,
                geometry_tolerance: self.geometry_tolerance,
            });
        }
        self.adaptive.validate()?;
        if let Some(regularization) = self.regularization {
            validate_regularization(regularization)?;
        }
        Ok(())
    }
}

/// Scalar interpolation model selected by the convenience contour workflow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum IrregularContourInterpolation {
    /// Exact piecewise-affine evaluation within each Delaunay triangle.
    #[default]
    Linear,
    /// Prepared self-consistent edge-alpha cubic field.
    CubicAlpha(IrregularCubicAlphaOptions),
}

/// Options for [`IrregularContourSet::compute`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularContourOptions {
    /// Field model prepared once before all requested levels are extracted.
    pub interpolation: IrregularContourInterpolation,
    /// Extraction and optional regularization settings.
    pub geometry: IrregularContourGeometryOptions,
}

impl IrregularContourOptions {
    /// Construct an always-available piecewise-linear irregular contour model.
    pub const fn linear() -> Self {
        Self {
            interpolation: IrregularContourInterpolation::Linear,
            geometry: IrregularContourGeometryOptions {
                value_tolerance: 1.0e-10,
                geometry_tolerance: 1.0e-8,
                adaptive: IrregularAdaptiveContourOptions {
                    max_depth: 7,
                    flatness_tolerance: 1.0e-5,
                    maximum_microtriangle_diameter: 0.025,
                },
                regularization: None,
            },
        }
    }

    /// Construct cubic-alpha irregular contour options.
    pub const fn cubic_alpha(options: IrregularCubicAlphaOptions) -> Self {
        Self {
            interpolation: IrregularContourInterpolation::CubicAlpha(options),
            geometry: IrregularContourGeometryOptions {
                value_tolerance: 1.0e-10,
                geometry_tolerance: 1.0e-8,
                adaptive: IrregularAdaptiveContourOptions {
                    max_depth: 7,
                    flatness_tolerance: 1.0e-5,
                    maximum_microtriangle_diameter: 0.025,
                },
                regularization: Some(ContourRegularization {
                    spacing: 0.0125,
                    redistribution_passes: 2,
                    projection_tolerance: 1.0e-9,
                    max_projection_iterations: 16,
                    max_normal_step: 0.05,
                }),
            },
        }
    }
}

impl Default for IrregularContourOptions {
    fn default() -> Self {
        Self::linear()
    }
}

/// Compact source-solver status retained with irregular contour diagnostics.
///
/// The full alpha diagnostics remain available from the prepared evaluator.
/// This summary intentionally does not duplicate its potentially long residual
/// history in every contour result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularCubicContourSourceDiagnostics {
    /// Number of canonical mesh edges in the source field.
    pub edge_count: usize,
    /// Number of completed synchronous alpha sweeps.
    pub sweep_count: usize,
    /// Final normalized alpha residual, when one completed.
    pub residual: Option<f64>,
    /// Final solver convergence classification.
    pub convergence: crate::IrregularAlphaConvergence,
}

impl From<&IrregularCubicAlphaDiagnostics> for IrregularCubicContourSourceDiagnostics {
    fn from(source: &IrregularCubicAlphaDiagnostics) -> Self {
        Self {
            edge_count: source.edge_count,
            sweep_count: source.sweep_count,
            residual: source.residual,
            convergence: source.convergence,
        }
    }
}

/// Aggregated diagnostics for one requested irregular iso-level.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IrregularContourLevelDiagnostics {
    /// Requested scalar level.
    pub level: f64,
    /// Original Delaunay triangles inspected.
    pub source_triangles: usize,
    /// Adaptive microtriangles evaluated for this level.
    pub evaluated_microtriangles: usize,
    /// Microtriangles subdivided for this level.
    pub refined_microtriangles: usize,
    /// Accepted cells that hit the configured maximum depth.
    pub maximum_depth_hits: usize,
    /// Canonical shared-edge roots calculated for cubic extraction.
    pub canonical_shared_edge_roots: usize,
    /// Degenerate duplicate segments discarded during assembly.
    pub duplicate_segments_removed: usize,
    /// Open final contour components.
    pub open_paths: usize,
    /// Closed final contour components.
    pub closed_paths: usize,
    /// Redistribution/project passes completed.
    pub regularization_passes: usize,
    /// Interior points submitted to level projection.
    pub projected_points: usize,
    /// Total damped normal/Newton iterations.
    pub projection_iterations: usize,
    /// Rejected damped projection candidates.
    pub projection_backtracking_steps: usize,
    /// Accepted projection steps entering a new selected triangle.
    pub triangle_boundary_crossings: usize,
    /// Candidate projection points rejected outside the convex hull.
    pub convex_hull_candidate_rejections: usize,
    /// Zero-gradient projection encounters.
    pub zero_gradient_encounters: usize,
    /// Largest final absolute residual when regularization is enabled.
    pub maximum_final_residual: f64,
    /// Logical chord-spacing coefficient of variation before regularization.
    pub spacing_cv_before: Option<f64>,
    /// Logical chord-spacing coefficient of variation after regularization.
    pub spacing_cv_after: Option<f64>,
}

/// Diagnostics for all levels in one irregular contour computation.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularContourDiagnostics {
    /// Prepared field family used for extraction and projection.
    pub interpolation: IrregularContourInterpolation,
    /// Number of source Delaunay triangles.
    pub source_triangle_count: usize,
    /// Number of requested scalar levels after validation.
    pub requested_level_count: usize,
    /// Compact alpha source status for cubic contours.
    pub cubic_source: Option<IrregularCubicContourSourceDiagnostics>,
    /// Diagnostics in sorted scalar-level order.
    pub levels: Vec<IrregularContourLevelDiagnostics>,
}

/// Backend-independent contour paths over an irregular Delaunay field.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularContourSet {
    /// Levels sorted in increasing scalar order.
    pub levels: Vec<ContourLevel>,
    diagnostics: IrregularContourDiagnostics,
}

impl IrregularContourSet {
    /// Prepare the requested field model once, then extract all levels.
    ///
    /// Cubic-alpha preparation is performed exactly once for the whole set;
    /// callers that already own a prepared evaluator should prefer
    /// [`Self::compute_prepared`].
    pub fn compute(
        field: &IrregularTernaryScalarField,
        levels: &[f64],
        options: IrregularContourOptions,
    ) -> Result<Self, IrregularContourError> {
        options.geometry.validate()?;
        let interpolation = match options.interpolation {
            IrregularContourInterpolation::Linear => IrregularFieldInterpolation::Linear,
            IrregularContourInterpolation::CubicAlpha(cubic) => {
                IrregularFieldInterpolation::CubicAlpha(cubic)
            }
        };
        let prepared = InterpolatedIrregularTernaryField::new(field, interpolation)
            .map_err(map_preparation_error)?;
        Self::compute_prepared(&prepared, levels, options.geometry)
    }

    /// Extract contours from an already prepared linear or converged cubic field.
    ///
    /// No alpha intervals, virtual stencils, or Jacobi sweeps are rebuilt.
    pub fn compute_prepared(
        field: &InterpolatedIrregularTernaryField<'_>,
        levels: &[f64],
        geometry: IrregularContourGeometryOptions,
    ) -> Result<Self, IrregularContourError> {
        geometry.validate()?;
        let levels = validated_levels(levels, geometry.value_tolerance)?;
        let interpolation = match field.interpolation() {
            IrregularFieldInterpolation::Linear => IrregularContourInterpolation::Linear,
            IrregularFieldInterpolation::CubicAlpha(options) => {
                IrregularContourInterpolation::CubicAlpha(options)
            }
        };
        let cubic_source = field
            .cubic_diagnostics()
            .map(IrregularCubicContourSourceDiagnostics::from);
        let mut diagnostics = IrregularContourDiagnostics {
            interpolation,
            source_triangle_count: field.mesh().triangle_count(),
            requested_level_count: levels.len(),
            cubic_source,
            levels: Vec::with_capacity(levels.len()),
        };
        let mut result = Vec::with_capacity(levels.len());
        for level in levels {
            let mut level_diagnostics = IrregularContourLevelDiagnostics {
                level,
                source_triangles: field.mesh().triangle_count(),
                ..IrregularContourLevelDiagnostics::default()
            };
            let mut paths = match field.interpolation() {
                IrregularFieldInterpolation::Linear => linear_paths(
                    field,
                    level,
                    geometry.value_tolerance,
                    geometry.geometry_tolerance,
                    &mut level_diagnostics,
                )?,
                IrregularFieldInterpolation::CubicAlpha(_) => {
                    cubic_paths(field, level, geometry, &mut level_diagnostics)?
                }
            };
            if let Some(regularization) = geometry.regularization {
                regularize_paths(
                    field,
                    &mut paths,
                    level,
                    regularization,
                    &mut level_diagnostics,
                )?;
            }
            level_diagnostics.open_paths = paths.iter().filter(|path| !path.closed).count();
            level_diagnostics.closed_paths = paths.iter().filter(|path| path.closed).count();
            diagnostics.levels.push(level_diagnostics);
            result.push(ContourLevel {
                value: level,
                paths,
            });
        }
        Ok(Self {
            levels: result,
            diagnostics,
        })
    }

    /// Return numerical extraction and regularization diagnostics.
    pub const fn diagnostics(&self) -> &IrregularContourDiagnostics {
        &self.diagnostics
    }

    /// Return contour levels in their validated increasing scalar order.
    ///
    /// This accessor is equivalent to borrowing [`Self::levels`] and is
    /// provided for callers that do not need to depend on the result layout.
    pub fn levels(&self) -> &[ContourLevel] {
        &self.levels
    }
}

/// Failures specific to irregular numerical contour construction.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum IrregularContourError {
    /// A requested contour level was not finite.
    NonFiniteLevel { index: usize, value: f64 },
    /// Two requested levels are equal within the configured value tolerance.
    DuplicateLevel {
        first: usize,
        second: usize,
        value: f64,
    },
    /// Value or geometry tolerance was not finite and positive.
    InvalidTolerance {
        value_tolerance: f64,
        geometry_tolerance: f64,
    },
    /// Adaptive settings were not bounded and numerically meaningful.
    InvalidAdaptiveOptions {
        max_depth: u8,
        flatness_tolerance: f64,
        maximum_microtriangle_diameter: f64,
    },
    /// Regularization settings were invalid.
    InvalidRegularization { message: &'static str },
    /// Cubic contours were requested without the `irregular-cubic-alpha` feature.
    CubicFeatureUnavailable,
    /// The prepared cubic field could not be constructed or converge.
    CubicConstruction(Box<IrregularCubicAlphaBuildError>),
    /// A field evaluation failed during extraction or global projection.
    FieldEvaluation(IrregularFieldEvaluationError),
    /// All three source-triangle values coincide with a requested linear level.
    FlatTriangle {
        triangle: IrregularTriangleId,
        level: f64,
    },
    /// A complete source edge coincides with a requested cubic level.
    FlatEdge { edge: IrregularEdgeId, level: f64 },
    /// A shared cubic edge remained unresolved at the configured micro depth.
    UnresolvedSharedEdgeRoots { edge: IrregularEdgeId, roots: usize },
    /// Endpoint graph degree exceeded two.
    BranchingTopology { degree: usize },
    /// Segment assembly produced an invalid path component.
    InvalidPathTopology { message: &'static str },
    /// A contour component has no measurable logical length.
    ZeroLengthPath,
    /// Projection encountered a selected one-sided gradient too small to use.
    ProjectionZeroGradient { residual: f64 },
    /// Projection failed to decrease its level residual.
    ProjectionNonConvergence { residual: f64, iterations: usize },
}

impl fmt::Display for IrregularContourError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteLevel { index, value } => {
                write!(formatter, "contour level {index} is not finite: {value:?}")
            }
            Self::DuplicateLevel {
                first,
                second,
                value,
            } => write!(
                formatter,
                "contour levels {first} and {second} duplicate {value:?}"
            ),
            Self::InvalidTolerance {
                value_tolerance,
                geometry_tolerance,
            } => write!(
                formatter,
                "irregular contour tolerances must be finite and positive: value={value_tolerance:?}, geometry={geometry_tolerance:?}"
            ),
            Self::InvalidAdaptiveOptions {
                max_depth,
                flatness_tolerance,
                maximum_microtriangle_diameter,
            } => write!(
                formatter,
                "invalid irregular adaptive options: max_depth={max_depth}, flatness={flatness_tolerance:?}, maximum_microtriangle_diameter={maximum_microtriangle_diameter:?}"
            ),
            Self::InvalidRegularization { message } => {
                write!(
                    formatter,
                    "invalid irregular contour regularization: {message}"
                )
            }
            Self::CubicFeatureUnavailable => write!(
                formatter,
                "irregular cubic contours require the `irregular-cubic-alpha` feature"
            ),
            Self::CubicConstruction(error) => {
                write!(
                    formatter,
                    "irregular cubic field construction failed: {error}"
                )
            }
            Self::FieldEvaluation(error) => {
                write!(formatter, "irregular field evaluation failed: {error}")
            }
            Self::FlatTriangle { triangle, level } => write!(
                formatter,
                "irregular triangle {triangle:?} is entirely coincident with contour level {level:?}"
            ),
            Self::FlatEdge { edge, level } => write!(
                formatter,
                "irregular cubic edge {edge:?} is entirely coincident with contour level {level:?}"
            ),
            Self::UnresolvedSharedEdgeRoots { edge, roots } => write!(
                formatter,
                "irregular cubic edge {edge:?} has {roots} unresolved shared roots at maximum micro depth"
            ),
            Self::BranchingTopology { degree } => write!(
                formatter,
                "irregular contour endpoint graph has non-manifold degree {degree}"
            ),
            Self::InvalidPathTopology { message } => {
                write!(
                    formatter,
                    "irregular contour path topology is invalid: {message}"
                )
            }
            Self::ZeroLengthPath => write!(formatter, "irregular contour has zero logical length"),
            Self::ProjectionZeroGradient { residual } => write!(
                formatter,
                "irregular implicit projection encountered a zero gradient at residual {residual:?}"
            ),
            Self::ProjectionNonConvergence {
                residual,
                iterations,
            } => write!(
                formatter,
                "irregular implicit projection did not converge after {iterations} iterations; residual={residual:?}"
            ),
        }
    }
}

impl std::error::Error for IrregularContourError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CubicConstruction(error) => Some(error),
            Self::FieldEvaluation(error) => Some(error),
            _ => None,
        }
    }
}

fn map_preparation_error(error: IrregularFieldEvaluationError) -> IrregularContourError {
    match error {
        IrregularFieldEvaluationError::CubicFeatureUnavailable => {
            IrregularContourError::CubicFeatureUnavailable
        }
        IrregularFieldEvaluationError::CubicConstruction(error) => {
            IrregularContourError::CubicConstruction(error)
        }
        error => IrregularContourError::FieldEvaluation(error),
    }
}

fn validated_levels(levels: &[f64], tolerance: f64) -> Result<Vec<f64>, IrregularContourError> {
    let mut indexed = levels.iter().copied().enumerate().collect::<Vec<_>>();
    if let Some((index, value)) = indexed
        .iter()
        .copied()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(IrregularContourError::NonFiniteLevel { index, value });
    }
    indexed.sort_by(|left, right| left.1.total_cmp(&right.1));
    for pair in indexed.windows(2) {
        if (pair[0].1 - pair[1].1).abs() <= tolerance {
            return Err(IrregularContourError::DuplicateLevel {
                first: pair[0].0,
                second: pair[1].0,
                value: pair[0].1,
            });
        }
    }
    Ok(indexed.into_iter().map(|(_, level)| level).collect())
}

fn validate_regularization(options: ContourRegularization) -> Result<(), IrregularContourError> {
    if !options.spacing.is_finite() || options.spacing <= 0.0 {
        return Err(IrregularContourError::InvalidRegularization {
            message: "spacing must be finite and positive",
        });
    }
    if !options.projection_tolerance.is_finite()
        || options.projection_tolerance <= 0.0
        || options.max_projection_iterations == 0
        || !options.max_normal_step.is_finite()
        || options.max_normal_step <= 0.0
    {
        return Err(IrregularContourError::InvalidRegularization {
            message: "projection tolerance, iteration limit, and maximum step must be positive",
        });
    }
    Ok(())
}

fn linear_paths(
    field: &InterpolatedIrregularTernaryField<'_>,
    level: f64,
    value_tolerance: f64,
    geometry_tolerance: f64,
    diagnostics: &mut IrregularContourLevelDiagnostics,
) -> Result<Vec<ContourPath>, IrregularContourError> {
    let _ = diagnostics;
    let mesh = field.mesh();
    let source = field.field();
    let mut segments = Vec::new();
    for triangle in mesh.triangles() {
        let values = triangle.vertices.map(|vertex| source.values()[vertex.0]);
        let points = mesh
            .triangle_compositions(triangle.id)
            .map_err(|error| {
                IrregularContourError::FieldEvaluation(
                    IrregularFieldEvaluationError::PointLocation(
                        crate::IrregularPointLocationError::BackendFailure {
                            message: error.to_string(),
                        },
                    ),
                )
            })?
            .map(Into::into);
        let on = values.map(|value| (value - level).abs() <= value_tolerance);
        if on.into_iter().all(|value| value) {
            return Err(IrregularContourError::FlatTriangle {
                triangle: triangle.id,
                level,
            });
        }
        if let Some((left, right, edge)) = [(0, 1, 0), (1, 2, 1), (2, 0, 2)]
            .into_iter()
            .find(|(left, right, _)| on[*left] && on[*right])
        {
            let edge = mesh
                .edge(triangle.edges[edge])
                .expect("triangle stores mesh edge");
            if edge.triangles[0] == Some(triangle.id) {
                segments.push(ContourSegment {
                    start: points[left],
                    end: points[right],
                });
            }
            continue;
        }
        let mut crossings = Vec::new();
        for (left, right) in [(0, 1), (1, 2), (2, 0)] {
            match (on[left], on[right]) {
                (true, false) => push_unique(&mut crossings, points[left], geometry_tolerance),
                (false, true) => push_unique(&mut crossings, points[right], geometry_tolerance),
                (false, false) => {
                    let first = values[left] - level;
                    let second = values[right] - level;
                    if first.is_sign_positive() != second.is_sign_positive() {
                        let denominator = values[right] - values[left];
                        if denominator.abs() > value_tolerance {
                            let mut t = (level - values[left]) / denominator;
                            if t.abs() <= value_tolerance {
                                t = 0.0;
                            } else if (1.0 - t).abs() <= value_tolerance {
                                t = 1.0;
                            }
                            push_unique(
                                &mut crossings,
                                lerp(points[left], points[right], t),
                                geometry_tolerance,
                            );
                        }
                    }
                }
                (true, true) => unreachable!(),
            }
        }
        if crossings.len() == 2 && !points_close(crossings[0], crossings[1], geometry_tolerance) {
            segments.push(ContourSegment {
                start: crossings[0],
                end: crossings[1],
            });
        }
    }
    join_segments(segments, geometry_tolerance).map_err(map_path_error)
}

fn map_path_error(error: ContourError) -> IrregularContourError {
    match error {
        ContourError::BranchingTopology { degree } => {
            IrregularContourError::BranchingTopology { degree }
        }
        ContourError::ZeroLengthPath => IrregularContourError::ZeroLengthPath,
        ContourError::InvalidClosedLoop => IrregularContourError::InvalidPathTopology {
            message: "closed component has fewer than three distinct points",
        },
        _ => IrregularContourError::InvalidPathTopology {
            message: "shared path assembler rejected the segment graph",
        },
    }
}

fn cubic_paths(
    _field: &InterpolatedIrregularTernaryField<'_>,
    _level: f64,
    _geometry: IrregularContourGeometryOptions,
    _diagnostics: &mut IrregularContourLevelDiagnostics,
) -> Result<Vec<ContourPath>, IrregularContourError> {
    #[cfg(not(feature = "irregular-cubic-alpha"))]
    {
        return Err(IrregularContourError::CubicFeatureUnavailable);
    }
    #[cfg(feature = "irregular-cubic-alpha")]
    {
        cubic_paths_impl(_field, _level, _geometry, _diagnostics)
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
#[derive(Clone, Copy, Debug)]
struct EdgeRoot {
    canonical_t: f64,
    point: TernaryCoordinate,
}

#[cfg(feature = "irregular-cubic-alpha")]
#[derive(Clone, Copy)]
struct CubicSample {
    local: [f64; 3],
    point: TernaryCoordinate,
    value: f64,
}

#[cfg(feature = "irregular-cubic-alpha")]
fn cubic_paths_impl(
    field: &InterpolatedIrregularTernaryField<'_>,
    level: f64,
    geometry: IrregularContourGeometryOptions,
    diagnostics: &mut IrregularContourLevelDiagnostics,
) -> Result<Vec<ContourPath>, IrregularContourError> {
    let roots = canonical_edge_roots(field, level, geometry.value_tolerance)?;
    diagnostics.canonical_shared_edge_roots = roots.iter().map(Vec::len).sum();
    let mut segments = Vec::new();
    for triangle in field.mesh().triangles() {
        let vertices = field
            .mesh()
            .triangle_compositions(triangle.id)
            .map_err(mesh_failure)?
            .map(Into::into);
        let cell = [
            cubic_sample(field, triangle, [1.0, 0.0, 0.0], vertices)?,
            cubic_sample(field, triangle, [0.0, 1.0, 0.0], vertices)?,
            cubic_sample(field, triangle, [0.0, 0.0, 1.0], vertices)?,
        ];
        refine_cubic_cell(
            field,
            triangle,
            vertices,
            cell,
            level,
            geometry,
            0,
            &roots,
            diagnostics,
            &mut segments,
        )?;
    }
    join_segments(segments, geometry.geometry_tolerance).map_err(map_path_error)
}

#[cfg(feature = "irregular-cubic-alpha")]
#[allow(clippy::too_many_arguments)]
fn refine_cubic_cell(
    field: &InterpolatedIrregularTernaryField<'_>,
    triangle: IrregularMeshTriangle,
    vertices: [TernaryCoordinate; 3],
    cell: [CubicSample; 3],
    level: f64,
    geometry: IrregularContourGeometryOptions,
    depth: u8,
    roots: &[Vec<EdgeRoot>],
    diagnostics: &mut IrregularContourLevelDiagnostics,
    segments: &mut Vec<ContourSegment>,
) -> Result<(), IrregularContourError> {
    diagnostics.evaluated_microtriangles += 1;
    let mids = [
        cubic_midpoint(field, triangle, cell[0], cell[1], vertices)?,
        cubic_midpoint(field, triangle, cell[1], cell[2], vertices)?,
        cubic_midpoint(field, triangle, cell[2], cell[0], vertices)?,
    ];
    let centre_local = [
        (cell[0].local[0] + cell[1].local[0] + cell[2].local[0]) / 3.0,
        (cell[0].local[1] + cell[1].local[1] + cell[2].local[1]) / 3.0,
        (cell[0].local[2] + cell[1].local[2] + cell[2].local[2]) / 3.0,
    ];
    let centre = cubic_sample(field, triangle, centre_local, vertices)?;
    let all = [
        cell[0].value,
        cell[1].value,
        cell[2].value,
        mids[0].value,
        mids[1].value,
        mids[2].value,
        centre.value,
    ];
    let minimum = all.into_iter().fold(f64::INFINITY, f64::min);
    let maximum = all.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let bracket =
        minimum <= level + geometry.value_tolerance && maximum >= level - geometry.value_tolerance;
    let flatness = [
        (mids[0].value - (cell[0].value + cell[1].value) / 2.0).abs(),
        (mids[1].value - (cell[1].value + cell[2].value) / 2.0).abs(),
        (mids[2].value - (cell[2].value + cell[0].value) / 2.0).abs(),
        (centre.value - (cell[0].value + cell[1].value + cell[2].value) / 3.0).abs(),
    ]
    .into_iter()
    .fold(0.0_f64, f64::max);
    let diameter = longest_logical_edge(cell);
    let active = bracket || flatness > geometry.adaptive.flatness_tolerance;
    let needs_minimum_probe = depth < 2;
    let needs_physical_resolution =
        bracket && diameter > geometry.adaptive.maximum_microtriangle_diameter;
    let needs_curvature_resolution = flatness > geometry.adaptive.flatness_tolerance;
    if depth < geometry.adaptive.max_depth
        && (needs_minimum_probe
            || (active && (needs_physical_resolution || needs_curvature_resolution)))
    {
        diagnostics.refined_microtriangles += 1;
        for child in [
            [cell[0], mids[0], mids[2]],
            [mids[0], cell[1], mids[1]],
            [mids[2], mids[1], cell[2]],
            [mids[0], mids[1], mids[2]],
        ] {
            refine_cubic_cell(
                field,
                triangle,
                vertices,
                child,
                level,
                geometry,
                depth + 1,
                roots,
                diagnostics,
                segments,
            )?;
        }
        return Ok(());
    }
    if !bracket {
        return Ok(());
    }
    if depth == geometry.adaptive.max_depth
        && (needs_physical_resolution || needs_curvature_resolution)
    {
        diagnostics.maximum_depth_hits += 1;
    }
    march_cubic_cell(
        triangle,
        cell,
        level,
        geometry.value_tolerance,
        roots,
        segments,
    )
}

#[cfg(feature = "irregular-cubic-alpha")]
fn cubic_midpoint(
    field: &InterpolatedIrregularTernaryField<'_>,
    triangle: IrregularMeshTriangle,
    left: CubicSample,
    right: CubicSample,
    vertices: [TernaryCoordinate; 3],
) -> Result<CubicSample, IrregularContourError> {
    cubic_sample(
        field,
        triangle,
        [
            (left.local[0] + right.local[0]) / 2.0,
            (left.local[1] + right.local[1]) / 2.0,
            (left.local[2] + right.local[2]) / 2.0,
        ],
        vertices,
    )
}

#[cfg(feature = "irregular-cubic-alpha")]
fn cubic_sample(
    field: &InterpolatedIrregularTernaryField<'_>,
    triangle: IrregularMeshTriangle,
    local: [f64; 3],
    vertices: [TernaryCoordinate; 3],
) -> Result<CubicSample, IrregularContourError> {
    let point = combine(vertices, local);
    let (value, _) = field
        .evaluate_in_triangle(triangle, local)
        .map_err(IrregularContourError::FieldEvaluation)?;
    if !value.is_finite() {
        return Err(IrregularContourError::FieldEvaluation(
            IrregularFieldEvaluationError::InvalidLocation {
                triangle: triangle.id,
            },
        ));
    }
    Ok(CubicSample {
        local,
        point,
        value,
    })
}

#[cfg(feature = "irregular-cubic-alpha")]
fn combine(vertices: [TernaryCoordinate; 3], weights: [f64; 3]) -> TernaryCoordinate {
    let vertices = vertices.map(TernaryCoordinate::as_array);
    TernaryCoordinate::new(
        vertices[0][0] * weights[0] + vertices[1][0] * weights[1] + vertices[2][0] * weights[2],
        vertices[0][1] * weights[0] + vertices[1][1] * weights[1] + vertices[2][1] * weights[2],
        vertices[0][2] * weights[0] + vertices[1][2] * weights[1] + vertices[2][2] * weights[2],
    )
}

#[cfg(feature = "irregular-cubic-alpha")]
fn longest_logical_edge(cell: [CubicSample; 3]) -> f64 {
    [(0, 1), (1, 2), (2, 0)]
        .into_iter()
        .map(|(left, right)| {
            crate::simplex::logical_distance(
                cell[left].point.as_array(),
                cell[right].point.as_array(),
            )
        })
        .fold(0.0_f64, f64::max)
}

#[cfg(feature = "irregular-cubic-alpha")]
fn march_cubic_cell(
    triangle: IrregularMeshTriangle,
    cell: [CubicSample; 3],
    level: f64,
    tolerance: f64,
    roots: &[Vec<EdgeRoot>],
    segments: &mut Vec<ContourSegment>,
) -> Result<(), IrregularContourError> {
    let mut crossings = Vec::new();
    for (left, right) in [(0, 1), (1, 2), (2, 0)] {
        let first = cell[left];
        let second = cell[right];
        let first_delta = first.value - level;
        let second_delta = second.value - level;
        let first_on = first_delta.abs() <= tolerance;
        let second_on = second_delta.abs() <= tolerance;
        if first_on {
            push_unique(
                &mut crossings,
                canonical_boundary_vertex(triangle, first, roots, tolerance),
                tolerance,
            );
        }
        if second_on {
            push_unique(
                &mut crossings,
                canonical_boundary_vertex(triangle, second, roots, tolerance),
                tolerance,
            );
        }
        if !first_on
            && !second_on
            && first_delta.is_sign_positive() != second_delta.is_sign_positive()
        {
            let fraction = (level - first.value) / (second.value - first.value);
            let point = interpolate_sample(first, second, fraction);
            let point =
                canonical_boundary_crossing(triangle, first, second, point, roots, tolerance)?;
            push_unique(&mut crossings, point, tolerance);
        }
    }
    if crossings.len() == 2 && !points_close(crossings[0], crossings[1], tolerance) {
        segments.push(ContourSegment {
            start: crossings[0],
            end: crossings[1],
        });
    }
    Ok(())
}

#[cfg(feature = "irregular-cubic-alpha")]
fn interpolate_sample(left: CubicSample, right: CubicSample, t: f64) -> CubicSample {
    CubicSample {
        local: [
            left.local[0] + (right.local[0] - left.local[0]) * t,
            left.local[1] + (right.local[1] - left.local[1]) * t,
            left.local[2] + (right.local[2] - left.local[2]) * t,
        ],
        point: lerp(left.point, right.point, t),
        value: left.value + (right.value - left.value) * t,
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
fn canonical_boundary_vertex(
    triangle: IrregularMeshTriangle,
    sample: CubicSample,
    roots: &[Vec<EdgeRoot>],
    tolerance: f64,
) -> TernaryCoordinate {
    boundary_edge_parameter(triangle, sample.local)
        .and_then(|(edge, parameter)| {
            roots[edge.0]
                .iter()
                .find(|root| (root.canonical_t - parameter).abs() <= tolerance * 8.0)
                .map(|root| root.point)
        })
        .unwrap_or(sample.point)
}

#[cfg(feature = "irregular-cubic-alpha")]
fn canonical_boundary_crossing(
    triangle: IrregularMeshTriangle,
    left: CubicSample,
    right: CubicSample,
    crossing: CubicSample,
    roots: &[Vec<EdgeRoot>],
    tolerance: f64,
) -> Result<TernaryCoordinate, IrregularContourError> {
    let Some((edge, crossing_parameter)) = boundary_edge_parameter(triangle, crossing.local) else {
        return Ok(crossing.point);
    };
    let Some((left_edge, left_parameter)) = boundary_edge_parameter(triangle, left.local) else {
        return Ok(crossing.point);
    };
    let Some((right_edge, right_parameter)) = boundary_edge_parameter(triangle, right.local) else {
        return Ok(crossing.point);
    };
    if edge != left_edge || edge != right_edge {
        return Ok(crossing.point);
    }
    let lower = left_parameter.min(right_parameter) - tolerance * 8.0;
    let upper = left_parameter.max(right_parameter) + tolerance * 8.0;
    let candidates = roots[edge.0]
        .iter()
        .filter(|root| root.canonical_t >= lower && root.canonical_t <= upper)
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [root] => Ok(root.point),
        [] => {
            let nearest = roots[edge.0].iter().min_by(|left, right| {
                (left.canonical_t - crossing_parameter)
                    .abs()
                    .total_cmp(&(right.canonical_t - crossing_parameter).abs())
            });
            nearest
                .map(|root| root.point)
                .ok_or(IrregularContourError::UnresolvedSharedEdgeRoots { edge, roots: 0 })
        }
        roots => Err(IrregularContourError::UnresolvedSharedEdgeRoots {
            edge,
            roots: roots.len(),
        }),
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
fn boundary_edge_parameter(
    triangle: IrregularMeshTriangle,
    local: [f64; 3],
) -> Option<(IrregularEdgeId, f64)> {
    const BOUNDARY_EPSILON: f64 = 128.0 * f64::EPSILON;
    let (edge_index, start, end, parameter) = if local[2].abs() <= BOUNDARY_EPSILON {
        (0, 0, 1, local[1])
    } else if local[0].abs() <= BOUNDARY_EPSILON {
        (1, 1, 2, local[2])
    } else if local[1].abs() <= BOUNDARY_EPSILON {
        (2, 2, 0, local[0])
    } else {
        return None;
    };
    let edge = triangle.edges[edge_index];
    let canonical = if triangle.vertices[start].0 < triangle.vertices[end].0 {
        parameter
    } else {
        1.0 - parameter
    };
    Some((edge, canonical.clamp(0.0, 1.0)))
}

#[cfg(feature = "irregular-cubic-alpha")]
fn canonical_edge_roots(
    field: &InterpolatedIrregularTernaryField<'_>,
    level: f64,
    tolerance: f64,
) -> Result<Vec<Vec<EdgeRoot>>, IrregularContourError> {
    field
        .mesh()
        .edges()
        .map(|edge| {
            let interval = field
                .cubic_alpha_interval(edge.id)
                .ok_or(IrregularContourError::CubicFeatureUnavailable)?;
            let start = field.field().value(edge.vertices[0]).map_err(|_| {
                IrregularContourError::FieldEvaluation(
                    IrregularFieldEvaluationError::InvalidLocation {
                        triangle: edge.triangles[0].unwrap_or(IrregularTriangleId(0)),
                    },
                )
            })?;
            let end = field
                .field()
                .value(edge.vertices[1])
                .map_err(|_| IrregularContourError::CubicFeatureUnavailable)?;
            let start_point: TernaryCoordinate = field
                .mesh()
                .composition(edge.vertices[0])
                .map_err(mesh_failure)?
                .into();
            let end_point: TernaryCoordinate = field
                .mesh()
                .composition(edge.vertices[1])
                .map_err(mesh_failure)?
                .into();
            let roots = interval_roots(interval, start, end, level, tolerance, edge.id)?;
            Ok(roots
                .into_iter()
                .map(|canonical_t| EdgeRoot {
                    canonical_t,
                    point: lerp(start_point, end_point, canonical_t),
                })
                .collect())
        })
        .collect()
}

#[cfg(feature = "irregular-cubic-alpha")]
fn interval_roots(
    interval: crate::interpolation::AlphaInterval,
    start: f64,
    end: f64,
    level: f64,
    tolerance: f64,
    edge: IrregularEdgeId,
) -> Result<Vec<f64>, IrregularContourError> {
    let coefficients = [
        start - level,
        -start + end + interval.alpha0,
        -interval.alpha0 + interval.alpha1,
        -interval.alpha1,
    ];
    let scale = coefficients
        .into_iter()
        .fold(1.0_f64, |scale, value| scale.max(value.abs()));
    if coefficients
        .into_iter()
        .all(|value| value.abs() <= tolerance)
    {
        return Err(IrregularContourError::FlatEdge { edge, level });
    }
    let polynomial = |t: f64| {
        ((coefficients[3] * t + coefficients[2]) * t + coefficients[1]) * t + coefficients[0]
    };
    let mut breaks = vec![0.0, 1.0];
    let derivative_scale = scale.max(1.0);
    let a = 3.0 * coefficients[3];
    let b = 2.0 * coefficients[2];
    let c = coefficients[1];
    let derivative_tolerance = 128.0 * f64::EPSILON * derivative_scale;
    if a.abs() <= derivative_tolerance {
        if b.abs() > derivative_tolerance {
            breaks.push((-c / b).clamp(0.0, 1.0));
        }
    } else {
        let discriminant = b.mul_add(b, -4.0 * a * c);
        if discriminant >= -derivative_tolerance {
            let root = discriminant.max(0.0).sqrt();
            breaks.push(((-b - root) / (2.0 * a)).clamp(0.0, 1.0));
            breaks.push(((-b + root) / (2.0 * a)).clamp(0.0, 1.0));
        }
    }
    breaks.sort_by(f64::total_cmp);
    breaks.dedup_by(|left, right| (*left - *right).abs() <= 128.0 * f64::EPSILON);
    let mut roots = Vec::new();
    for &point in &breaks {
        if polynomial(point).abs() <= tolerance {
            push_root(&mut roots, point);
        }
    }
    for pair in breaks.windows(2) {
        let left = pair[0];
        let right = pair[1];
        let left_value = polynomial(left);
        let right_value = polynomial(right);
        if left_value.is_sign_positive() != right_value.is_sign_positive()
            && left_value.abs() > tolerance
            && right_value.abs() > tolerance
        {
            let mut lower = left;
            let mut upper = right;
            for _ in 0..80 {
                let middle = (lower + upper) / 2.0;
                let value = polynomial(middle);
                if value.abs() <= tolerance * 0.25 {
                    lower = middle;
                    upper = middle;
                    break;
                }
                if polynomial(lower).is_sign_positive() == value.is_sign_positive() {
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
            push_root(&mut roots, (lower + upper) / 2.0);
        }
    }
    roots.sort_by(f64::total_cmp);
    Ok(roots)
}

#[cfg(feature = "irregular-cubic-alpha")]
fn push_root(roots: &mut Vec<f64>, root: f64) {
    if roots
        .iter()
        .all(|existing| (*existing - root).abs() > 1.0e-10)
    {
        roots.push(root.clamp(0.0, 1.0));
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
fn mesh_failure(error: crate::IrregularMeshError) -> IrregularContourError {
    IrregularContourError::FieldEvaluation(IrregularFieldEvaluationError::PointLocation(
        crate::IrregularPointLocationError::BackendFailure {
            message: error.to_string(),
        },
    ))
}

fn regularize_paths(
    field: &InterpolatedIrregularTernaryField<'_>,
    paths: &mut [ContourPath],
    level: f64,
    options: ContourRegularization,
    diagnostics: &mut IrregularContourLevelDiagnostics,
) -> Result<(), IrregularContourError> {
    diagnostics.spacing_cv_before = spacing_coefficient_of_variation(paths);
    for path in &mut *paths {
        let original_start = path.points.first().copied();
        let original_end = path.points.last().copied();
        for _ in 0..options.redistribution_passes.max(1) {
            path.points = redistribute(&path.points, path.closed, options.spacing)?;
            diagnostics.regularization_passes += 1;
            let length = path.points.len();
            for (index, point) in path.points.iter_mut().enumerate() {
                if !path.closed && (index == 0 || index + 1 == length) {
                    continue;
                }
                diagnostics.projected_points += 1;
                *point = project_point(field, *point, level, options, diagnostics)?;
            }
        }
        if !path.closed {
            if let Some(start) = original_start {
                path.points[0] = start;
            }
            if let Some(end) = original_end {
                *path.points.last_mut().expect("open path has two endpoints") = end;
            }
        }
    }
    diagnostics.spacing_cv_after = spacing_coefficient_of_variation(paths);
    diagnostics.maximum_final_residual = paths
        .iter()
        .flat_map(|path| path.points.iter().copied())
        .map(|point| {
            field
                .value(point.as_array())
                .map(|value| (value - level).abs())
                .map_err(IrregularContourError::FieldEvaluation)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .fold(0.0_f64, f64::max);
    Ok(())
}

fn redistribute(
    points: &[TernaryCoordinate],
    closed: bool,
    spacing: f64,
) -> Result<Vec<TernaryCoordinate>, IrregularContourError> {
    if points.len() < 2 {
        return Err(IrregularContourError::ZeroLengthPath);
    }
    let edge_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let mut cumulative = vec![0.0];
    for index in 0..edge_count {
        let next = (index + 1) % points.len();
        cumulative.push(
            cumulative.last().copied().unwrap_or_default()
                + crate::simplex::logical_distance(
                    points[index].as_array(),
                    points[next].as_array(),
                ),
        );
    }
    let total = cumulative.last().copied().unwrap_or_default();
    if total <= f64::EPSILON {
        return Err(IrregularContourError::ZeroLengthPath);
    }
    let intervals = (total / spacing).ceil().max(if closed { 3.0 } else { 1.0 }) as usize;
    let sample_count = if closed { intervals } else { intervals + 1 };
    let mut result = Vec::with_capacity(sample_count);
    for sample in 0..sample_count {
        let target = total * sample as f64 / intervals as f64;
        let edge = cumulative
            .windows(2)
            .position(|window| target <= window[1])
            .unwrap_or(edge_count - 1);
        let span = cumulative[edge + 1] - cumulative[edge];
        let fraction = if span <= f64::EPSILON {
            0.0
        } else {
            (target - cumulative[edge]) / span
        };
        result.push(lerp(
            points[edge],
            points[(edge + 1) % points.len()],
            fraction,
        ));
    }
    Ok(result)
}

fn project_point(
    field: &InterpolatedIrregularTernaryField<'_>,
    mut point: TernaryCoordinate,
    level: f64,
    options: ContourRegularization,
    diagnostics: &mut IrregularContourLevelDiagnostics,
) -> Result<TernaryCoordinate, IrregularContourError> {
    for iteration in 0..options.max_projection_iterations {
        let located = field
            .evaluate(point.as_array())
            .map_err(IrregularContourError::FieldEvaluation)?;
        let residual = located.value - level;
        diagnostics.projection_iterations += 1;
        if residual.abs() <= options.projection_tolerance {
            return Ok(point);
        }
        let norm2 = located.gradient_ab[0].powi(2) + located.gradient_ab[1].powi(2);
        if !norm2.is_finite() || norm2 <= 1.0e-24 {
            diagnostics.zero_gradient_encounters += 1;
            return Err(IrregularContourError::ProjectionZeroGradient { residual });
        }
        let factor = -residual / norm2;
        let mut delta = [
            factor * located.gradient_ab[0],
            factor * located.gradient_ab[1],
        ];
        let magnitude = delta[0].hypot(delta[1]);
        if magnitude > options.max_normal_step {
            let scale = options.max_normal_step / magnitude;
            delta[0] *= scale;
            delta[1] *= scale;
        }
        let source = point.as_array();
        let mut accepted = None;
        let mut damping = 1.0;
        for _ in 0..24 {
            let Some(candidate) = normalized_candidate(
                source[0] + damping * delta[0],
                source[1] + damping * delta[1],
                1.0 - source[0] - source[1] - damping * (delta[0] + delta[1]),
                options.projection_tolerance.max(1.0e-12),
            ) else {
                diagnostics.projection_backtracking_steps += 1;
                damping *= 0.5;
                continue;
            };
            match field.evaluate(candidate.as_array()) {
                Ok(next) if (next.value - level).abs() < residual.abs() => {
                    if next.location.triangle.id != located.location.triangle.id {
                        diagnostics.triangle_boundary_crossings += 1;
                    }
                    accepted = Some(candidate);
                    break;
                }
                Err(IrregularFieldEvaluationError::PointLocation(
                    crate::IrregularPointLocationError::OutsideConvexHull { .. },
                )) => {
                    diagnostics.convex_hull_candidate_rejections += 1;
                }
                Err(IrregularFieldEvaluationError::PointLocation(
                    crate::IrregularPointLocationError::OutsideSimplex { .. },
                )) => {}
                Err(error) => return Err(IrregularContourError::FieldEvaluation(error)),
                Ok(_) => {}
            }
            diagnostics.projection_backtracking_steps += 1;
            damping *= 0.5;
        }
        let Some(candidate) = accepted else {
            return Err(IrregularContourError::ProjectionNonConvergence {
                residual,
                iterations: iteration + 1,
            });
        };
        point = candidate;
    }
    let residual = field
        .value(point.as_array())
        .map_err(IrregularContourError::FieldEvaluation)?
        - level;
    Err(IrregularContourError::ProjectionNonConvergence {
        residual,
        iterations: options.max_projection_iterations,
    })
}

fn normalized_candidate(a: f64, b: f64, c: f64, tolerance: f64) -> Option<TernaryCoordinate> {
    if ![a, b, c].into_iter().all(f64::is_finite)
        || [a, b, c].into_iter().any(|value| value < -tolerance)
    {
        return None;
    }
    let components = [a.max(0.0), b.max(0.0), c.max(0.0)];
    let sum = components.into_iter().sum::<f64>();
    if !sum.is_finite() || sum <= tolerance {
        return None;
    }
    Some(TernaryCoordinate::new(
        components[0] / sum,
        components[1] / sum,
        components[2] / sum,
    ))
}

fn spacing_coefficient_of_variation(paths: &[ContourPath]) -> Option<f64> {
    let lengths = paths
        .iter()
        .flat_map(|path| {
            let count = if path.closed {
                path.points.len()
            } else {
                path.points.len().saturating_sub(1)
            };
            (0..count).map(move |index| {
                crate::simplex::logical_distance(
                    path.points[index].as_array(),
                    path.points[(index + 1) % path.points.len()].as_array(),
                )
            })
        })
        .collect::<Vec<_>>();
    if lengths.len() < 2 {
        return None;
    }
    let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
    if mean <= f64::EPSILON {
        return None;
    }
    let variance = lengths
        .iter()
        .map(|length| (length - mean).powi(2))
        .sum::<f64>()
        / lengths.len() as f64;
    Some(variance.sqrt() / mean)
}

fn lerp(left: TernaryCoordinate, right: TernaryCoordinate, t: f64) -> TernaryCoordinate {
    let left = left.as_array();
    let right = right.as_array();
    TernaryCoordinate::new(
        left[0] + (right[0] - left[0]) * t,
        left[1] + (right[1] - left[1]) * t,
        left[2] + (right[2] - left[2]) * t,
    )
}

fn push_unique(points: &mut Vec<TernaryCoordinate>, point: TernaryCoordinate, tolerance: f64) {
    if points
        .iter()
        .all(|existing| !points_close(*existing, point, tolerance))
    {
        points.push(point);
    }
}

fn points_close(left: TernaryCoordinate, right: TernaryCoordinate, tolerance: f64) -> bool {
    left.as_array()
        .into_iter()
        .zip(right.as_array())
        .all(|(left, right)| (left - right).abs() <= tolerance)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IrregularTernaryMesh, IrregularTernaryScalarField};

    fn samples() -> [[f64; 3]; 10] {
        [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.76, 0.15, 0.09],
            [0.57, 0.28, 0.15],
            [0.18, 0.61, 0.21],
            [0.23, 0.16, 0.61],
            [0.31, 0.42, 0.27],
            [0.47, 0.12, 0.41],
            [0.14, 0.37, 0.49],
        ]
    }

    fn affine_field() -> IrregularTernaryScalarField {
        IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples()).unwrap(),
            |[a, b, c]| 2.0 * a - 3.0 * b + 5.0 * c + 0.25,
        )
        .unwrap()
    }

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() <= 2.0e-9, "{left:?} != {right:?}");
    }

    #[test]
    fn linear_irregular_contours_reproduce_an_affine_level_set() {
        let field = affine_field();
        let contours =
            IrregularContourSet::compute(&field, &[-0.25, 0.5], IrregularContourOptions::linear())
                .unwrap();
        assert_eq!(contours.levels.len(), 2);
        for level in &contours.levels {
            assert_eq!(level.paths.len(), 1);
            assert!(!level.paths[0].closed);
            for point in &level.paths[0].points {
                let [a, b, c] = point.as_array();
                close(2.0 * a - 3.0 * b + 5.0 * c + 0.25, level.value);
            }
        }
        let repeated =
            IrregularContourSet::compute(&field, &[0.5, -0.25], IrregularContourOptions::linear())
                .unwrap();
        assert_eq!(contours, repeated);
    }

    #[test]
    fn linear_prepared_and_convenience_workflows_agree() {
        let field = affine_field();
        let prepared =
            InterpolatedIrregularTernaryField::new(&field, IrregularFieldInterpolation::Linear)
                .unwrap();
        let geometry = IrregularContourGeometryOptions {
            regularization: Some(ContourRegularization {
                spacing: 0.04,
                ..ContourRegularization::default()
            }),
            ..IrregularContourGeometryOptions::default()
        };
        let prepared_set =
            IrregularContourSet::compute_prepared(&prepared, &[0.5], geometry).unwrap();
        let convenience_set = IrregularContourSet::compute(
            &field,
            &[0.5],
            IrregularContourOptions {
                interpolation: IrregularContourInterpolation::Linear,
                geometry,
            },
        )
        .unwrap();
        assert_eq!(prepared_set, convenience_set);
        let diagnostics = prepared_set.diagnostics();
        assert!(diagnostics.levels[0].projected_points > 0);
        assert!(diagnostics.levels[0].maximum_final_residual <= 1.0e-9);
        assert!(diagnostics.levels[0].spacing_cv_after <= diagnostics.levels[0].spacing_cv_before);
    }

    #[test]
    fn irregular_level_validation_is_consistent_with_regular_contours() {
        let field = affine_field();
        assert!(matches!(
            IrregularContourSet::compute(&field, &[f64::NAN], IrregularContourOptions::linear()),
            Err(IrregularContourError::NonFiniteLevel { .. })
        ));
        assert!(matches!(
            IrregularContourSet::compute(
                &field,
                &[0.5, 0.5 + 1.0e-12],
                IrregularContourOptions::linear()
            ),
            Err(IrregularContourError::DuplicateLevel { .. })
        ));
    }

    #[test]
    fn linear_flat_triangle_is_explicit() {
        let mesh = IrregularTernaryMesh::new([
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.4, 0.3, 0.3],
        ])
        .unwrap();
        let field = IrregularTernaryScalarField::new(mesh, vec![1.0; 4]).unwrap();
        assert!(matches!(
            IrregularContourSet::compute(&field, &[1.0], IrregularContourOptions::linear()),
            Err(IrregularContourError::FlatTriangle { .. })
        ));
    }

    #[cfg(not(feature = "irregular-cubic-alpha"))]
    #[test]
    fn cubic_irregular_contours_remain_feature_gated() {
        let field = affine_field();
        assert!(matches!(
            IrregularContourSet::compute(
                &field,
                &[0.5],
                IrregularContourOptions::cubic_alpha(IrregularCubicAlphaOptions::default()),
            ),
            Err(IrregularContourError::CubicFeatureUnavailable)
        ));
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_prepared_and_convenience_workflows_reuse_the_same_field_model() {
        let field = IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples()).unwrap(),
            |[a, b, c]| (a - 0.31).powi(2) + 0.7 * (b - 0.27).powi(2) + 0.2 * c + 0.15 * a * b,
        )
        .unwrap();
        let cubic = IrregularCubicAlphaOptions::default();
        let prepared = InterpolatedIrregularTernaryField::new(
            &field,
            IrregularFieldInterpolation::CubicAlpha(cubic),
        )
        .unwrap();
        let geometry = IrregularContourGeometryOptions {
            adaptive: IrregularAdaptiveContourOptions {
                max_depth: 6,
                maximum_microtriangle_diameter: 0.04,
                ..IrregularAdaptiveContourOptions::default()
            },
            regularization: Some(ContourRegularization {
                spacing: 0.05,
                ..ContourRegularization::default()
            }),
            ..IrregularContourGeometryOptions::default()
        };
        let level = 0.18;
        let prepared_set =
            IrregularContourSet::compute_prepared(&prepared, &[level], geometry).unwrap();
        let convenience_set = IrregularContourSet::compute(
            &field,
            &[level],
            IrregularContourOptions {
                interpolation: IrregularContourInterpolation::CubicAlpha(cubic),
                geometry,
            },
        )
        .unwrap();
        assert_eq!(prepared_set, convenience_set);
        let diagnostics = prepared_set.diagnostics();
        assert!(diagnostics.cubic_source.is_some());
        assert!(diagnostics.levels[0].canonical_shared_edge_roots > 0);
        for path in &prepared_set.levels[0].paths {
            for point in &path.points {
                close(prepared.value(point.as_array()).unwrap(), level);
            }
        }
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_shared_edge_roots_are_exact_from_each_incident_triangle() {
        let field = IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples()).unwrap(),
            |[a, b, c]| (a - 0.31).powi(2) + 0.7 * (b - 0.27).powi(2) + 0.2 * c + 0.15 * a * b,
        )
        .unwrap();
        let prepared = InterpolatedIrregularTernaryField::new(
            &field,
            IrregularFieldInterpolation::CubicAlpha(IrregularCubicAlphaOptions::default()),
        )
        .unwrap();
        let level = 0.18;
        let roots = canonical_edge_roots(&prepared, level, 1.0e-10).unwrap();
        for edge in prepared.mesh().edges().filter(|edge| !edge.is_boundary()) {
            for root in &roots[edge.id.0] {
                for triangle in edge.triangles.into_iter().flatten() {
                    let barycentric = crate::simplex::canonical_barycentric(
                        crate::simplex::barycentric_ab(
                            prepared.mesh().triangle_compositions(triangle).unwrap(),
                            root.point.as_array(),
                        )
                        .unwrap(),
                        crate::POINT_LOCATION_TOLERANCE,
                    )
                    .unwrap();
                    close(
                        prepared
                            .evaluate_in_triangle(
                                prepared.mesh().triangle(triangle).unwrap(),
                                barycentric,
                            )
                            .unwrap()
                            .0,
                        level,
                    );
                }
            }
        }
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_maximum_depth_is_reported_without_silently_dropping_cells() {
        let field = IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples()).unwrap(),
            |[a, b, c]| (a - 0.31).powi(2) + 0.7 * (b - 0.27).powi(2) + 0.2 * c + 0.15 * a * b,
        )
        .unwrap();
        let prepared = InterpolatedIrregularTernaryField::new(
            &field,
            IrregularFieldInterpolation::CubicAlpha(IrregularCubicAlphaOptions::default()),
        )
        .unwrap();
        let contours = IrregularContourSet::compute_prepared(
            &prepared,
            &[0.18],
            IrregularContourGeometryOptions {
                adaptive: IrregularAdaptiveContourOptions {
                    max_depth: 2,
                    flatness_tolerance: 1.0e-14,
                    maximum_microtriangle_diameter: 1.0e-5,
                },
                ..IrregularContourGeometryOptions::default()
            },
        )
        .unwrap();
        assert!(contours.diagnostics().levels[0].maximum_depth_hits > 0);
        assert!(!contours.levels[0].paths.is_empty());
    }
    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_edge_root_solver_preserves_reversal_and_multiple_roots() {
        let interval = crate::interpolation::AlphaInterval::new(16.0, -32.0);
        let forward = interval_roots(interval, 0.0, 0.0, 1.0, 1.0e-10, IrregularEdgeId(0)).unwrap();
        let reverse = interval_roots(
            interval.reversed(),
            0.0,
            0.0,
            1.0,
            1.0e-10,
            IrregularEdgeId(0),
        )
        .unwrap();
        assert_eq!(forward.len(), 2);
        assert_eq!(reverse.len(), 2);
        for (left, right) in forward.iter().zip(reverse.iter().rev()) {
            close(*left, 1.0 - *right);
        }
    }
}
