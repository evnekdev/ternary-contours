//! Shared scalar-field analysis records for regular and irregular fields.

use super::{DistributionError, DistributionSummary, TernaryGradient};

/// Explicit weighting basis for a reported scalar distribution.
///
/// Metric-producing APIs label their aggregation basis instead of silently
/// comparing vertex, area, edge-length, or contour-length populations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum MetricWeighting {
    /// Each contributing entity has one equal weight.
    #[default]
    Unweighted,
    /// Contributions are weighted by logical-plane area.
    Area,
    /// Contributions are weighted by logical-plane edge length.
    EdgeLength,
    /// Contributions are weighted by logical contour length.
    ContourLength,
}

/// Comparable scalar-field distribution summaries.
///
/// Every summary records its associated [`MetricWeighting`]. Empty populations
/// retain the explicit empty semantics of [`DistributionSummary`].
#[derive(Clone, Debug, PartialEq)]
pub struct ScalarFieldDistributionMetrics {
    /// Distribution of sampled vertex values.
    pub sample_values: DistributionSummary,
    /// Distribution of signed endpoint differences over unique edges.
    pub edge_differences: DistributionSummary,
    /// Distribution of signed scalar secants per logical edge length.
    pub edge_secant_slopes: DistributionSummary,
    /// Distribution of analytic prepared-interpolant gradient magnitudes.
    pub gradient_norms: DistributionSummary,
    /// Distribution of local sampled-field Hessian norms where available.
    pub hessian_norms: DistributionSummary,
    /// Distribution of local sampled-field Laplacians where available.
    pub laplacians: DistributionSummary,
    /// Distribution of local sampled-field curvature anisotropies where defined.
    pub curvature_anisotropies: DistributionSummary,
    /// Distribution of one-sided interior-edge gradient-jump magnitudes.
    pub gradient_jump_magnitudes: DistributionSummary,
    /// Aggregation basis for each distribution above.
    pub weighting: MetricWeighting,
    /// Number of locations where an optional local metric could not be evaluated.
    pub unavailable_local_estimate_count: usize,
    /// Number of non-finite evaluations observed while measuring a prepared field.
    pub non_finite_evaluation_count: usize,
}

pub(crate) struct ScalarFieldDistributionInput<'a> {
    pub sample_values: &'a [f64],
    pub edge_differences: &'a [f64],
    pub edge_secant_slopes: &'a [f64],
    pub gradient_norms: &'a [f64],
    pub hessian_norms: &'a [f64],
    pub laplacians: &'a [f64],
    pub curvature_anisotropies: &'a [f64],
    pub gradient_jump_magnitudes: &'a [f64],
    pub unavailable_local_estimate_count: usize,
    pub non_finite_evaluation_count: usize,
}

impl ScalarFieldDistributionMetrics {
    pub(crate) fn from_input(
        input: ScalarFieldDistributionInput<'_>,
    ) -> Result<Self, DistributionError> {
        Ok(Self {
            sample_values: DistributionSummary::from_values(input.sample_values)?,
            edge_differences: DistributionSummary::from_values(input.edge_differences)?,
            edge_secant_slopes: DistributionSummary::from_values(input.edge_secant_slopes)?,
            gradient_norms: DistributionSummary::from_values(input.gradient_norms)?,
            hessian_norms: DistributionSummary::from_values(input.hessian_norms)?,
            laplacians: DistributionSummary::from_values(input.laplacians)?,
            curvature_anisotropies: DistributionSummary::from_values(input.curvature_anisotropies)?,
            gradient_jump_magnitudes: DistributionSummary::from_values(
                input.gradient_jump_magnitudes,
            )?,
            weighting: MetricWeighting::Unweighted,
            unavailable_local_estimate_count: input.unavailable_local_estimate_count,
            non_finite_evaluation_count: input.non_finite_evaluation_count,
        })
    }
}

/// One-sided derivative discontinuity across an interior elementary-triangle edge.
///
/// The field value is shared at `position`; `left_gradient` and
/// `right_gradient` are evaluated from the two explicitly selected triangle
/// interiors. `tangential_jump` and `normal_jump` are signed projections of
/// `right-left` onto the canonical logical edge tangent and its left normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientJump {
    /// Semantic composition at which the one-sided derivatives were sampled.
    pub position: [f64; 3],
    /// Gradient from the triangle whose third vertex lies left of the canonical edge direction.
    pub left_gradient: TernaryGradient,
    /// Gradient from the triangle whose third vertex lies right of the canonical edge direction.
    pub right_gradient: TernaryGradient,
    /// Euclidean logical-plane norm of `right_gradient-left_gradient`.
    pub magnitude: f64,
    /// Signed jump along the canonical logical edge tangent.
    pub tangential_jump: f64,
    /// Signed jump along the left logical edge normal.
    pub normal_jump: f64,
}

impl GradientJump {
    pub(crate) fn from_gradients(
        position: [f64; 3],
        left_gradient: TernaryGradient,
        right_gradient: TernaryGradient,
        tangent: [f64; 2],
    ) -> Option<Self> {
        let left = left_gradient.logical_xy();
        let right = right_gradient.logical_xy();
        let jump = [right[0] - left[0], right[1] - left[1]];
        let tangent_norm = tangent[0].hypot(tangent[1]);
        if !position.into_iter().all(f64::is_finite)
            || !left_gradient.is_finite()
            || !right_gradient.is_finite()
            || !tangent_norm.is_finite()
            || tangent_norm == 0.0
        {
            return None;
        }
        let tangent = [tangent[0] / tangent_norm, tangent[1] / tangent_norm];
        let normal = [-tangent[1], tangent[0]];
        let tangential_jump = jump[0].mul_add(tangent[0], jump[1] * tangent[1]);
        let normal_jump = jump[0].mul_add(normal[0], jump[1] * normal[1]);
        let magnitude = jump[0].hypot(jump[1]);
        (tangential_jump.is_finite() && normal_jump.is_finite() && magnitude.is_finite()).then_some(
            Self {
                position,
                left_gradient,
                right_gradient,
                magnitude,
                tangential_jump,
                normal_jump,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_projections_reconstruct_the_logical_difference() {
        let jump = GradientJump::from_gradients(
            [0.3, 0.3, 0.4],
            TernaryGradient::from_logical_xy([1.0, 2.0]),
            TernaryGradient::from_logical_xy([4.0, 6.0]),
            [1.0, 0.0],
        )
        .unwrap();
        assert!((jump.magnitude - jump.tangential_jump.hypot(jump.normal_jump)).abs() < 1.0e-14);
    }
}
