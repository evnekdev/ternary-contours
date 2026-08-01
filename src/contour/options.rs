use crate::interpolation::BinaryExtrapolation;

use super::ContourError;

pub use crate::interpolation::{CubicAlphaMethod, CubicBoundaryPolicy};

/// Bounds for adaptive cubic topology extraction in barycentric coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptiveContourOptions {
    /// Scalar equality tolerance used while classifying a level.
    pub value_tolerance: f64,
    /// Composition-space endpoint cleanup tolerance.
    pub geometry_tolerance: f64,
    /// Maximum recursive microtriangle depth, in `1..=10`.
    pub max_depth: u8,
    /// Maximum sampled field disagreement allowed before further refinement.
    pub flatness_tolerance: f64,
}
impl Default for AdaptiveContourOptions {
    fn default() -> Self {
        Self {
            value_tolerance: 1.0e-10,
            geometry_tolerance: 1.0e-7,
            max_depth: 5,
            flatness_tolerance: 1.0e-5,
        }
    }
}
impl AdaptiveContourOptions {
    pub(crate) fn validate(self) -> Result<(), ContourError> {
        if !self.value_tolerance.is_finite()
            || self.value_tolerance <= 0.0
            || !self.geometry_tolerance.is_finite()
            || self.geometry_tolerance <= 0.0
        {
            return Err(ContourError::InvalidTolerance {
                value_tolerance: self.value_tolerance,
                geometry_tolerance: self.geometry_tolerance,
            });
        }
        if self.max_depth == 0
            || self.max_depth > 10
            || !self.flatness_tolerance.is_finite()
            || self.flatness_tolerance <= 0.0
        {
            return Err(ContourError::InvalidAdaptiveOptions {
                max_depth: self.max_depth,
                flatness_tolerance: self.flatness_tolerance,
            });
        }
        Ok(())
    }
}

/// Backward-compatible contour name for shared path regularization controls.
pub use crate::PathRegularizationOptions as ContourRegularization;

/// Cubic-alpha construction policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicAlphaOptions {
    /// The `spline1d` method used on each regular lattice line.
    pub method: CubicAlphaMethod,
    /// Policy for two-sample boundary lines.
    pub boundary_policy: CubicBoundaryPolicy,
    /// Interior continuation of the shared directed edge intervals.
    pub extrapolation: BinaryExtrapolation,
    /// Bounded adaptive topology options.
    pub adaptive: AdaptiveContourOptions,
    /// Optional equal-arclength regularization after topology extraction.
    pub regularization: Option<ContourRegularization>,
}
impl Default for CubicAlphaOptions {
    fn default() -> Self {
        Self {
            method: CubicAlphaMethod::Steffen,
            boundary_policy: CubicBoundaryPolicy::LinearFallback,
            extrapolation: BinaryExtrapolation::Muggianu,
            adaptive: AdaptiveContourOptions::default(),
            regularization: Some(ContourRegularization::default()),
        }
    }
}

/// Scalar interpolation model used for contour construction.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum ContourInterpolation {
    /// Exact piecewise-affine interpolation within each elementary triangle.
    Linear,
    /// Edge-derived cubic-alpha interpolation; requires the `cubic-alpha` feature.
    CubicAlpha(CubicAlphaOptions),
}

/// Shared options for one [`super::ContourSet`] computation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourOptions {
    /// Linear or cubic-alpha scalar model.
    pub interpolation: ContourInterpolation,
    /// Finite positive scalar equality tolerance.
    pub value_tolerance: f64,
    /// Finite positive composition-space cleanup tolerance.
    pub geometry_tolerance: f64,
    /// Optional regularization after path extraction.
    pub regularization: Option<ContourRegularization>,
}
impl ContourOptions {
    /// Construct the always-available piecewise-linear baseline.
    pub const fn linear() -> Self {
        Self {
            interpolation: ContourInterpolation::Linear,
            value_tolerance: 1.0e-10,
            geometry_tolerance: 1.0e-8,
            regularization: None,
        }
    }
    /// Construct cubic-alpha options from one cubic configuration.
    pub const fn cubic_alpha(options: CubicAlphaOptions) -> Self {
        Self {
            interpolation: ContourInterpolation::CubicAlpha(options),
            value_tolerance: options.adaptive.value_tolerance,
            geometry_tolerance: options.adaptive.geometry_tolerance,
            regularization: options.regularization,
        }
    }
    /// Replace the post-extraction regularization policy.
    pub const fn regularization(mut self, options: Option<ContourRegularization>) -> Self {
        self.regularization = options;
        self
    }
    pub(crate) fn validate(self) -> Result<(), ContourError> {
        if !self.value_tolerance.is_finite()
            || self.value_tolerance <= 0.0
            || !self.geometry_tolerance.is_finite()
            || self.geometry_tolerance <= 0.0
        {
            return Err(ContourError::InvalidTolerance {
                value_tolerance: self.value_tolerance,
                geometry_tolerance: self.geometry_tolerance,
            });
        }
        if let ContourInterpolation::CubicAlpha(options) = self.interpolation {
            options.adaptive.validate()?;
        }
        if let Some(regularization) = self.regularization {
            match crate::path::validate_regularization(regularization) {
                Ok(()) => {}
                Err(crate::path::PathProcessingError::InvalidSpacing { spacing }) => {
                    return Err(ContourError::InvalidRegularizationSpacing { spacing });
                }
                Err(_) => {
                    return Err(ContourError::InvalidProjectionOptions {
                        tolerance: regularization.projection_tolerance,
                        iterations: regularization.max_projection_iterations,
                        max_step: regularization.max_normal_step,
                    });
                }
            }
        }
        Ok(())
    }
}
impl Default for ContourOptions {
    fn default() -> Self {
        Self::linear()
    }
}

/// Diagnostics from cubic field preparation, refinement, and projection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CubicContourDiagnostics {
    /// Number of regular-grid edges assigned a cubic alpha interval.
    pub cubic_edges: usize,
    /// Number of two-sample edges using the configured linear fallback.
    pub linear_fallback_edges: usize,
    /// Number of recursively refined microtriangles.
    pub refined_triangles: usize,
    /// Number of cells that reached the configured depth while still non-flat.
    pub maximum_depth_hits: usize,
    /// Number of failed projection attempts observed before returning an error.
    pub projection_failures: usize,
}
