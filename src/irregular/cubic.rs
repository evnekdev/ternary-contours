//! Self-consistent irregular-mesh cubic-alpha preparation and evaluation.
//!
//! One [`AlphaInterval`] is stored for each canonical mesh edge. The virtual
//! endpoint locations used during Jacobi sweeps are located exactly once.

use core::fmt;

#[cfg(feature = "irregular-cubic-alpha")]
use crate::interpolation::evaluate_pair;
use crate::interpolation::{
    AlphaInterval, BinaryExtrapolation, CubicAlphaMethod, CubicBoundaryPolicy,
    DirectedAlphaInterval,
};
use crate::{POINT_LOCATION_TOLERANCE, simplex::global_gradient_ab};

use super::{
    IrregularEdgeId, IrregularFieldEvaluationError, IrregularFieldSample, IrregularMeshTriangle,
    IrregularPointLocationError, IrregularTernaryMesh, IrregularTernaryScalarField,
    IrregularTriangleId, LocatedIrregularTriangle, PreparedIrregularTernaryField,
};

/// Interpolation family used by [`InterpolatedIrregularTernaryField`].
///
/// `Muggianu`, `Kohler`, and `RawBarycentric` are interior continuation
/// policies within the cubic-alpha model, rather than separate families.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum IrregularFieldInterpolation {
    /// Piecewise-affine interpolation in each Delaunay triangle.
    #[default]
    Linear,
    /// Edge-alpha cubic interpolation; requires `irregular-cubic-alpha`.
    CubicAlpha(IrregularCubicAlphaOptions),
}

/// Numerical controls for synchronous self-consistent irregular alpha sweeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularAlphaSweepOptions {
    /// Maximum number of full Jacobi sweeps.
    pub max_sweeps: usize,
    /// Jacobi damping in `(0, 1]`.
    pub damping: f64,
    /// Absolute coefficient convergence tolerance.
    pub absolute_tolerance: f64,
    /// Relative coefficient convergence tolerance.
    pub relative_tolerance: f64,
    /// Consecutive materially non-improving sweeps before stagnation.
    pub stagnation_window: usize,
}
impl Default for IrregularAlphaSweepOptions {
    fn default() -> Self {
        Self {
            max_sweeps: 128,
            damping: 0.5,
            absolute_tolerance: 1.0e-12,
            relative_tolerance: 1.0e-8,
            stagnation_window: 16,
        }
    }
}

/// Field-construction choices for irregular edge-alpha cubic interpolation.
///
/// `method` controls the same one-dimensional alpha construction as regular
/// fields. `boundary_policy` controls unavailable virtual stencils, while
/// `extrapolation` chooses ternary interior continuation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularCubicAlphaOptions {
    /// One-dimensional cubic-alpha method for every canonical edge update.
    pub method: CubicAlphaMethod,
    /// Treatment of an unavailable virtual endpoint stencil.
    pub boundary_policy: CubicBoundaryPolicy,
    /// Ternary interior continuation policy.
    pub extrapolation: BinaryExtrapolation,
    /// Synchronous fixed-point convergence controls.
    pub sweeps: IrregularAlphaSweepOptions,
}
impl Default for IrregularCubicAlphaOptions {
    fn default() -> Self {
        Self {
            method: CubicAlphaMethod::Steffen,
            boundary_policy: CubicBoundaryPolicy::LinearFallback,
            extrapolation: BinaryExtrapolation::Muggianu,
            sweeps: IrregularAlphaSweepOptions::default(),
        }
    }
}

/// Which reflected endpoint is unavailable for a canonical edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum IrregularVirtualStencilSide {
    /// The virtual point `2*x0 - x1` before canonical endpoint zero.
    BeforeStart,
    /// The virtual point `2*x1 - x0` after canonical endpoint one.
    AfterEnd,
}

/// Why a virtual edge point cannot be used as a mesh interpolation stencil.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum IrregularVirtualStencilFailure {
    /// The reflected point lies outside the semantic ternary simplex.
    OutsideSimplex,
    /// The reflected point lies outside the supplied samples' convex hull.
    OutsideConvexHull,
}

/// Final state of the globally synchronous irregular alpha solve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum IrregularAlphaConvergence {
    /// All intervals met mixed absolute/relative tolerance.
    Converged,
    /// The configured sweep limit was reached first.
    IterationLimit,
    /// The residual stopped materially improving.
    Stagnated,
    /// A derived alpha coefficient became non-finite.
    Diverged,
}

/// Diagnostics from an irregular cubic-alpha preparation attempt.
///
/// A non-converged field is rejected; its full deterministic diagnostics are
/// carried by the typed construction error.
#[derive(Clone, Debug, PartialEq)]
pub struct IrregularCubicAlphaDiagnostics {
    /// Complete immutable construction options.
    pub options: IrregularCubicAlphaOptions,
    /// Number of canonical undirected mesh edges.
    pub edge_count: usize,
    /// Edges with both reflected virtual locations available.
    pub complete_stencil_edges: usize,
    /// Edges pinned to zero alpha by `LinearFallback`.
    pub linear_fallback_edges: usize,
    /// Missing virtual locations outside the semantic simplex.
    pub virtual_points_outside_simplex: usize,
    /// Missing virtual locations outside the mesh convex hull.
    pub virtual_points_outside_convex_hull: usize,
    /// Number of completed global Jacobi sweeps.
    pub sweep_count: usize,
    /// Last maximum mixed residual, if a sweep completed.
    pub residual: Option<f64>,
    /// Last maximum damped coefficient change, if a sweep completed.
    pub max_coefficient_change: Option<f64>,
    /// Canonical edge owning the last maximum residual.
    pub worst_edge: Option<IrregularEdgeId>,
    /// Final solver status.
    pub convergence: IrregularAlphaConvergence,
    /// Per-sweep maximum residuals in deterministic edge order.
    pub residual_history: Vec<f64>,
}
#[cfg(feature = "irregular-cubic-alpha")]
impl IrregularCubicAlphaDiagnostics {
    fn new(options: IrregularCubicAlphaOptions, edge_count: usize) -> Self {
        Self {
            options,
            edge_count,
            complete_stencil_edges: 0,
            linear_fallback_edges: 0,
            virtual_points_outside_simplex: 0,
            virtual_points_outside_convex_hull: 0,
            sweep_count: 0,
            residual: None,
            max_coefficient_change: None,
            worst_edge: None,
            convergence: IrregularAlphaConvergence::IterationLimit,
            residual_history: Vec::new(),
        }
    }
}

/// Failure while preparing the irregular cubic-alpha model.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum IrregularCubicAlphaBuildError {
    /// Solver options were not numerically meaningful.
    InvalidOptions { message: &'static str },
    /// A virtual point is unavailable and the boundary policy is `Error`.
    BoundaryStencilUnavailable {
        /// Canonical mesh edge whose stencil is incomplete.
        edge: IrregularEdgeId,
        /// Missing endpoint of that canonical directed interval.
        side: IrregularVirtualStencilSide,
        /// Geometric reason the virtual point is unavailable.
        failure: IrregularVirtualStencilFailure,
    },
    /// Point location failed for a reason other than a documented boundary.
    VirtualPointLocation {
        /// Canonical mesh edge whose virtual point failed.
        edge: IrregularEdgeId,
        /// Requested reflected endpoint.
        side: IrregularVirtualStencilSide,
        /// Underlying point-location failure.
        error: IrregularPointLocationError,
    },
    /// Fixed-point iteration did not produce an acceptable alpha field.
    NonConverged {
        /// Full deterministic solver diagnostics.
        diagnostics: Box<IrregularCubicAlphaDiagnostics>,
    },
}
impl fmt::Display for IrregularCubicAlphaBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions { message } => {
                write!(
                    formatter,
                    "invalid irregular cubic-alpha options: {message}"
                )
            }
            Self::BoundaryStencilUnavailable {
                edge,
                side,
                failure,
            } => write!(
                formatter,
                "virtual stencil {side:?} for edge {edge:?} is unavailable: {failure:?}"
            ),
            Self::VirtualPointLocation { edge, side, error } => write!(
                formatter,
                "virtual stencil {side:?} for edge {edge:?} could not be located: {error}"
            ),
            Self::NonConverged { diagnostics } => write!(
                formatter,
                "irregular cubic-alpha solver ended {:?} after {} sweeps",
                diagnostics.convergence, diagnostics.sweep_count
            ),
        }
    }
}
impl std::error::Error for IrregularCubicAlphaBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::VirtualPointLocation { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Prepared irregular pointwise scalar interpolation.
///
/// The linear variant is equivalent to [`PreparedIrregularTernaryField`]. The
/// cubic-alpha variant builds one interval per canonical edge and completes all
/// self-consistent Jacobi sweeps at construction time. Both models are C0 but
/// not promised C1; edge and vertex gradients use deterministic mesh owners.
pub struct InterpolatedIrregularTernaryField<'a> {
    field: &'a IrregularTernaryScalarField,
    interpolation: PreparedIrregularInterpolation<'a>,
}

enum PreparedIrregularInterpolation<'a> {
    Linear(PreparedIrregularTernaryField<'a>),
    #[cfg(feature = "irregular-cubic-alpha")]
    CubicAlpha(Box<IrregularCubicAlphaField<'a>>),
}

impl<'a> InterpolatedIrregularTernaryField<'a> {
    /// Prepare a reusable irregular interpolation model.
    pub fn new(
        field: &'a IrregularTernaryScalarField,
        interpolation: IrregularFieldInterpolation,
    ) -> Result<Self, IrregularFieldEvaluationError> {
        let interpolation = match interpolation {
            IrregularFieldInterpolation::Linear => {
                PreparedIrregularInterpolation::Linear(PreparedIrregularTernaryField::new(field))
            }
            IrregularFieldInterpolation::CubicAlpha(options) => {
                #[cfg(feature = "irregular-cubic-alpha")]
                {
                    PreparedIrregularInterpolation::CubicAlpha(Box::new(
                        IrregularCubicAlphaField::new(field, options).map_err(|error| {
                            IrregularFieldEvaluationError::CubicConstruction(Box::new(error))
                        })?,
                    ))
                }
                #[cfg(not(feature = "irregular-cubic-alpha"))]
                {
                    let _ = options;
                    return Err(IrregularFieldEvaluationError::CubicFeatureUnavailable);
                }
            }
        };
        Ok(Self {
            field,
            interpolation,
        })
    }

    /// Return the interpolation family selected at construction time.
    pub fn interpolation(&self) -> IrregularFieldInterpolation {
        match &self.interpolation {
            PreparedIrregularInterpolation::Linear(_) => IrregularFieldInterpolation::Linear,
            #[cfg(feature = "irregular-cubic-alpha")]
            PreparedIrregularInterpolation::CubicAlpha(cubic) => {
                IrregularFieldInterpolation::CubicAlpha(cubic.diagnostics.options)
            }
        }
    }

    /// Evaluate one already-known mesh-local barycentric position without
    /// performing global point location.
    ///
    /// This crate-private hook is used by adaptive contour extraction. Public
    /// callers should use [`Self::evaluate_at_location`] so mesh identity and
    /// semantic composition validation remain explicit.
    #[cfg_attr(not(feature = "irregular-cubic-alpha"), allow(dead_code))]
    pub(crate) fn evaluate_in_triangle(
        &self,
        triangle: IrregularMeshTriangle,
        barycentric: [f64; 3],
    ) -> Result<(f64, [f64; 2]), IrregularFieldEvaluationError> {
        let expected = self
            .field
            .mesh
            .triangles
            .get(triangle.id.0)
            .copied()
            .ok_or(IrregularFieldEvaluationError::InvalidLocation {
                triangle: triangle.id,
            })?;
        if expected != triangle
            || !crate::simplex::valid_barycentric(barycentric, POINT_LOCATION_TOLERANCE)
        {
            return Err(IrregularFieldEvaluationError::InvalidLocation {
                triangle: triangle.id,
            });
        }
        match &self.interpolation {
            PreparedIrregularInterpolation::Linear(_) => {
                let values = triangle.vertices.map(|vertex| self.field.values[vertex.0]);
                let value = values
                    .into_iter()
                    .zip(barycentric)
                    .map(|(sample, weight)| sample * weight)
                    .sum();
                let gradient_ab = global_gradient_ab(
                    self.field
                        .mesh
                        .triangle_compositions(triangle.id)
                        .map_err(|_| IrregularFieldEvaluationError::InvalidLocation {
                            triangle: triangle.id,
                        })?,
                    [values[0] - values[2], values[1] - values[2]],
                )
                .ok_or(IrregularFieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?;
                Ok((value, gradient_ab))
            }
            #[cfg(feature = "irregular-cubic-alpha")]
            PreparedIrregularInterpolation::CubicAlpha(cubic) => {
                cubic.evaluate_triangle(triangle, barycentric)
            }
        }
    }

    /// Return the sampled scalar field used by this evaluator.
    pub const fn field(&self) -> &'a IrregularTernaryScalarField {
        self.field
    }
    /// Return the immutable Delaunay mesh used by this evaluator.
    pub fn mesh(&self) -> &'a IrregularTernaryMesh {
        self.field.mesh()
    }
    /// Evaluate only the scalar value at a semantic composition.
    pub fn value(&self, composition: [f64; 3]) -> Result<f64, IrregularFieldEvaluationError> {
        Ok(self.evaluate(composition)?.value)
    }
    /// Evaluate value, analytic global `(a,b)` gradient, and location.
    pub fn evaluate(
        &self,
        composition: [f64; 3],
    ) -> Result<IrregularFieldSample, IrregularFieldEvaluationError> {
        let location = self.field.mesh().locate(composition)?;
        self.evaluate_at_location(&location)
    }
    /// Evaluate only the scalar value at a prior mesh location.
    pub fn value_at_location(
        &self,
        location: &LocatedIrregularTriangle,
    ) -> Result<f64, IrregularFieldEvaluationError> {
        Ok(self.evaluate_at_location(location)?.value)
    }
    /// Evaluate without repeating composition validation or point location.
    pub fn evaluate_at_location(
        &self,
        location: &LocatedIrregularTriangle,
    ) -> Result<IrregularFieldSample, IrregularFieldEvaluationError> {
        match &self.interpolation {
            PreparedIrregularInterpolation::Linear(linear) => linear.evaluate_at_location(location),
            #[cfg(feature = "irregular-cubic-alpha")]
            PreparedIrregularInterpolation::CubicAlpha(cubic) => {
                cubic.evaluate_at_location(location)
            }
        }
    }
    /// Lazily evaluate a batch without reconstructing this prepared model.
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
    /// Values before a failing input are written; the remainder is unchanged.
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
    /// Return cubic solver diagnostics, if cubic-alpha was selected.
    pub fn cubic_diagnostics(&self) -> Option<&IrregularCubicAlphaDiagnostics> {
        match &self.interpolation {
            PreparedIrregularInterpolation::Linear(_) => None,
            #[cfg(feature = "irregular-cubic-alpha")]
            PreparedIrregularInterpolation::CubicAlpha(cubic) => Some(cubic.diagnostics()),
        }
    }
    /// Return final canonical alpha intervals in dense mesh-edge order.
    pub fn cubic_alpha_intervals(&self) -> Option<&[AlphaInterval]> {
        match &self.interpolation {
            PreparedIrregularInterpolation::Linear(_) => None,
            #[cfg(feature = "irregular-cubic-alpha")]
            PreparedIrregularInterpolation::CubicAlpha(cubic) => Some(cubic.intervals()),
        }
    }
    /// Return one final canonical alpha interval by mesh edge ID.
    pub fn cubic_alpha_interval(&self, edge: IrregularEdgeId) -> Option<AlphaInterval> {
        self.cubic_alpha_intervals()
            .and_then(|intervals| intervals.get(edge.0))
            .copied()
    }
    /// Return local directed intervals in pair order `(0,1)`, `(1,2)`, `(0,2)`.
    pub fn cubic_triangle_intervals(
        &self,
        _triangle: IrregularTriangleId,
    ) -> Option<[DirectedAlphaInterval; 3]> {
        match &self.interpolation {
            PreparedIrregularInterpolation::Linear(_) => None,
            #[cfg(feature = "irregular-cubic-alpha")]
            PreparedIrregularInterpolation::CubicAlpha(cubic) => {
                cubic.triangle_intervals(_triangle)
            }
        }
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
#[derive(Clone, Copy)]
struct CachedVirtualLocation {
    triangle: IrregularTriangleId,
    barycentric_uv: [f64; 2],
}
#[cfg(feature = "irregular-cubic-alpha")]
impl CachedVirtualLocation {
    fn from_location(location: LocatedIrregularTriangle) -> Self {
        Self {
            triangle: location.triangle.id,
            barycentric_uv: [location.barycentric[0], location.barycentric[1]],
        }
    }
    fn barycentric(self) -> [f64; 3] {
        [
            self.barycentric_uv[0],
            self.barycentric_uv[1],
            1.0 - self.barycentric_uv[0] - self.barycentric_uv[1],
        ]
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
enum EdgeStencil {
    Complete {
        before_start: CachedVirtualLocation,
        after_end: CachedVirtualLocation,
    },
    LinearFallback,
}

#[cfg(feature = "irregular-cubic-alpha")]
#[derive(Clone, Copy)]
struct TriangleAlphaAccess {
    edges: [IrregularEdgeId; 3],
    reversed_mask: u8,
}

#[cfg(feature = "irregular-cubic-alpha")]
struct IrregularCubicAlphaField<'a> {
    field: &'a IrregularTernaryScalarField,
    intervals: Box<[AlphaInterval]>,
    triangle_access: Box<[TriangleAlphaAccess]>,
    diagnostics: IrregularCubicAlphaDiagnostics,
}

#[cfg(feature = "irregular-cubic-alpha")]
impl<'a> IrregularCubicAlphaField<'a> {
    fn new(
        field: &'a IrregularTernaryScalarField,
        options: IrregularCubicAlphaOptions,
    ) -> Result<Self, IrregularCubicAlphaBuildError> {
        validate_sweep_options(options.sweeps)?;
        let mesh = field.mesh();
        let mut diagnostics = IrregularCubicAlphaDiagnostics::new(options, mesh.edge_count());
        let stencils = build_stencils(mesh, options.boundary_policy, &mut diagnostics)?;
        let triangle_access = build_triangle_access(mesh);
        let intervals = solve_intervals(
            field,
            &stencils,
            &triangle_access,
            options,
            &mut diagnostics,
        )?;
        Ok(Self {
            field,
            intervals: intervals.into_boxed_slice(),
            triangle_access: triangle_access.into_boxed_slice(),
            diagnostics,
        })
    }
    const fn diagnostics(&self) -> &IrregularCubicAlphaDiagnostics {
        &self.diagnostics
    }
    fn intervals(&self) -> &[AlphaInterval] {
        &self.intervals
    }
    fn triangle_intervals(
        &self,
        _triangle: IrregularTriangleId,
    ) -> Option<[DirectedAlphaInterval; 3]> {
        let access = *self.triangle_access.get(_triangle.0)?;
        let directed = [
            self.directed_interval(access, 0),
            self.directed_interval(access, 1),
            self.directed_interval(access, 2),
        ];
        Some([
            DirectedAlphaInterval::new(0, 1, directed[0]).expect("cached finite local interval"),
            DirectedAlphaInterval::new(1, 2, directed[1]).expect("cached finite local interval"),
            DirectedAlphaInterval::new(0, 2, directed[2]).expect("cached finite local interval"),
        ])
    }
    fn evaluate_at_location(
        &self,
        location: &LocatedIrregularTriangle,
    ) -> Result<IrregularFieldSample, IrregularFieldEvaluationError> {
        let linear = PreparedIrregularTernaryField::new(self.field);
        let triangle = linear.validated_triangle(location)?;
        let (value, gradient_ab) = self.evaluate_triangle(triangle, location.barycentric)?;
        Ok(IrregularFieldSample {
            value,
            gradient_ab,
            location: *location,
        })
    }
    fn evaluate_triangle(
        &self,
        triangle: IrregularMeshTriangle,
        barycentric: [f64; 3],
    ) -> Result<(f64, [f64; 2]), IrregularFieldEvaluationError> {
        let (value, reduced_gradient) = self.evaluate_triangle_unchecked(triangle, barycentric);
        let gradient_ab = global_gradient_ab(
            self.field
                .mesh
                .triangle_compositions(triangle.id)
                .map_err(|_| IrregularFieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
            reduced_gradient,
        )
        .ok_or(IrregularFieldEvaluationError::InvalidLocation {
            triangle: triangle.id,
        })?;
        Ok((value, gradient_ab))
    }
    fn evaluate_triangle_unchecked(
        &self,
        triangle: IrregularMeshTriangle,
        barycentric: [f64; 3],
    ) -> (f64, [f64; 2]) {
        evaluate_with_intervals(
            self.field,
            triangle,
            barycentric,
            &self.intervals,
            self.triangle_access[triangle.id.0],
            self.diagnostics.options.extrapolation,
        )
    }
    #[cfg(feature = "irregular-cubic-alpha")]
    fn directed_interval(&self, access: TriangleAlphaAccess, index: usize) -> AlphaInterval {
        directed_interval(&self.intervals, access, index)
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
fn directed_interval(
    intervals: &[AlphaInterval],
    access: TriangleAlphaAccess,
    index: usize,
) -> AlphaInterval {
    let interval = intervals[access.edges[index].0];
    if (access.reversed_mask & (1 << index)) != 0 {
        interval.reversed()
    } else {
        interval
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
fn evaluate_with_intervals(
    field: &IrregularTernaryScalarField,
    triangle: IrregularMeshTriangle,
    barycentric: [f64; 3],
    intervals: &[AlphaInterval],
    access: TriangleAlphaAccess,
    extrapolation: BinaryExtrapolation,
) -> (f64, [f64; 2]) {
    let values = triangle.vertices.map(|vertex| field.values[vertex.0]);
    let mut value = values
        .into_iter()
        .zip(barycentric)
        .map(|(sample, weight)| sample * weight)
        .sum::<f64>();
    let mut partial = values;
    for (index, (start, end)) in [(0, 1), (1, 2), (0, 2)].into_iter().enumerate() {
        let remaining = 3 - start - end;
        let pair = evaluate_pair(
            barycentric[start],
            barycentric[end],
            barycentric[remaining],
            directed_interval(intervals, access, index),
            extrapolation,
        );
        value += pair.value;
        partial[start] += pair.derivatives[0];
        partial[end] += pair.derivatives[1];
        partial[remaining] += pair.derivatives[2];
    }
    (value, [partial[0] - partial[2], partial[1] - partial[2]])
}

#[cfg(feature = "irregular-cubic-alpha")]
fn validate_sweep_options(
    options: IrregularAlphaSweepOptions,
) -> Result<(), IrregularCubicAlphaBuildError> {
    if options.max_sweeps == 0 {
        return Err(IrregularCubicAlphaBuildError::InvalidOptions {
            message: "max_sweeps must be greater than zero",
        });
    }
    if !options.damping.is_finite() || options.damping <= 0.0 || options.damping > 1.0 {
        return Err(IrregularCubicAlphaBuildError::InvalidOptions {
            message: "damping must be finite and in (0, 1]",
        });
    }
    if !options.absolute_tolerance.is_finite()
        || !options.relative_tolerance.is_finite()
        || options.absolute_tolerance < 0.0
        || options.relative_tolerance < 0.0
        || (options.absolute_tolerance == 0.0 && options.relative_tolerance == 0.0)
    {
        return Err(IrregularCubicAlphaBuildError::InvalidOptions {
            message: "at least one finite positive convergence tolerance is required",
        });
    }
    if options.stagnation_window == 0 {
        return Err(IrregularCubicAlphaBuildError::InvalidOptions {
            message: "stagnation_window must be greater than zero",
        });
    }
    Ok(())
}

#[cfg(feature = "irregular-cubic-alpha")]
fn build_stencils(
    mesh: &IrregularTernaryMesh,
    boundary_policy: CubicBoundaryPolicy,
    diagnostics: &mut IrregularCubicAlphaDiagnostics,
) -> Result<Vec<EdgeStencil>, IrregularCubicAlphaBuildError> {
    mesh.edges
        .iter()
        .copied()
        .map(|edge| {
            let start = mesh.compositions[edge.vertices[0].0];
            let end = mesh.compositions[edge.vertices[1].0];
            let hint = edge.triangles[0].expect("mesh edge has an incident triangle");
            let before = locate_virtual(
                mesh,
                reflect(start, end),
                hint,
                IrregularVirtualStencilSide::BeforeStart,
            );
            let after = locate_virtual(
                mesh,
                reflect(end, start),
                hint,
                IrregularVirtualStencilSide::AfterEnd,
            );
            match (before, after) {
                (Ok(before_start), Ok(after_end)) => {
                    diagnostics.complete_stencil_edges += 1;
                    Ok(EdgeStencil::Complete {
                        before_start,
                        after_end,
                    })
                }
                (before, after) => {
                    let mut first_boundary = None;
                    for failure in [before.err(), after.err()].into_iter().flatten() {
                        match failure {
                            VirtualLocationFailure::Boundary(side, failure) => {
                                match failure {
                                    IrregularVirtualStencilFailure::OutsideSimplex => {
                                        diagnostics.virtual_points_outside_simplex += 1
                                    }
                                    IrregularVirtualStencilFailure::OutsideConvexHull => {
                                        diagnostics.virtual_points_outside_convex_hull += 1
                                    }
                                }
                                first_boundary.get_or_insert((side, failure));
                            }
                            VirtualLocationFailure::Location(side, error) => {
                                return Err(IrregularCubicAlphaBuildError::VirtualPointLocation {
                                    edge: edge.id,
                                    side,
                                    error,
                                });
                            }
                        }
                    }
                    let (side, failure) = first_boundary
                        .expect("incomplete virtual stencil must record a boundary failure");
                    match boundary_policy {
                        CubicBoundaryPolicy::LinearFallback => {
                            diagnostics.linear_fallback_edges += 1;
                            Ok(EdgeStencil::LinearFallback)
                        }
                        CubicBoundaryPolicy::Error => {
                            Err(IrregularCubicAlphaBuildError::BoundaryStencilUnavailable {
                                edge: edge.id,
                                side,
                                failure,
                            })
                        }
                    }
                }
            }
        })
        .collect()
}

#[cfg(feature = "irregular-cubic-alpha")]
enum VirtualLocationFailure {
    Boundary(IrregularVirtualStencilSide, IrregularVirtualStencilFailure),
    Location(IrregularVirtualStencilSide, IrregularPointLocationError),
}
#[cfg(feature = "irregular-cubic-alpha")]
fn locate_virtual(
    mesh: &IrregularTernaryMesh,
    composition: [f64; 3],
    hint: IrregularTriangleId,
    side: IrregularVirtualStencilSide,
) -> Result<CachedVirtualLocation, VirtualLocationFailure> {
    mesh.locate_with_hint(composition, Some(hint))
        .map(CachedVirtualLocation::from_location)
        .map_err(|error| match error {
            IrregularPointLocationError::OutsideSimplex { .. }
            | IrregularPointLocationError::InvalidCompositionSum { .. } => {
                VirtualLocationFailure::Boundary(
                    side,
                    IrregularVirtualStencilFailure::OutsideSimplex,
                )
            }
            IrregularPointLocationError::OutsideConvexHull { .. } => {
                VirtualLocationFailure::Boundary(
                    side,
                    IrregularVirtualStencilFailure::OutsideConvexHull,
                )
            }
            error => VirtualLocationFailure::Location(side, error),
        })
}

#[cfg(feature = "irregular-cubic-alpha")]
fn reflect(endpoint: [f64; 3], other: [f64; 3]) -> [f64; 3] {
    [
        2.0 * endpoint[0] - other[0],
        2.0 * endpoint[1] - other[1],
        2.0 * endpoint[2] - other[2],
    ]
}

#[cfg(feature = "irregular-cubic-alpha")]
fn build_triangle_access(mesh: &IrregularTernaryMesh) -> Vec<TriangleAlphaAccess> {
    mesh.triangles
        .iter()
        .copied()
        .map(|triangle| {
            let mut reversed_mask = 0_u8;
            for (index, (start, _)) in [(0, 1), (1, 2), (0, 2)].into_iter().enumerate() {
                let edge = mesh.edges[triangle.edges[index].0];
                if edge.vertices[0] != triangle.vertices[start] {
                    reversed_mask |= 1 << index;
                }
            }
            TriangleAlphaAccess {
                edges: triangle.edges,
                reversed_mask,
            }
        })
        .collect()
}

#[cfg(feature = "irregular-cubic-alpha")]
fn solve_intervals(
    field: &IrregularTernaryScalarField,
    stencils: &[EdgeStencil],
    triangle_access: &[TriangleAlphaAccess],
    options: IrregularCubicAlphaOptions,
    diagnostics: &mut IrregularCubicAlphaDiagnostics,
) -> Result<Vec<AlphaInterval>, IrregularCubicAlphaBuildError> {
    use crate::interpolation::alpha_from_uniform_four_values;

    let mut current = stencils
        .iter()
        .enumerate()
        .map(|(index, stencil)| match stencil {
            EdgeStencil::LinearFallback => AlphaInterval::default(),
            EdgeStencil::Complete {
                before_start,
                after_end,
            } => {
                let edge = field.mesh.edges[index];
                alpha_from_uniform_four_values(
                    options.method,
                    [
                        linear_cached_value(field, *before_start),
                        field.values[edge.vertices[0].0],
                        field.values[edge.vertices[1].0],
                        linear_cached_value(field, *after_end),
                    ],
                )
            }
        })
        .collect::<Vec<_>>();
    let mut next = vec![AlphaInterval::default(); current.len()];
    let mut best_residual = f64::INFINITY;
    let mut stagnant_sweeps = 0_usize;

    for sweep in 1..=options.sweeps.max_sweeps {
        let model = SweepModel {
            field,
            intervals: &current,
            options,
            triangle_access,
        };
        let mut maximum_residual = 0.0_f64;
        let mut maximum_change = 0.0_f64;
        let mut worst_edge = None;
        for (index, stencil) in stencils.iter().enumerate() {
            let target = match stencil {
                EdgeStencil::LinearFallback => AlphaInterval::default(),
                EdgeStencil::Complete {
                    before_start,
                    after_end,
                } => {
                    let edge = field.mesh.edges[index];
                    alpha_from_uniform_four_values(
                        options.method,
                        [
                            model.evaluate_cached(*before_start),
                            field.values[edge.vertices[0].0],
                            field.values[edge.vertices[1].0],
                            model.evaluate_cached(*after_end),
                        ],
                    )
                }
            };
            if !target.alpha0.is_finite() || !target.alpha1.is_finite() {
                diagnostics.sweep_count = sweep;
                diagnostics.convergence = IrregularAlphaConvergence::Diverged;
                return Err(IrregularCubicAlphaBuildError::NonConverged {
                    diagnostics: Box::new(diagnostics.clone()),
                });
            }
            let previous = current[index];
            let residual = normalized_interval_difference(previous, target, options.sweeps);
            if residual > maximum_residual {
                maximum_residual = residual;
                worst_edge = Some(IrregularEdgeId(index));
            }
            let updated = AlphaInterval::new(
                previous.alpha0 + options.sweeps.damping * (target.alpha0 - previous.alpha0),
                previous.alpha1 + options.sweeps.damping * (target.alpha1 - previous.alpha1),
            );
            if !updated.alpha0.is_finite() || !updated.alpha1.is_finite() {
                diagnostics.sweep_count = sweep;
                diagnostics.convergence = IrregularAlphaConvergence::Diverged;
                return Err(IrregularCubicAlphaBuildError::NonConverged {
                    diagnostics: Box::new(diagnostics.clone()),
                });
            }
            maximum_change = maximum_change.max((updated.alpha0 - previous.alpha0).abs());
            maximum_change = maximum_change.max((updated.alpha1 - previous.alpha1).abs());
            next[index] = updated;
        }
        diagnostics.sweep_count = sweep;
        diagnostics.residual = Some(maximum_residual);
        diagnostics.max_coefficient_change = Some(maximum_change);
        diagnostics.worst_edge = worst_edge;
        diagnostics.residual_history.push(maximum_residual);
        if maximum_residual <= 1.0 {
            diagnostics.convergence = IrregularAlphaConvergence::Converged;
            return Ok(next);
        }
        if maximum_residual < best_residual * (1.0 - 1.0e-3) {
            best_residual = maximum_residual;
            stagnant_sweeps = 0;
        } else {
            stagnant_sweeps += 1;
            if stagnant_sweeps >= options.sweeps.stagnation_window {
                diagnostics.convergence = IrregularAlphaConvergence::Stagnated;
                return Err(IrregularCubicAlphaBuildError::NonConverged {
                    diagnostics: Box::new(diagnostics.clone()),
                });
            }
        }
        core::mem::swap(&mut current, &mut next);
    }
    diagnostics.convergence = IrregularAlphaConvergence::IterationLimit;
    Err(IrregularCubicAlphaBuildError::NonConverged {
        diagnostics: Box::new(diagnostics.clone()),
    })
}

#[cfg(feature = "irregular-cubic-alpha")]
struct SweepModel<'a> {
    field: &'a IrregularTernaryScalarField,
    intervals: &'a [AlphaInterval],
    options: IrregularCubicAlphaOptions,
    triangle_access: &'a [TriangleAlphaAccess],
}
#[cfg(feature = "irregular-cubic-alpha")]
impl SweepModel<'_> {
    fn evaluate_cached(&self, location: CachedVirtualLocation) -> f64 {
        let triangle = self.field.mesh.triangles[location.triangle.0];
        evaluate_with_intervals(
            self.field,
            triangle,
            location.barycentric(),
            self.intervals,
            self.triangle_access[triangle.id.0],
            self.options.extrapolation,
        )
        .0
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
fn linear_cached_value(
    field: &IrregularTernaryScalarField,
    location: CachedVirtualLocation,
) -> f64 {
    let triangle = field.mesh.triangles[location.triangle.0];
    triangle
        .vertices
        .into_iter()
        .zip(location.barycentric())
        .map(|(vertex, weight)| field.values[vertex.0] * weight)
        .sum()
}

#[cfg(feature = "irregular-cubic-alpha")]
fn normalized_interval_difference(
    current: AlphaInterval,
    target: AlphaInterval,
    options: IrregularAlphaSweepOptions,
) -> f64 {
    let scale0 = options.absolute_tolerance
        + options.relative_tolerance * current.alpha0.abs().max(target.alpha0.abs());
    let scale1 = options.absolute_tolerance
        + options.relative_tolerance * current.alpha1.abs().max(target.alpha1.abs());
    ((target.alpha0 - current.alpha0).abs() / scale0)
        .max((target.alpha1 - current.alpha1).abs() / scale1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IrregularTernaryMesh, IrregularTernaryScalarField};

    #[cfg(feature = "irregular-cubic-alpha")]
    const TOLERANCE: f64 = 2.0e-9;
    #[cfg(feature = "irregular-cubic-alpha")]
    fn close(left: f64, right: f64) {
        assert!((left - right).abs() <= TOLERANCE, "{left:?} != {right:?}");
    }
    fn samples() -> [[f64; 3]; 7] {
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
    fn field() -> IrregularTernaryScalarField {
        IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples()).unwrap(),
            |[a, b, c]| 0.7 * a * a - 0.2 * b * b + 0.5 * c * c + 0.3 * a * b - 0.4 * b * c,
        )
        .unwrap()
    }
    #[test]
    fn linear_wrapper_matches_legacy_evaluator() {
        let field = field();
        let wrapped =
            InterpolatedIrregularTernaryField::new(&field, IrregularFieldInterpolation::Linear)
                .unwrap();
        let legacy = PreparedIrregularTernaryField::new(&field);
        for point in [[0.4, 0.3, 0.3], [0.2, 0.5, 0.3], [0.25, 0.25, 0.5]] {
            assert_eq!(
                wrapped.evaluate(point).unwrap(),
                legacy.evaluate(point).unwrap()
            );
        }
    }
    #[cfg(not(feature = "irregular-cubic-alpha"))]
    #[test]
    fn cubic_selection_reports_feature_unavailable() {
        let field = field();
        assert!(matches!(
            InterpolatedIrregularTernaryField::new(
                &field,
                IrregularFieldInterpolation::CubicAlpha(IrregularCubicAlphaOptions::default())
            ),
            Err(IrregularFieldEvaluationError::CubicFeatureUnavailable)
        ));
    }
    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_reproduces_vertices_and_has_cached_diagnostics() {
        let field = field();
        let evaluator = InterpolatedIrregularTernaryField::new(
            &field,
            IrregularFieldInterpolation::CubicAlpha(IrregularCubicAlphaOptions::default()),
        )
        .unwrap();
        let diagnostics = evaluator.cubic_diagnostics().unwrap();
        assert_eq!(
            diagnostics.convergence,
            IrregularAlphaConvergence::Converged
        );
        assert_eq!(diagnostics.edge_count, field.mesh().edge_count());
        assert_eq!(
            evaluator.cubic_alpha_intervals().unwrap().len(),
            field.mesh().edge_count()
        );
        for vertex in field.mesh().vertex_ids() {
            let composition = field.mesh().composition(vertex).unwrap();
            close(
                evaluator.value(composition).unwrap(),
                field.value(vertex).unwrap(),
            );
        }
    }
    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_batch_and_cached_location_agree() {
        let field = field();
        let evaluator = InterpolatedIrregularTernaryField::new(
            &field,
            IrregularFieldInterpolation::CubicAlpha(IrregularCubicAlphaOptions::default()),
        )
        .unwrap();
        let inputs = [[0.4, 0.3, 0.3], [0.2, 0.5, 0.3], [0.25, 0.25, 0.5]];
        let scalar = inputs.map(|point| evaluator.value(point).unwrap());
        let batched = evaluator
            .values(inputs)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for (actual, expected) in batched.into_iter().zip(scalar) {
            close(actual, expected);
        }
        let location = field.mesh().locate(inputs[0]).unwrap();
        assert_eq!(
            evaluator.evaluate(inputs[0]).unwrap(),
            evaluator.evaluate_at_location(&location).unwrap()
        );
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    fn cubic(
        field: &IrregularTernaryScalarField,
        options: IrregularCubicAlphaOptions,
    ) -> InterpolatedIrregularTernaryField<'_> {
        InterpolatedIrregularTernaryField::new(
            field,
            IrregularFieldInterpolation::CubicAlpha(options),
        )
        .unwrap()
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn all_cubic_methods_and_interior_policies_prepare_deterministically() {
        let field = field();
        for method in [
            CubicAlphaMethod::Akima,
            CubicAlphaMethod::Makima,
            CubicAlphaMethod::Pchip,
            CubicAlphaMethod::Steffen,
        ] {
            for extrapolation in [
                BinaryExtrapolation::Muggianu,
                BinaryExtrapolation::Kohler,
                BinaryExtrapolation::RawBarycentric,
            ] {
                let options = IrregularCubicAlphaOptions {
                    method,
                    extrapolation,
                    ..IrregularCubicAlphaOptions::default()
                };
                let first = cubic(&field, options);
                let second = cubic(&field, options);
                assert_eq!(
                    first.cubic_alpha_intervals(),
                    second.cubic_alpha_intervals()
                );
                assert_eq!(first.cubic_diagnostics(), second.cubic_diagnostics());
            }
        }
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_uses_single_directed_interval_and_is_c0_on_shared_edges() {
        use crate::simplex::barycentric_ab;

        let field = field();
        let evaluator = cubic(&field, IrregularCubicAlphaOptions::default());
        let cubic = match &evaluator.interpolation {
            PreparedIrregularInterpolation::CubicAlpha(cubic) => cubic,
            PreparedIrregularInterpolation::Linear(_) => unreachable!(),
        };
        for edge in field.mesh().edges() {
            let start = field.mesh().composition(edge.vertices[0]).unwrap();
            let end = field.mesh().composition(edge.vertices[1]).unwrap();
            let midpoint = [
                (start[0] + end[0]) * 0.5,
                (start[1] + end[1]) * 0.5,
                (start[2] + end[2]) * 0.5,
            ];
            let mut values = Vec::new();
            for triangle_id in edge.triangles.into_iter().flatten() {
                let triangle = field.mesh().triangle(triangle_id).unwrap();
                let barycentric = barycentric_ab(
                    field.mesh().triangle_compositions(triangle_id).unwrap(),
                    midpoint,
                )
                .unwrap();
                values.push(cubic.evaluate_triangle_unchecked(triangle, barycentric).0);
            }
            if values.len() == 2 {
                close(values[0], values[1]);
            }

            let interval = evaluator.cubic_alpha_interval(edge.id).unwrap();
            let t = 0.37;
            let expected = interval.value(
                field.value(edge.vertices[0]).unwrap(),
                field.value(edge.vertices[1]).unwrap(),
                t,
            );
            let point = [
                start[0] * (1.0 - t) + end[0] * t,
                start[1] * (1.0 - t) + end[1] * t,
                start[2] * (1.0 - t) + end[2] * t,
            ];
            close(evaluator.value(point).unwrap(), expected);
        }
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_reproduces_affine_values_and_global_gradients() {
        let field = IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples()).unwrap(),
            |[a, b, c]| 2.25 * a - 3.5 * b + 0.75 * c + 1.125,
        )
        .unwrap();
        let evaluator = cubic(&field, IrregularCubicAlphaOptions::default());
        for triangle in field.mesh().triangles() {
            let points = field.mesh().triangle_compositions(triangle.id).unwrap();
            let point = [
                0.2 * points[0][0] + 0.3 * points[1][0] + 0.5 * points[2][0],
                0.2 * points[0][1] + 0.3 * points[1][1] + 0.5 * points[2][1],
                0.2 * points[0][2] + 0.3 * points[1][2] + 0.5 * points[2][2],
            ];
            let sample = evaluator.evaluate(point).unwrap();
            close(
                sample.value,
                2.25 * point[0] - 3.5 * point[1] + 0.75 * point[2] + 1.125,
            );
            close(sample.gradient_ab[0], 1.5);
            close(sample.gradient_ab[1], -4.25);
        }
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn boundary_error_and_invalid_solver_options_are_typed() {
        let field = field();
        let boundary = IrregularCubicAlphaOptions {
            boundary_policy: CubicBoundaryPolicy::Error,
            ..IrregularCubicAlphaOptions::default()
        };
        assert!(matches!(
            InterpolatedIrregularTernaryField::new(
                &field,
                IrregularFieldInterpolation::CubicAlpha(boundary)
            ),
            Err(IrregularFieldEvaluationError::CubicConstruction(error))
                if matches!(*error, IrregularCubicAlphaBuildError::BoundaryStencilUnavailable { .. })
        ));
        let invalid = IrregularCubicAlphaOptions {
            sweeps: IrregularAlphaSweepOptions {
                max_sweeps: 0,
                ..IrregularAlphaSweepOptions::default()
            },
            ..IrregularCubicAlphaOptions::default()
        };
        assert!(matches!(
            InterpolatedIrregularTernaryField::new(
                &field,
                IrregularFieldInterpolation::CubicAlpha(invalid)
            ),
            Err(IrregularFieldEvaluationError::CubicConstruction(error))
                if matches!(*error, IrregularCubicAlphaBuildError::InvalidOptions { .. })
        ));
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_batch_reports_partial_write_and_mesh_incompatibility() {
        let first = field();
        let second = field();
        let evaluator = cubic(&first, IrregularCubicAlphaOptions::default());
        assert_eq!(
            evaluator.values([]).collect::<Result<Vec<_>, _>>().unwrap(),
            Vec::<f64>::new()
        );
        let mut wrong_size = [0.0; 2];
        assert!(matches!(
            evaluator.values_into(
                &[[0.4, 0.3, 0.3], [0.2, 0.5, 0.3], [0.25, 0.25, 0.5]],
                &mut wrong_size
            ),
            Err(IrregularFieldEvaluationError::OutputSizeMismatch {
                expected: 3,
                actual: 2
            })
        ));
        let mut output = [f64::NAN; 3];
        assert!(matches!(
            evaluator.values_into(
                &[[0.4, 0.3, 0.3], [1.1, 0.0, 0.0], [0.2, 0.5, 0.3]],
                &mut output
            ),
            Err(IrregularFieldEvaluationError::PointLocation(_))
        ));
        assert!(output[0].is_finite());
        assert!(output[2].is_nan());
        let foreign = second.mesh().locate([0.4, 0.3, 0.3]).unwrap();
        assert!(matches!(
            evaluator.evaluate_at_location(&foreign),
            Err(IrregularFieldEvaluationError::IncompatibleLocation)
        ));
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn dense_irregular_mesh_exercises_self_consistent_sweeps() {
        let samples = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.82, 0.11, 0.07],
            [0.64, 0.27, 0.09],
            [0.53, 0.31, 0.16],
            [0.72, 0.08, 0.20],
            [0.48, 0.39, 0.13],
            [0.34, 0.51, 0.15],
            [0.21, 0.65, 0.14],
            [0.13, 0.73, 0.14],
            [0.16, 0.49, 0.35],
            [0.27, 0.28, 0.45],
            [0.38, 0.17, 0.45],
            [0.46, 0.11, 0.43],
            [0.25, 0.16, 0.59],
            [0.12, 0.26, 0.62],
            [0.18, 0.11, 0.71],
            [0.38, 0.37, 0.25],
            [0.31, 0.44, 0.25],
            [0.55, 0.19, 0.26],
        ];
        let field = IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples).unwrap(),
            |[a, b, c]| a * a + 0.7 * b * b - 0.4 * c * c + 0.9 * a * b * c,
        )
        .unwrap();
        let evaluator = cubic(&field, IrregularCubicAlphaOptions::default());
        let diagnostics = evaluator.cubic_diagnostics().unwrap();
        assert!(diagnostics.complete_stencil_edges > 0);
        assert!(
            diagnostics.sweep_count > 1,
            "expected coupled virtual stencil updates"
        );
        assert_eq!(diagnostics.residual_history.len(), diagnostics.sweep_count);
        assert!(diagnostics.residual.unwrap().is_finite());
    }

    #[cfg(feature = "irregular-cubic-alpha")]
    #[test]
    fn cubic_muggianu_preserves_component_permutation_symmetry() {
        let original = IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples()).unwrap(),
            |[a, b, c]| a * a + b * b + c * c + 0.3 * a * b * c,
        )
        .unwrap();
        let permuted = IrregularTernaryScalarField::from_fn(
            IrregularTernaryMesh::new(samples().map(|[a, b, c]| [b, c, a])).unwrap(),
            |[a, b, c]| a * a + b * b + c * c + 0.3 * a * b * c,
        )
        .unwrap();
        let options = IrregularCubicAlphaOptions::default();
        let original_evaluator = cubic(&original, options);
        let permuted_evaluator = cubic(&permuted, options);
        let point = [0.41, 0.34, 0.25];
        close(
            original_evaluator.value(point).unwrap(),
            permuted_evaluator
                .value([point[1], point[2], point[0]])
                .unwrap(),
        );
    }
}
