//! Deterministic analytical scalar fields used by numerical examples.
//!
//! These helpers are deliberately small and explicit. They describe synthetic
//! liquidus-like fields for reproducible examples and regression tests; they do
//! not claim thermodynamic meaning or provide an optimisation framework.

use crate::{FieldError, RegularTernaryScalarField, StablePhaseId, TernaryCoordinate};

const EQUILATERAL_HEIGHT: f64 = 0.866_025_403_784_438_6;

/// A deterministic descending liquidus-like scalar field specification.
///
/// The field is evaluated in the canonical equilateral logical plane:
///
/// ```text
/// T(x) = maximum - dᵀ quadratic d - quartic |d|⁴
/// ```
///
/// where `d` is the logical displacement from [`Self::centre`]. The matrix is
/// expected to be symmetric positive semidefinite for a concave-down field,
/// but the constructor leaves that policy to the caller so deliberately
/// asymmetric validation cases can also be generated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidusFieldSpec {
    /// Stable phase identifier associated with the sampled field.
    pub phase: StablePhaseId,
    /// Congruent or maximum-temperature composition.
    pub centre: TernaryCoordinate,
    /// Field value at the centre.
    pub maximum: f64,
    /// Directional quadratic steepness in logical `(x, y)` coordinates.
    pub quadratic: [[f64; 2]; 2],
    /// Nonnegative quartic sharpening coefficient.
    pub quartic: f64,
}

impl LiquidusFieldSpec {
    /// Construct an explicit synthetic field specification.
    pub const fn new(
        phase: StablePhaseId,
        centre: TernaryCoordinate,
        maximum: f64,
        quadratic: [[f64; 2]; 2],
        quartic: f64,
    ) -> Self {
        Self {
            phase,
            centre,
            maximum,
            quadratic,
            quartic,
        }
    }

    /// Construct an isotropic field with optional quartic sharpening.
    pub const fn isotropic(
        phase: StablePhaseId,
        centre: TernaryCoordinate,
        maximum: f64,
        steepness: f64,
        quartic: f64,
    ) -> Self {
        Self::new(
            phase,
            centre,
            maximum,
            [[steepness, 0.0], [0.0, steepness]],
            quartic,
        )
    }

    /// Construct a field centred at pure component A.
    pub const fn corner_a(phase: StablePhaseId, maximum: f64, steepness: f64) -> Self {
        Self::isotropic(
            phase,
            TernaryCoordinate::new(1.0, 0.0, 0.0),
            maximum,
            steepness,
            0.0,
        )
    }

    /// Construct a field centred at pure component B.
    pub const fn corner_b(phase: StablePhaseId, maximum: f64, steepness: f64) -> Self {
        Self::isotropic(
            phase,
            TernaryCoordinate::new(0.0, 1.0, 0.0),
            maximum,
            steepness,
            0.0,
        )
    }

    /// Construct a field centred at pure component C.
    pub const fn corner_c(phase: StablePhaseId, maximum: f64, steepness: f64) -> Self {
        Self::isotropic(
            phase,
            TernaryCoordinate::new(0.0, 0.0, 1.0),
            maximum,
            steepness,
            0.0,
        )
    }

    /// Construct a field centred at a fraction along edge A-B.
    pub const fn edge_ab(
        phase: StablePhaseId,
        fraction_b: f64,
        maximum: f64,
        steepness: f64,
    ) -> Self {
        Self::isotropic(
            phase,
            TernaryCoordinate::new(1.0 - fraction_b, fraction_b, 0.0),
            maximum,
            steepness,
            0.0,
        )
    }

    /// Construct a field centred at a fraction along edge A-C.
    pub const fn edge_ac(
        phase: StablePhaseId,
        fraction_c: f64,
        maximum: f64,
        steepness: f64,
    ) -> Self {
        Self::isotropic(
            phase,
            TernaryCoordinate::new(1.0 - fraction_c, 0.0, fraction_c),
            maximum,
            steepness,
            0.0,
        )
    }

    /// Construct a field centred at a fraction along edge B-C.
    pub const fn edge_bc(
        phase: StablePhaseId,
        fraction_c: f64,
        maximum: f64,
        steepness: f64,
    ) -> Self {
        Self::isotropic(
            phase,
            TernaryCoordinate::new(0.0, 1.0 - fraction_c, fraction_c),
            maximum,
            steepness,
            0.0,
        )
    }

    /// Evaluate the analytical field at a semantic composition.
    pub fn value(&self, composition: [f64; 3]) -> f64 {
        let centre = logical_from_composition(self.centre.as_array());
        let point = logical_from_composition(composition);
        let dx = point[0] - centre[0];
        let dy = point[1] - centre[1];
        let radius_squared = dx * dx + dy * dy;
        let quadratic = self.quadratic[0][0] * dx * dx
            + (self.quadratic[0][1] + self.quadratic[1][0]) * dx * dy
            + self.quadratic[1][1] * dy * dy;
        self.maximum - quadratic - self.quartic * radius_squared * radius_squared
    }

    /// Sample this specification on a regular scalar field.
    pub fn sample(&self, subdivisions: usize) -> Result<RegularTernaryScalarField, FieldError> {
        let specification = *self;
        RegularTernaryScalarField::from_fn(subdivisions, move |composition| {
            specification.value(composition)
        })
    }
}

fn logical_from_composition([_a, b, c]: [f64; 3]) -> [f64; 2] {
    [b + 0.5 * c, EQUILATERAL_HEIGHT * c]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_place_maxima_at_requested_compositions() {
        let cases = [
            LiquidusFieldSpec::corner_a(StablePhaseId(1), 10.0, 2.0),
            LiquidusFieldSpec::corner_b(StablePhaseId(2), 11.0, 2.0),
            LiquidusFieldSpec::corner_c(StablePhaseId(3), 12.0, 2.0),
            LiquidusFieldSpec::edge_ab(StablePhaseId(4), 0.3, 13.0, 2.0),
            LiquidusFieldSpec::edge_ac(StablePhaseId(5), 0.4, 14.0, 2.0),
            LiquidusFieldSpec::edge_bc(StablePhaseId(6), 0.6, 15.0, 2.0),
        ];
        for case in cases {
            assert_eq!(case.value(case.centre.as_array()), case.maximum);
        }
    }

    #[test]
    fn isotropic_fields_are_component_permutation_symmetric() {
        let field = LiquidusFieldSpec::isotropic(
            StablePhaseId(1),
            TernaryCoordinate::new(1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0),
            100.0,
            4.0,
            0.7,
        );
        let permutations = [[0.2, 0.3, 0.5], [0.3, 0.5, 0.2], [0.5, 0.2, 0.3]];
        let values = permutations.map(|composition| field.value(composition));
        assert!((values[0] - values[1]).abs() < 1.0e-12);
        assert!((values[1] - values[2]).abs() < 1.0e-12);
    }
}
