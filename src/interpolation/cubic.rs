use std::fmt;

use super::AlphaInterval;

const KOHLER_DENOMINATOR_GUARD: f64 = 64.0 * f64::EPSILON;

/// Geometric extension of a directed binary interaction into a ternary interior.
///
/// Every policy retains the raw multicomponent prefactor `xi*xj`; only the
/// directed interval parameter changes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum BinaryExtrapolation {
    /// Experimental, non-recommended direct extension `t=xj`.
    ///
    /// This is neither linear interpolation nor conventional Muggianu. Its
    /// ternary-interior value depends on the canonical edge direction; use
    /// [`Self::Muggianu`] or [`Self::Kohler`] for stable applications.
    RawBarycentric,
    /// Assign half the remaining component to each member: `t=xj+xk/2`.
    #[default]
    Muggianu,
    /// Normalize within the binary pair: `t=xj/(xi+xj)`.
    Kohler,
}

/// Value and unconstrained barycentric partial derivatives of one pair term.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairEvaluation {
    pub value: f64,
    /// Derivatives with respect to `[xi, xj, xk]` before the simplex constraint.
    pub derivatives: [f64; 3],
    pub parameter: f64,
}

/// A directed alpha interval attached to two local triangle vertices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectedAlphaInterval {
    pub start: usize,
    pub end: usize,
    pub interval: AlphaInterval,
}

impl DirectedAlphaInterval {
    pub fn new(
        start: usize,
        end: usize,
        interval: AlphaInterval,
    ) -> Result<Self, InterpolationError> {
        if start >= 3 || end >= 3 || start == end {
            return Err(InterpolationError::InvalidDirectedEdge { start, end });
        }
        if !interval.alpha0.is_finite() || !interval.alpha1.is_finite() {
            return Err(InterpolationError::NonFiniteAlpha {
                alpha0: interval.alpha0,
                alpha1: interval.alpha1,
            });
        }
        Ok(Self {
            start,
            end,
            interval,
        })
    }

    pub const fn reversed(self) -> Self {
        Self {
            start: self.end,
            end: self.start,
            interval: self.interval.reversed(),
        }
    }
}

/// One local cubic-alpha field on a ternary elementary triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct CubicAlphaTriangle {
    vertex_values: [f64; 3],
    edge_intervals: [DirectedAlphaInterval; 3],
    extrapolation: BinaryExtrapolation,
}

impl CubicAlphaTriangle {
    pub fn new(
        vertex_values: [f64; 3],
        edge_intervals: [DirectedAlphaInterval; 3],
        extrapolation: BinaryExtrapolation,
    ) -> Result<Self, InterpolationError> {
        if let Some((index, value)) = vertex_values
            .into_iter()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(InterpolationError::NonFiniteVertexValue { index, value });
        }
        let mut pairs = [false; 3];
        for edge in edge_intervals {
            if edge.start >= 3 || edge.end >= 3 || edge.start == edge.end {
                return Err(InterpolationError::InvalidDirectedEdge {
                    start: edge.start,
                    end: edge.end,
                });
            }
            if !edge.interval.alpha0.is_finite() || !edge.interval.alpha1.is_finite() {
                return Err(InterpolationError::NonFiniteAlpha {
                    alpha0: edge.interval.alpha0,
                    alpha1: edge.interval.alpha1,
                });
            }
            let pair = pair_index(edge.start, edge.end);
            if pairs[pair] {
                return Err(InterpolationError::DuplicateEdgePair {
                    start: edge.start,
                    end: edge.end,
                });
            }
            pairs[pair] = true;
        }
        if pairs.iter().any(|present| !present) {
            return Err(InterpolationError::MissingEdgePair);
        }
        Ok(Self {
            vertex_values,
            edge_intervals,
            extrapolation,
        })
    }

    pub const fn vertex_values(&self) -> [f64; 3] {
        self.vertex_values
    }

    pub const fn edge_intervals(&self) -> &[DirectedAlphaInterval; 3] {
        &self.edge_intervals
    }

    pub const fn extrapolation(&self) -> BinaryExtrapolation {
        self.extrapolation
    }

    /// Evaluate the complete local field at barycentric coordinates.
    pub fn value(&self, barycentric: [f64; 3]) -> f64 {
        let linear = self
            .vertex_values
            .into_iter()
            .zip(barycentric)
            .map(|(value, weight)| value * weight)
            .sum::<f64>();
        linear
            + self
                .edge_intervals
                .iter()
                .map(|edge| self.evaluate_edge(*edge, barycentric).value)
                .sum::<f64>()
    }

    /// Analytic reduced gradient for `x0=u`, `x1=v`, `x2=1-u-v`.
    pub fn gradient_reduced(&self, u: f64, v: f64) -> [f64; 2] {
        let barycentric = [u, v, 1.0 - u - v];
        let mut partial = self.vertex_values;
        for edge in self.edge_intervals {
            let evaluation = self.evaluate_edge(edge, barycentric);
            let remaining = remaining_index(edge.start, edge.end);
            partial[edge.start] += evaluation.derivatives[0];
            partial[edge.end] += evaluation.derivatives[1];
            partial[remaining] += evaluation.derivatives[2];
        }
        [partial[0] - partial[2], partial[1] - partial[2]]
    }

    fn evaluate_edge(&self, edge: DirectedAlphaInterval, barycentric: [f64; 3]) -> PairEvaluation {
        let remaining = remaining_index(edge.start, edge.end);
        evaluate_pair(
            barycentric[edge.start],
            barycentric[edge.end],
            barycentric[remaining],
            edge.interval,
            self.extrapolation,
        )
    }
}

/// Centralized binary pair contribution and policy-specific analytic derivative.
///
/// Inputs and derivatives are ordered as directed source `xi`, destination
/// `xj`, and remaining component `xk`.
pub fn evaluate_pair(
    xi: f64,
    xj: f64,
    xk: f64,
    interval: AlphaInterval,
    extrapolation: BinaryExtrapolation,
) -> PairEvaluation {
    let prefactor = xi * xj;
    let (parameter, parameter_derivatives) = match extrapolation {
        BinaryExtrapolation::RawBarycentric => (xj, [0.0, 1.0, 0.0]),
        BinaryExtrapolation::Muggianu => (xj + 0.5 * xk, [0.0, 1.0, 0.5]),
        BinaryExtrapolation::Kohler => {
            let denominator = xi + xj;
            if denominator.abs() <= KOHLER_DENOMINATOR_GUARD {
                return PairEvaluation {
                    value: 0.0,
                    derivatives: [0.0; 3],
                    parameter: 0.5,
                };
            }
            let inverse_square = 1.0 / (denominator * denominator);
            (
                xj / denominator,
                [-xj * inverse_square, xi * inverse_square, 0.0],
            )
        }
    };
    let polynomial = interval.alpha0 + interval.alpha1 * parameter;
    let prefactor_derivatives = [xj, xi, 0.0];
    let mut derivatives = [0.0; 3];
    for index in 0..3 {
        derivatives[index] = prefactor_derivatives[index] * polynomial
            + prefactor * interval.alpha1 * parameter_derivatives[index];
    }
    PairEvaluation {
        value: prefactor * polynomial,
        derivatives,
        parameter,
    }
}

const fn pair_index(left: usize, right: usize) -> usize {
    let (low, high) = if left < right {
        (left, right)
    } else {
        (right, left)
    };
    match (low, high) {
        (0, 1) => 0,
        (1, 2) => 1,
        (0, 2) => 2,
        _ => usize::MAX,
    }
}

const fn remaining_index(left: usize, right: usize) -> usize {
    3 - left - right
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum InterpolationError {
    InvalidDirectedEdge { start: usize, end: usize },
    DuplicateEdgePair { start: usize, end: usize },
    MissingEdgePair,
    NonFiniteAlpha { alpha0: f64, alpha1: f64 },
    NonFiniteVertexValue { index: usize, value: f64 },
}

impl fmt::Display for InterpolationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDirectedEdge { start, end } => {
                write!(formatter, "invalid local directed edge {start}->{end}")
            }
            Self::DuplicateEdgePair { start, end } => {
                write!(formatter, "duplicate local edge pair {start}-{end}")
            }
            Self::MissingEdgePair => {
                write!(formatter, "local cubic triangle is missing an edge pair")
            }
            Self::NonFiniteAlpha { alpha0, alpha1 } => {
                write!(
                    formatter,
                    "alpha coefficients must be finite: ({alpha0:?}, {alpha1:?})"
                )
            }
            Self::NonFiniteVertexValue { index, value } => {
                write!(
                    formatter,
                    "triangle vertex value {index} is not finite: {value:?}"
                )
            }
        }
    }
}

impl std::error::Error for InterpolationError {}

#[cfg(test)]
mod tests {
    use super::*;

    const ALPHA: AlphaInterval = AlphaInterval::new(1.7, -2.3);

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() < 2.0e-9, "{left} != {right}");
    }

    #[test]
    fn policy_parameters_and_raw_prefactor_are_exact() {
        let (xi, xj, xk) = (0.2, 0.3, 0.5);
        let raw = evaluate_pair(xi, xj, xk, ALPHA, BinaryExtrapolation::RawBarycentric);
        let muggianu = evaluate_pair(xi, xj, xk, ALPHA, BinaryExtrapolation::Muggianu);
        let kohler = evaluate_pair(xi, xj, xk, ALPHA, BinaryExtrapolation::Kohler);
        close(raw.parameter, xj);
        close(muggianu.parameter, xj + xk / 2.0);
        close(kohler.parameter, xj / (xi + xj));
        for evaluation in [raw, muggianu, kohler] {
            close(
                evaluation.value,
                xi * xj * (ALPHA.alpha0 + ALPHA.alpha1 * evaluation.parameter),
            );
        }
    }

    #[test]
    fn all_policies_are_exact_on_binary_edges_and_for_constant_alpha() {
        for t in [0.0, 0.13, 0.5, 0.87, 1.0] {
            for policy in [
                BinaryExtrapolation::RawBarycentric,
                BinaryExtrapolation::Muggianu,
                BinaryExtrapolation::Kohler,
            ] {
                let pair = evaluate_pair(1.0 - t, t, 0.0, ALPHA, policy);
                close(pair.parameter, t);
                close(
                    pair.value,
                    (1.0 - t) * t * (ALPHA.alpha0 + ALPHA.alpha1 * t),
                );
            }
        }
        let constant = AlphaInterval::new(1.7, 0.0);
        for policy in [
            BinaryExtrapolation::RawBarycentric,
            BinaryExtrapolation::Muggianu,
            BinaryExtrapolation::Kohler,
        ] {
            close(
                evaluate_pair(0.2, 0.3, 0.5, constant, policy).value,
                1.7 * 0.2 * 0.3,
            );
        }
    }

    #[test]
    fn muggianu_and_kohler_reverse_interior_but_raw_is_directional() {
        let (xi, xj, xk) = (0.17, 0.31, 0.52);
        for policy in [BinaryExtrapolation::Muggianu, BinaryExtrapolation::Kohler] {
            let forward = evaluate_pair(xi, xj, xk, ALPHA, policy);
            let reverse = evaluate_pair(xj, xi, xk, ALPHA.reversed(), policy);
            close(reverse.parameter, 1.0 - forward.parameter);
            close(forward.value, reverse.value);
        }
        let forward = evaluate_pair(xi, xj, xk, ALPHA, BinaryExtrapolation::RawBarycentric);
        let reverse = evaluate_pair(
            xj,
            xi,
            xk,
            ALPHA.reversed(),
            BinaryExtrapolation::RawBarycentric,
        );
        assert!((forward.value - reverse.value).abs() > 1.0e-4);
    }

    #[test]
    fn kohler_has_quadratic_dilution_and_finite_third_vertex_limit() {
        let ratio = 0.35;
        let full = evaluate_pair(1.0 - ratio, ratio, 0.0, ALPHA, BinaryExtrapolation::Kohler);
        for dilution in [0.8, 0.2, 1.0e-8] {
            let diluted = evaluate_pair(
                dilution * (1.0 - ratio),
                dilution * ratio,
                1.0 - dilution,
                ALPHA,
                BinaryExtrapolation::Kohler,
            );
            close(diluted.value, full.value * dilution * dilution);
            assert!(diluted.derivatives.into_iter().all(f64::is_finite));
        }
        let corner = evaluate_pair(0.0, 0.0, 1.0, ALPHA, BinaryExtrapolation::Kohler);
        assert_eq!(corner.value, 0.0);
        assert!(corner.derivatives.into_iter().all(f64::is_finite));
    }

    #[test]
    fn analytic_pair_derivatives_match_centered_differences() {
        let point = [0.21, 0.34, 0.45];
        let h = 1.0e-7;
        for policy in [
            BinaryExtrapolation::RawBarycentric,
            BinaryExtrapolation::Muggianu,
            BinaryExtrapolation::Kohler,
        ] {
            let actual = evaluate_pair(point[0], point[1], point[2], ALPHA, policy);
            for index in 0..3 {
                let mut plus = point;
                let mut minus = point;
                plus[index] += h;
                minus[index] -= h;
                let plus_value = evaluate_pair(plus[0], plus[1], plus[2], ALPHA, policy).value;
                let minus_value = evaluate_pair(minus[0], minus[1], minus[2], ALPHA, policy).value;
                close(
                    actual.derivatives[index],
                    (plus_value - minus_value) / (2.0 * h),
                );
            }
        }
    }

    fn triangle(policy: BinaryExtrapolation) -> CubicAlphaTriangle {
        CubicAlphaTriangle::new(
            [2.0, -0.5, 1.25],
            [
                DirectedAlphaInterval::new(0, 1, ALPHA).unwrap(),
                DirectedAlphaInterval::new(1, 2, AlphaInterval::new(-0.7, 1.1)).unwrap(),
                DirectedAlphaInterval::new(0, 2, AlphaInterval::new(0.4, -0.9)).unwrap(),
            ],
            policy,
        )
        .unwrap()
    }

    #[test]
    fn local_field_reproduces_vertices_edges_and_zero_alpha_linear_limit() {
        for policy in [
            BinaryExtrapolation::RawBarycentric,
            BinaryExtrapolation::Muggianu,
            BinaryExtrapolation::Kohler,
        ] {
            let model = triangle(policy);
            assert_eq!(model.value([1.0, 0.0, 0.0]), 2.0);
            assert_eq!(model.value([0.0, 1.0, 0.0]), -0.5);
            assert_eq!(model.value([0.0, 0.0, 1.0]), 1.25);
            for t in [0.0, 0.2, 0.7, 1.0] {
                close(model.value([1.0 - t, t, 0.0]), ALPHA.value(2.0, -0.5, t));
            }
        }
        let zero = AlphaInterval::default();
        let model = CubicAlphaTriangle::new(
            [2.0, -0.5, 1.25],
            [
                DirectedAlphaInterval::new(0, 1, zero).unwrap(),
                DirectedAlphaInterval::new(1, 2, zero).unwrap(),
                DirectedAlphaInterval::new(0, 2, zero).unwrap(),
            ],
            BinaryExtrapolation::Muggianu,
        )
        .unwrap();
        let x = [0.2, 0.3, 0.5];
        close(model.value(x), 2.0 * x[0] - 0.5 * x[1] + 1.25 * x[2]);
    }

    #[test]
    fn constant_alpha_reproduces_pairwise_quadratic_regular_solution() {
        let model = CubicAlphaTriangle::new(
            [1.2, -0.4, 2.1],
            [
                DirectedAlphaInterval::new(0, 1, AlphaInterval::new(0.8, 0.0)).unwrap(),
                DirectedAlphaInterval::new(1, 2, AlphaInterval::new(-1.1, 0.0)).unwrap(),
                DirectedAlphaInterval::new(0, 2, AlphaInterval::new(0.35, 0.0)).unwrap(),
            ],
            BinaryExtrapolation::Kohler,
        )
        .unwrap();
        for x in [[0.2, 0.3, 0.5], [0.01, 0.49, 0.5], [0.6, 0.2, 0.2]] {
            let expected = 1.2 * x[0] - 0.4 * x[1] + 2.1 * x[2] + 0.8 * x[0] * x[1]
                - 1.1 * x[1] * x[2]
                + 0.35 * x[0] * x[2];
            close(model.value(x), expected);
        }
    }

    #[test]
    fn local_vertex_permutation_preserves_canonical_directed_metadata() {
        let original_edges = [
            DirectedAlphaInterval::new(0, 1, ALPHA).unwrap(),
            DirectedAlphaInterval::new(1, 2, AlphaInterval::new(-0.7, 1.1)).unwrap(),
            DirectedAlphaInterval::new(0, 2, AlphaInterval::new(0.4, -0.9)).unwrap(),
        ];
        let old_to_new = [1, 2, 0];
        let original_x = [0.2, 0.3, 0.5];
        let permuted_x = [original_x[2], original_x[0], original_x[1]];
        for policy in [
            BinaryExtrapolation::RawBarycentric,
            BinaryExtrapolation::Muggianu,
            BinaryExtrapolation::Kohler,
        ] {
            let original =
                CubicAlphaTriangle::new([2.0, -0.5, 1.25], original_edges, policy).unwrap();
            let transformed = original_edges.map(|edge| {
                DirectedAlphaInterval::new(
                    old_to_new[edge.start],
                    old_to_new[edge.end],
                    edge.interval,
                )
                .unwrap()
            });
            let permuted = CubicAlphaTriangle::new([1.25, 2.0, -0.5], transformed, policy).unwrap();
            close(original.value(original_x), permuted.value(permuted_x));
        }
    }

    #[test]
    fn local_analytic_gradient_matches_finite_differences_for_all_policies() {
        let h = 1.0e-7;
        for policy in [
            BinaryExtrapolation::RawBarycentric,
            BinaryExtrapolation::Muggianu,
            BinaryExtrapolation::Kohler,
        ] {
            let model = triangle(policy);
            for [u, v] in [[0.23, 0.31], [0.49, 0.49], [1.0e-7, 1.0e-7]] {
                let gradient = model.gradient_reduced(u, v);
                let du = (model.value([u + h, v, 1.0 - u - h - v])
                    - model.value([u - h, v, 1.0 - u + h - v]))
                    / (2.0 * h);
                let dv = (model.value([u, v + h, 1.0 - u - v - h])
                    - model.value([u, v - h, 1.0 - u - v + h]))
                    / (2.0 * h);
                assert!((gradient[0] - du).abs() < 5.0e-7);
                assert!((gradient[1] - dv).abs() < 5.0e-7);
            }
        }
    }
}
