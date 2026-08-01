use super::StableContourError;

/// Practical source-to-sampling-grid verification and global refinement controls.
///
/// Verification samples are a resolution check, not a proof that no smaller
/// feature exists between the configured points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StableGridVerification {
    /// Enable midpoint/centroid comparison against original source evaluators.
    pub enabled: bool,
    /// Maximum number of deterministic global refinement passes.
    pub maximum_refinement_passes: usize,
    /// Hard upper bound on regular sampling-grid subdivisions.
    pub maximum_subdivisions: usize,
    /// Verify all three edge midpoints in every sampling-grid triangle.
    pub verify_edge_midpoints: bool,
    /// Verify every sampling-grid triangle centroid.
    pub verify_centroids: bool,
    /// Maximum accepted absolute source-height approximation error.
    pub height_error_tolerance: f64,
    /// Maximum accepted absolute secondary approximation error.
    pub secondary_error_tolerance: f64,
    /// Tolerance used when comparing predicted and direct stable phase sets.
    pub ownership_tolerance: f64,
    /// Permit a result after the resolution limit with unresolved triangles.
    pub allow_unresolved: bool,
}

impl Default for StableGridVerification {
    fn default() -> Self {
        Self {
            enabled: false,
            maximum_refinement_passes: 3,
            maximum_subdivisions: 256,
            verify_edge_midpoints: true,
            verify_centroids: true,
            height_error_tolerance: 1.0e-6,
            secondary_error_tolerance: 1.0e-6,
            ownership_tolerance: 1.0e-9,
            allow_unresolved: false,
        }
    }
}

/// Numerical controls for the common virtual regular sampling-grid representation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StableGridOptions {
    /// Initial subdivision count of the regular sampling grid.
    pub subdivisions: usize,
    /// Scalar equality and requested-level tolerance.
    pub value_tolerance: f64,
    /// Height upper-envelope and phase-tie tolerance.
    pub stability_tolerance: f64,
    /// Point canonicalisation and zero-length geometry tolerance.
    pub geometry_tolerance: f64,
    /// Strict local event and directed traversal progress tolerance.
    pub parameter_tolerance: f64,
    /// Optional source approximation verification and global refinement.
    pub verification: StableGridVerification,
}

impl Default for StableGridOptions {
    fn default() -> Self {
        Self {
            subdivisions: 24,
            value_tolerance: 1.0e-10,
            stability_tolerance: 1.0e-10,
            geometry_tolerance: 1.0e-10,
            parameter_tolerance: 1.0e-12,
            verification: StableGridVerification::default(),
        }
    }
}

impl StableGridOptions {
    pub(crate) fn validate(self) -> Result<(), StableContourError> {
        if self.subdivisions == 0 {
            return Err(StableContourError::invalid_option(
                "subdivisions must be positive",
            ));
        }
        for (name, value) in [
            ("value_tolerance", self.value_tolerance),
            ("stability_tolerance", self.stability_tolerance),
            ("geometry_tolerance", self.geometry_tolerance),
            ("parameter_tolerance", self.parameter_tolerance),
            (
                "verification.height_error_tolerance",
                self.verification.height_error_tolerance,
            ),
            (
                "verification.secondary_error_tolerance",
                self.verification.secondary_error_tolerance,
            ),
            (
                "verification.ownership_tolerance",
                self.verification.ownership_tolerance,
            ),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(StableContourError::invalid_option(format!(
                    "{name} must be finite and nonnegative"
                )));
            }
        }
        if self.geometry_tolerance == 0.0 || self.parameter_tolerance == 0.0 {
            return Err(StableContourError::invalid_option(
                "geometry_tolerance and parameter_tolerance must be positive",
            ));
        }
        if self.verification.maximum_subdivisions < self.subdivisions {
            return Err(StableContourError::invalid_option(
                "verification.maximum_subdivisions is below the initial subdivisions",
            ));
        }
        if self.verification.enabled
            && !self.verification.verify_edge_midpoints
            && !self.verification.verify_centroids
        {
            return Err(StableContourError::invalid_option(
                "verification requires centroid or edge-midpoint samples",
            ));
        }
        Ok(())
    }
}
