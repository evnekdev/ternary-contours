//! Field-independent ternary-coordinate normalization and local transforms.
//!
//! The helpers in this module deliberately operate only on semantic A/B/C
//! coordinates and triangle-local barycentric weights. Grid ownership remains
//! the responsibility of the regular or irregular point locators.

use std::fmt;

/// Tolerance for deciding whether a coordinate triplet is already normalized.
///
/// User-facing coordinate entry uses this stricter tolerance than point
/// location. It is intentionally small enough that a successful normalize
/// action is explicit rather than silently hidden by formatting.
pub const COORDINATE_NORMALIZATION_TOLERANCE: f64 = 1.0e-12;

/// Failure while validating or normalizing a ternary coordinate triplet.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum CoordinateTransformError {
    /// A component was NaN or infinite.
    NonFinite { component: usize, value: f64 },
    /// A component was negative. Negative continuation is intentionally not
    /// accepted by the Viewer coordinate-entry workflow.
    Negative { component: usize, value: f64 },
    /// The finite non-negative triplet had no positive total.
    NonPositiveSum { sum: f64 },
}

impl fmt::Display for CoordinateTransformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { component, value } => write!(
                formatter,
                "coordinate component {component} must be finite; received {value:?}"
            ),
            Self::Negative { component, value } => write!(
                formatter,
                "coordinate component {component} must be non-negative; received {value:?}"
            ),
            Self::NonPositiveSum { sum } => write!(
                formatter,
                "coordinates must have a positive finite sum; received {sum:?}"
            ),
        }
    }
}

impl std::error::Error for CoordinateTransformError {}

/// Validate a finite non-negative triplet and normalize it to sum exactly one.
///
/// This does not locate a point or choose a triangle owner. Call a grid's
/// existing deterministic locator after normalizing a global composition.
pub fn normalize_ternary_triplet(
    coordinates: [f64; 3],
) -> Result<[f64; 3], CoordinateTransformError> {
    for (component, value) in coordinates.into_iter().enumerate() {
        if !value.is_finite() {
            return Err(CoordinateTransformError::NonFinite { component, value });
        }
        if value < 0.0 {
            return Err(CoordinateTransformError::Negative { component, value });
        }
    }
    let sum = coordinates.into_iter().sum::<f64>();
    if !sum.is_finite() || sum <= 0.0 {
        return Err(CoordinateTransformError::NonPositiveSum { sum });
    }
    Ok(coordinates.map(|value| value / sum))
}

/// Convert normalized-or-normalizable triangle-local barycentric weights into
/// a semantic global A/B/C composition.
///
/// `vertices` and the returned composition retain the local vertex ordering;
/// callers must use the same order as their triangle locator.
pub fn composition_from_local_barycentric(
    vertices: [[f64; 3]; 3],
    local: [f64; 3],
) -> Result<[f64; 3], CoordinateTransformError> {
    let local = normalize_ternary_triplet(local)?;
    let mut composition = [0.0; 3];
    for (weight, vertex) in local.into_iter().zip(vertices) {
        for component in 0..3 {
            composition[component] += weight * vertex[component];
        }
    }
    normalize_ternary_triplet(composition)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() <= 1.0e-12, "{left} != {right}");
    }

    #[test]
    fn normalizes_positive_triplets_and_rejects_invalid_input() {
        assert_eq!(
            normalize_ternary_triplet([2.0, 3.0, 5.0]).unwrap(),
            [0.2, 0.3, 0.5]
        );
        assert!(matches!(
            normalize_ternary_triplet([0.0, 0.0, 0.0]),
            Err(CoordinateTransformError::NonPositiveSum { .. })
        ));
        assert!(matches!(
            normalize_ternary_triplet([-1.0, 1.0, 1.0]),
            Err(CoordinateTransformError::Negative { component: 0, .. })
        ));
        assert!(matches!(
            normalize_ternary_triplet([f64::NAN, 1.0, 1.0]),
            Err(CoordinateTransformError::NonFinite { component: 0, .. })
        ));
    }

    #[test]
    fn local_barycentric_transform_preserves_vertex_order() {
        let composition = composition_from_local_barycentric(
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [2.0, 3.0, 5.0],
        )
        .unwrap();
        close(composition[0], 0.2);
        close(composition[1], 0.3);
        close(composition[2], 0.5);
    }
}
