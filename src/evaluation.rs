//! Prepared pointwise interpolation of regular ternary scalar fields.

use core::fmt;

#[cfg(feature = "cubic-alpha")]
use crate::RegularTernaryPartialScalarField;

use crate::{
    CubicAlphaBuildOptions, FieldError, GridTriangle, LocatedTriangle, PointLocationError,
    RegularTernaryGrid, RegularTernaryScalarField,
    field::{CubicBuildDiagnostics, CubicGridField},
    simplex::global_gradient_ab,
};

/// Interpolation family used by an [`InterpolatedTernaryField`].
///
/// Muggianu, Kohler, and RawBarycentric are cubic-alpha interior continuation
/// policies selected through [`CubicAlphaBuildOptions`], not separate field
/// interpolation families.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum FieldInterpolation {
    /// Piecewise-affine interpolation in each elementary triangle.
    #[default]
    Linear,
    /// Shared-edge cubic-alpha interpolation; requires the `cubic-alpha` feature.
    CubicAlpha(CubicAlphaBuildOptions),
}

/// Value, global gradient, and selected triangle for one field evaluation.
///
/// `gradient_ab` contains derivatives with respect to the independent semantic
/// coordinates `(a, b)`, where `c = 1 - a - b`. Linear gradients are constant
/// inside an elementary triangle; cubic-alpha gradients vary locally. Both
/// models are C0 but are not guaranteed C1 across elementary-triangle edges.
/// An edge or vertex uses the gradient of its deterministically owned triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldSample {
    /// Interpolated scalar value.
    pub value: f64,
    /// Derivatives with respect to global semantic `(a, b)` coordinates.
    pub gradient_ab: [f64; 2],
    /// Containing elementary triangle and its barycentric point location.
    pub location: LocatedTriangle,
}

impl FieldSample {
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

/// Failure while preparing or evaluating an interpolated scalar field.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FieldEvaluationError {
    /// Point normalization or grid location failed.
    PointLocation(PointLocationError),
    /// A supplied location belongs to a grid with a different subdivision count.
    IncompatibleLocation {
        location_subdivisions: usize,
        field_subdivisions: usize,
    },
    /// A manually corrupted or otherwise invalid location was supplied.
    InvalidLocation { triangle: usize },
    /// Finite input samples produced a non-finite interpolated value or gradient.
    NonFiniteEvaluation,
    /// Cubic-alpha evaluation was selected without the `cubic-alpha` feature.
    CubicFeatureUnavailable,
    /// Construction of the shared cubic-alpha field model failed.
    CubicConstruction(FieldError),
    /// The allocation-free batch output slice has the wrong size.
    OutputSizeMismatch { expected: usize, actual: usize },
}

impl fmt::Display for FieldEvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PointLocation(error) => write!(formatter, "point location failed: {error}"),
            Self::IncompatibleLocation {
                location_subdivisions,
                field_subdivisions,
            } => write!(
                formatter,
                "location uses {location_subdivisions} subdivisions but field uses {field_subdivisions}"
            ),
            Self::InvalidLocation { triangle } => {
                write!(
                    formatter,
                    "location does not describe canonical triangle {triangle}"
                )
            }
            Self::NonFiniteEvaluation => {
                write!(
                    formatter,
                    "interpolation produced a non-finite value or gradient"
                )
            }
            Self::CubicFeatureUnavailable => write!(
                formatter,
                "cubic-alpha field evaluation requires the `cubic-alpha` feature"
            ),
            Self::CubicConstruction(error) => {
                write!(formatter, "cubic-alpha field construction failed: {error}")
            }
            Self::OutputSizeMismatch { expected, actual } => write!(
                formatter,
                "batch output requires {expected} values; received {actual}"
            ),
        }
    }
}

impl std::error::Error for FieldEvaluationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PointLocation(error) => Some(error),
            Self::CubicConstruction(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PointLocationError> for FieldEvaluationError {
    fn from(value: PointLocationError) -> Self {
        Self::PointLocation(value)
    }
}

enum PreparedInterpolation<'a> {
    Linear,
    CubicAlpha(CubicGridField<'a>),
}

/// A scalar field prepared once for repeated pointwise evaluation.
///
/// Constructing the cubic-alpha variant derives all one-dimensional intervals
/// and local triangle models once. Subsequent evaluations only locate a point
/// and evaluate its selected cached local model.
pub struct InterpolatedTernaryField<'a> {
    field: &'a RegularTernaryScalarField,
    interpolation: PreparedInterpolation<'a>,
}

impl<'a> InterpolatedTernaryField<'a> {
    /// Prepare one scalar interpolation model for repeated evaluation.
    pub fn new(
        field: &'a RegularTernaryScalarField,
        interpolation: FieldInterpolation,
    ) -> Result<Self, FieldEvaluationError> {
        let interpolation = match interpolation {
            FieldInterpolation::Linear => PreparedInterpolation::Linear,
            FieldInterpolation::CubicAlpha(options) => {
                let model = CubicGridField::new(field, options).map_err(|error| match error {
                    FieldError::CubicFeatureUnavailable => Self::cubic_feature_unavailable(),
                    error => FieldEvaluationError::CubicConstruction(error),
                })?;
                PreparedInterpolation::CubicAlpha(model)
            }
        };
        Ok(Self {
            field,
            interpolation,
        })
    }

    fn cubic_feature_unavailable() -> FieldEvaluationError {
        FieldEvaluationError::CubicFeatureUnavailable
    }

    /// Return the sampled scalar field used by this prepared evaluator.
    pub const fn field(&self) -> &'a RegularTernaryScalarField {
        self.field
    }

    /// Evaluate only the scalar value at one semantic composition.
    pub fn value(&self, composition: [f64; 3]) -> Result<f64, FieldEvaluationError> {
        Ok(self.evaluate(composition)?.value)
    }

    /// Evaluate the scalar value, analytic global gradient, and location.
    pub fn evaluate(&self, composition: [f64; 3]) -> Result<FieldSample, FieldEvaluationError> {
        let location = self.field.grid().locate(composition)?;
        self.evaluate_at_location(&location)
    }

    /// Evaluate only the scalar value at a previously located composition.
    pub fn value_at_location(
        &self,
        location: &LocatedTriangle,
    ) -> Result<f64, FieldEvaluationError> {
        Ok(self.evaluate_at_location(location)?.value)
    }

    /// Evaluate the scalar value and analytic global gradient at a previous
    /// location without repeating composition normalization or point location.
    pub fn evaluate_at_location(
        &self,
        location: &LocatedTriangle,
    ) -> Result<FieldSample, FieldEvaluationError> {
        let triangle = self.validated_triangle(location)?;
        let local_gradient = match &self.interpolation {
            PreparedInterpolation::Linear => {
                let values = triangle.vertices.map(|id| self.field.values()[id.0]);
                [values[0] - values[2], values[1] - values[2]]
            }
            PreparedInterpolation::CubicAlpha(model) => model
                .gradient_in_triangle(
                    triangle.id,
                    location.barycentric[0],
                    location.barycentric[1],
                )
                .ok_or(FieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
        };
        let value = match &self.interpolation {
            PreparedInterpolation::Linear => triangle
                .vertices
                .into_iter()
                .zip(location.barycentric)
                .map(|(id, weight)| self.field.values()[id.0] * weight)
                .sum(),
            PreparedInterpolation::CubicAlpha(model) => model
                .value_in_triangle(triangle.id, location.barycentric)
                .ok_or(FieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
        };
        let gradient_ab = global_gradient(self.field.grid(), triangle, local_gradient)?;
        if !value.is_finite() || !gradient_ab.into_iter().all(f64::is_finite) {
            return Err(FieldEvaluationError::NonFiniteEvaluation);
        }
        Ok(FieldSample {
            value,
            gradient_ab,
            location: *location,
        })
    }

    /// Evaluate one known triangle-local barycentric position without point location.
    ///
    /// This crate-private hook supports deterministic one-sided metric probes.
    pub(crate) fn evaluate_in_triangle(
        &self,
        triangle: GridTriangle,
        barycentric: [f64; 3],
    ) -> Result<(f64, [f64; 2]), FieldEvaluationError> {
        if self.field.grid().triangle(triangle.id).ok() != Some(triangle)
            || !valid_barycentric(barycentric)
        {
            return Err(FieldEvaluationError::InvalidLocation {
                triangle: triangle.id,
            });
        }
        let local_gradient = match &self.interpolation {
            PreparedInterpolation::Linear => {
                let values = triangle.vertices.map(|id| self.field.values()[id.0]);
                [values[0] - values[2], values[1] - values[2]]
            }
            PreparedInterpolation::CubicAlpha(model) => model
                .gradient_in_triangle(triangle.id, barycentric[0], barycentric[1])
                .ok_or(FieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
        };
        let value = match &self.interpolation {
            PreparedInterpolation::Linear => triangle
                .vertices
                .into_iter()
                .zip(barycentric)
                .map(|(id, weight)| self.field.values()[id.0] * weight)
                .sum(),
            PreparedInterpolation::CubicAlpha(model) => model
                .value_in_triangle(triangle.id, barycentric)
                .ok_or(FieldEvaluationError::InvalidLocation {
                    triangle: triangle.id,
                })?,
        };
        let gradient = global_gradient(self.field.grid(), triangle, local_gradient)?;
        if !value.is_finite() || !gradient.into_iter().all(f64::is_finite) {
            return Err(FieldEvaluationError::NonFiniteEvaluation);
        }
        Ok((value, gradient))
    }

    /// Lazily evaluate a batch without rebuilding the prepared model.
    pub fn values<'b, I>(
        &'b self,
        compositions: I,
    ) -> impl Iterator<Item = Result<f64, FieldEvaluationError>> + 'b
    where
        I: IntoIterator<Item = [f64; 3]>,
        I::IntoIter: 'b,
    {
        compositions
            .into_iter()
            .map(|composition| self.value(composition))
    }

    /// Evaluate into a caller-owned output slice without allocation.
    ///
    /// On an evaluation failure, output elements before the failing composition
    /// have been written and later elements are left unchanged.
    pub fn values_into(
        &self,
        compositions: &[[f64; 3]],
        output: &mut [f64],
    ) -> Result<(), FieldEvaluationError> {
        if output.len() != compositions.len() {
            return Err(FieldEvaluationError::OutputSizeMismatch {
                expected: compositions.len(),
                actual: output.len(),
            });
        }
        for (composition, value) in compositions.iter().copied().zip(output) {
            *value = self.value(composition)?;
        }
        Ok(())
    }

    /// Return cubic construction diagnostics, or `None` for linear evaluation.
    pub fn cubic_diagnostics(&self) -> Option<&CubicBuildDiagnostics> {
        match &self.interpolation {
            PreparedInterpolation::Linear => None,
            PreparedInterpolation::CubicAlpha(model) => Some(model.diagnostics()),
        }
    }

    fn validated_triangle(
        &self,
        location: &LocatedTriangle,
    ) -> Result<GridTriangle, FieldEvaluationError> {
        if location.subdivisions() != self.field.subdivisions() {
            return Err(FieldEvaluationError::IncompatibleLocation {
                location_subdivisions: location.subdivisions(),
                field_subdivisions: self.field.subdivisions(),
            });
        }
        let triangle = self
            .field
            .grid()
            .triangle(location.triangle.id)
            .map_err(|_| FieldEvaluationError::InvalidLocation {
                triangle: location.triangle.id,
            })?;
        if triangle != location.triangle || !valid_barycentric(location.barycentric) {
            return Err(FieldEvaluationError::InvalidLocation {
                triangle: location.triangle.id,
            });
        }
        Ok(triangle)
    }
}

/// Prepared regular interpolation for fields with locally unavailable vertices.
///
/// A triangle containing an unavailable corner evaluates to `None`; complete
/// triangles use cubic-alpha, one-sided cubic, or linear fallback models chosen
/// during construction. No unavailable value enters a spline helper.
#[cfg(feature = "cubic-alpha")]
pub struct InterpolatedPartialTernaryField {
    field: RegularTernaryPartialScalarField,
    interpolation: PartialPreparedInterpolation,
}

#[cfg(feature = "cubic-alpha")]
enum PartialPreparedInterpolation {
    Linear,
    Cubic(crate::field::PartialCubicGridField),
}

#[cfg(feature = "cubic-alpha")]
impl InterpolatedPartialTernaryField {
    pub fn new(
        field: RegularTernaryPartialScalarField,
        interpolation: FieldInterpolation,
    ) -> Result<Self, FieldEvaluationError> {
        let interpolation = match interpolation {
            FieldInterpolation::Linear => PartialPreparedInterpolation::Linear,
            FieldInterpolation::CubicAlpha(options) => PartialPreparedInterpolation::Cubic(
                crate::field::PartialCubicGridField::new(field.clone(), options)
                    .map_err(FieldEvaluationError::CubicConstruction)?,
            ),
        };
        Ok(Self {
            field,
            interpolation,
        })
    }

    pub const fn field(&self) -> &RegularTernaryPartialScalarField {
        &self.field
    }

    pub fn evaluate(
        &self,
        composition: [f64; 3],
    ) -> Result<Option<FieldSample>, FieldEvaluationError> {
        let location = self.field.grid().locate(composition)?;
        self.evaluate_at_location(&location)
    }

    pub fn evaluate_at_location(
        &self,
        location: &LocatedTriangle,
    ) -> Result<Option<FieldSample>, FieldEvaluationError> {
        let triangle = self.validated_triangle(location)?;
        let values = triangle.vertices.map(|vertex| {
            self.field
                .value(vertex)
                .expect("triangle vertices are valid")
        });
        if values.iter().any(Option::is_none) {
            return Ok(None);
        }
        let values = values.map(|value| value.expect("defined partial triangle"));
        let (value, local_gradient) = match &self.interpolation {
            PartialPreparedInterpolation::Linear => (
                values
                    .into_iter()
                    .zip(location.barycentric)
                    .map(|(value, weight)| value * weight)
                    .sum(),
                [values[0] - values[2], values[1] - values[2]],
            ),
            PartialPreparedInterpolation::Cubic(model) => {
                let value = model
                    .value_in_triangle(triangle.id, location.barycentric)
                    .flatten()
                    .ok_or(FieldEvaluationError::InvalidLocation {
                        triangle: triangle.id,
                    })?;
                let gradient = model
                    .gradient_in_triangle(
                        triangle.id,
                        location.barycentric[0],
                        location.barycentric[1],
                    )
                    .flatten()
                    .ok_or(FieldEvaluationError::InvalidLocation {
                        triangle: triangle.id,
                    })?;
                (value, gradient)
            }
        };
        let gradient_ab = global_gradient(self.field.grid(), triangle, local_gradient)?;
        if !value.is_finite() || !gradient_ab.into_iter().all(f64::is_finite) {
            return Err(FieldEvaluationError::NonFiniteEvaluation);
        }
        Ok(Some(FieldSample {
            value,
            gradient_ab,
            location: *location,
        }))
    }

    pub fn value_at_location(
        &self,
        location: &LocatedTriangle,
    ) -> Result<Option<f64>, FieldEvaluationError> {
        Ok(self
            .evaluate_at_location(location)?
            .map(|sample| sample.value))
    }

    pub fn cubic_diagnostics(&self) -> Option<&CubicBuildDiagnostics> {
        match &self.interpolation {
            PartialPreparedInterpolation::Linear => None,
            PartialPreparedInterpolation::Cubic(model) => Some(model.diagnostics()),
        }
    }

    fn validated_triangle(
        &self,
        location: &LocatedTriangle,
    ) -> Result<GridTriangle, FieldEvaluationError> {
        if location.subdivisions() != self.field.subdivisions() {
            return Err(FieldEvaluationError::IncompatibleLocation {
                location_subdivisions: location.subdivisions(),
                field_subdivisions: self.field.subdivisions(),
            });
        }
        let triangle = self
            .field
            .grid()
            .triangle(location.triangle.id)
            .map_err(|_| FieldEvaluationError::InvalidLocation {
                triangle: location.triangle.id,
            })?;
        if triangle != location.triangle || !valid_barycentric(location.barycentric) {
            return Err(FieldEvaluationError::InvalidLocation {
                triangle: location.triangle.id,
            });
        }
        Ok(triangle)
    }
}

fn valid_barycentric(barycentric: [f64; 3]) -> bool {
    let sum = barycentric.into_iter().sum::<f64>();
    barycentric
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.0)
        && (sum - 1.0).abs() <= crate::POINT_LOCATION_TOLERANCE
}

fn global_gradient(
    grid: RegularTernaryGrid,
    triangle: GridTriangle,
    local: [f64; 2],
) -> Result<[f64; 2], FieldEvaluationError> {
    let vertex = |index: usize| {
        grid.composition(triangle.vertices[index]).map_err(|_| {
            FieldEvaluationError::InvalidLocation {
                triangle: triangle.id,
            }
        })
    };
    let v0 = vertex(0)?;
    let v1 = vertex(1)?;
    let v2 = vertex(2)?;
    global_gradient_ab([v0, v1, v2], local).ok_or(FieldEvaluationError::NonFiniteEvaluation)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "cubic-alpha")]
    use std::collections::BTreeMap;

    use super::*;
    use crate::PointBoundaryLocation;
    #[cfg(feature = "cubic-alpha")]
    use crate::{
        BinaryExtrapolation, CubicAlphaMethod, CubicBoundaryPolicy, GridVertexId,
        RegularTernaryGrid,
    };

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() < 2.0e-10, "{left:?} != {right:?}");
    }

    fn affine_field(subdivisions: usize) -> RegularTernaryScalarField {
        RegularTernaryScalarField::from_fn(subdivisions, |[a, b, c]| {
            2.25 * a - 3.5 * b + 0.75 * c + 1.125
        })
        .unwrap()
    }

    #[test]
    fn linear_evaluation_reproduces_affine_values_and_global_gradients() {
        let mut state = 0x51ca_1a7e_u64;
        let next = |state: &mut u64| {
            *state = state
                .wrapping_mul(2_862_933_555_777_941_757)
                .wrapping_add(3_037_000_493);
            (*state >> 11) as f64 / ((u64::MAX >> 11) as f64)
        };
        for subdivisions in [1, 2, 7, 31] {
            let field = affine_field(subdivisions);
            let evaluator =
                InterpolatedTernaryField::new(&field, FieldInterpolation::Linear).unwrap();
            for (id, point) in field.grid().indexed_compositions() {
                close(evaluator.value(point).unwrap(), field.value(id).unwrap());
            }
            for _ in 0..101 {
                let a = next(&mut state);
                let b = (1.0 - a) * next(&mut state);
                let point = [a, b, 1.0 - a - b];
                let sample = evaluator.evaluate(point).unwrap();
                close(sample.value, 2.25 * a - 3.5 * b + 0.75 * point[2] + 1.125);
                close(sample.gradient_ab[0], 1.5);
                close(sample.gradient_ab[1], -4.25);
            }
        }
    }

    #[test]
    fn prepared_batch_evaluation_preserves_order_and_reports_errors() {
        let field = affine_field(6);
        let evaluator = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear).unwrap();
        let points = [[0.2, 0.3, 0.5], [0.7, 0.1, 0.2], [0.0, 1.0, 0.0]];
        let scalar = points.map(|point| evaluator.value(point).unwrap());
        assert_eq!(
            evaluator
                .values(points)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            scalar
        );
        let mut output = [f64::NAN; 3];
        evaluator.values_into(&points, &mut output).unwrap();
        assert_eq!(output, scalar);
        assert!(evaluator.values(Vec::new()).next().is_none());
        assert!(matches!(
            evaluator.values_into(&points, &mut output[..2]),
            Err(FieldEvaluationError::OutputSizeMismatch {
                expected: 3,
                actual: 2
            })
        ));
        let invalid = [[0.2, 0.3, 0.5], [-0.1, 0.4, 0.7], [0.0, 0.0, 1.0]];
        let mut partial = [f64::NAN; 3];
        assert!(matches!(
            evaluator.values_into(&invalid, &mut partial),
            Err(FieldEvaluationError::PointLocation(_))
        ));
        close(partial[0], scalar[0]);
        assert!(partial[1].is_nan() && partial[2].is_nan());
    }

    #[test]
    fn locations_are_reusable_but_reject_incompatible_grids() {
        let first = affine_field(5);
        let second = affine_field(6);
        let first_evaluator =
            InterpolatedTernaryField::new(&first, FieldInterpolation::Linear).unwrap();
        let second_evaluator =
            InterpolatedTernaryField::new(&second, FieldInterpolation::Linear).unwrap();
        let location = first.grid().locate([0.2, 0.3, 0.5]).unwrap();
        assert_eq!(
            first_evaluator.value_at_location(&location),
            first_evaluator.value([0.2, 0.3, 0.5])
        );
        assert!(matches!(
            second_evaluator.value_at_location(&location),
            Err(FieldEvaluationError::IncompatibleLocation { .. })
        ));
        assert!(matches!(
            first.grid().locate([0.0, 0.5, 0.5]).unwrap().boundary,
            PointBoundaryLocation::Vertex | PointBoundaryLocation::Edge
        ));
    }

    #[cfg(not(feature = "cubic-alpha"))]
    #[test]
    fn cubic_alpha_reports_a_feature_error_without_optional_dependency() {
        let field = affine_field(2);
        assert!(matches!(
            InterpolatedTernaryField::new(
                &field,
                FieldInterpolation::CubicAlpha(CubicAlphaBuildOptions::default())
            ),
            Err(FieldEvaluationError::CubicFeatureUnavailable)
        ));
    }

    #[cfg(feature = "cubic-alpha")]
    fn barycentric(grid: &RegularTernaryGrid, triangle: GridTriangle, point: [f64; 3]) -> [f64; 3] {
        let vertices = triangle.vertices.map(|id| grid.composition(id).unwrap());
        let determinant = (vertices[0][0] - vertices[2][0]) * (vertices[1][1] - vertices[2][1])
            - (vertices[1][0] - vertices[2][0]) * (vertices[0][1] - vertices[2][1]);
        let pa = point[0] - vertices[2][0];
        let pb = point[1] - vertices[2][1];
        let u = (pa * (vertices[1][1] - vertices[2][1]) - (vertices[1][0] - vertices[2][0]) * pb)
            / determinant;
        let v = ((vertices[0][0] - vertices[2][0]) * pb - pa * (vertices[0][1] - vertices[2][1]))
            / determinant;
        [u, v, 1.0 - u - v]
    }

    #[cfg(feature = "cubic-alpha")]
    #[test]
    fn cubic_alpha_preparation_is_cached_and_reproduces_vertices_and_shared_edges() {
        let field = RegularTernaryScalarField::from_fn(6, |[a, b, c]| {
            a.powi(3) - 0.4 * b.powi(2) + 0.8 * c + a * b
        })
        .unwrap();
        for method in [
            CubicAlphaMethod::Akima,
            CubicAlphaMethod::Makima,
            CubicAlphaMethod::Pchip,
            CubicAlphaMethod::Steffen,
        ] {
            for extrapolation in [
                BinaryExtrapolation::RawBarycentric,
                BinaryExtrapolation::Muggianu,
                BinaryExtrapolation::Kohler,
            ] {
                let options = CubicAlphaBuildOptions {
                    method,
                    boundary_policy: CubicBoundaryPolicy::LinearFallback,
                    partial_domain_policy: Default::default(),
                    extrapolation,
                };
                let evaluator =
                    InterpolatedTernaryField::new(&field, FieldInterpolation::CubicAlpha(options))
                        .unwrap();
                assert!(evaluator.cubic_diagnostics().is_some());
                for (id, point) in field.grid().indexed_compositions() {
                    close(evaluator.value(point).unwrap(), field.value(id).unwrap());
                }
                let repeated = evaluator.value([0.23, 0.31, 0.46]).unwrap();
                close(repeated, evaluator.value([0.23, 0.31, 0.46]).unwrap());

                let core = CubicGridField::new(&field, options).unwrap();
                let mut owners: BTreeMap<(GridVertexId, GridVertexId), Vec<GridTriangle>> =
                    BTreeMap::new();
                for triangle in field.grid().elementary_triangles().unwrap() {
                    for [left, right] in [[0, 1], [1, 2], [2, 0]] {
                        let a = triangle.vertices[left];
                        let b = triangle.vertices[right];
                        owners
                            .entry(if a < b { (a, b) } else { (b, a) })
                            .or_default()
                            .push(triangle);
                    }
                }
                for ((left, right), triangles) in owners {
                    if triangles.len() != 2 {
                        continue;
                    }
                    let a = field.grid().composition(left).unwrap();
                    let b = field.grid().composition(right).unwrap();
                    for fraction in [0.17, 0.5, 0.83] {
                        let point = [
                            a[0] * (1.0 - fraction) + b[0] * fraction,
                            a[1] * (1.0 - fraction) + b[1] * fraction,
                            a[2] * (1.0 - fraction) + b[2] * fraction,
                        ];
                        let lhs = core
                            .value_in_triangle(
                                triangles[0].id,
                                barycentric(&field.grid(), triangles[0], point),
                            )
                            .unwrap();
                        let rhs = core
                            .value_in_triangle(
                                triangles[1].id,
                                barycentric(&field.grid(), triangles[1], point),
                            )
                            .unwrap();
                        close(lhs, rhs);
                        close(evaluator.value(point).unwrap(), lhs);
                    }
                }
            }
        }
    }

    #[cfg(feature = "cubic-alpha")]
    #[test]
    fn cubic_alpha_boundary_error_is_reported_during_preparation() {
        let field = affine_field(1);
        let options = CubicAlphaBuildOptions {
            boundary_policy: CubicBoundaryPolicy::Error,
            ..CubicAlphaBuildOptions::default()
        };
        assert!(matches!(
            InterpolatedTernaryField::new(&field, FieldInterpolation::CubicAlpha(options)),
            Err(FieldEvaluationError::CubicConstruction(
                FieldError::InsufficientStencil { samples: 2 }
            ))
        ));
    }

    /// Manual timing smoke test; run with `--features cubic-alpha -- --ignored --nocapture`.
    #[cfg(feature = "cubic-alpha")]
    #[test]
    #[ignore = "manual timing comparison, not a correctness test"]
    fn benchmark_prepared_linear_and_cubic_evaluation() {
        let field = RegularTernaryScalarField::from_fn(96, |[a, b, c]| {
            a.powi(3) - 0.4 * b.powi(2) + 0.8 * c + a * b
        })
        .unwrap();
        let linear = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear).unwrap();
        let cubic = InterpolatedTernaryField::new(
            &field,
            FieldInterpolation::CubicAlpha(CubicAlphaBuildOptions::default()),
        )
        .unwrap();
        let points = (0..20_000)
            .map(|index| {
                let a = (index as f64 + 0.23) / 20_001.0;
                let b = (1.0 - a) * (((index * 37) % 20_000) as f64 + 0.61) / 20_000.0;
                [a, b, 1.0 - a - b]
            })
            .collect::<Vec<_>>();

        let start = std::time::Instant::now();
        let linear_sum = linear
            .values(points.iter().copied())
            .sum::<Result<f64, _>>()
            .unwrap();
        let linear_elapsed = start.elapsed();
        let start = std::time::Instant::now();
        let cubic_sum = cubic
            .values(points.iter().copied())
            .sum::<Result<f64, _>>()
            .unwrap();
        let cubic_elapsed = start.elapsed();
        assert!(linear_sum.is_finite() && cubic_sum.is_finite());
        eprintln!(
            "n=96, points={}: prepared linear={linear_elapsed:?}, prepared cubic={cubic_elapsed:?}",
            points.len(),
        );
    }
}
