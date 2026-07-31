use ternary_contours::{ContourOptions, ContourSet, RegularTernaryScalarField};

fn field(n: usize, function: impl Fn(f64, f64, f64) -> f64) -> RegularTernaryScalarField {
    let count = (n + 1) * (n + 2) / 2;
    let blank = RegularTernaryScalarField::new(n, vec![0.0; count]).unwrap();
    let values = (0..count)
        .map(|index| {
            let [a, b, c] = blank.composition_at(index).unwrap();
            function(a, b, c)
        })
        .collect();
    RegularTernaryScalarField::new(n, values).unwrap()
}

fn max_residual(set: &ContourSet, function: impl Fn(f64, f64, f64) -> f64) -> f64 {
    let mut maximum = 0.0_f64;
    for level in &set.levels {
        for path in &level.paths {
            for point in &path.points {
                let [a, b, c] = point.as_array();
                maximum = maximum.max((function(a, b, c) - level.value).abs());
            }
        }
    }
    maximum
}

#[test]
fn analytic_linear_plane_is_exact_open_and_deterministic() {
    let plane = |a: f64, b: f64, c: f64| 2.0 * a - 3.0 * b + 5.0 * c;
    let field = field(11, plane);
    let first = ContourSet::compute(&field, &[-1.0, 0.5, 2.5], ContourOptions::linear()).unwrap();
    let second = ContourSet::compute(&field, &[-1.0, 0.5, 2.5], ContourOptions::linear()).unwrap();
    assert_eq!(first, second);
    assert!(
        first
            .levels
            .iter()
            .all(|level| level.paths.iter().all(|path| !path.closed))
    );
    assert!(max_residual(&first, plane) < 1.0e-10);
}

#[test]
fn smooth_closed_loop_and_saddle_have_expected_path_classes() {
    let loop_function = |a: f64, b: f64, c: f64| {
        (a - 1.0 / 3.0).powi(2) + (b - 1.0 / 3.0).powi(2) + (c - 1.0 / 3.0).powi(2)
    };
    let loop_set =
        ContourSet::compute(&field(18, loop_function), &[0.06], ContourOptions::linear()).unwrap();
    assert_eq!(loop_set.levels[0].paths.len(), 1);
    assert!(loop_set.levels[0].paths[0].closed);
    let loop_residual = max_residual(&loop_set, loop_function);
    assert!(loop_residual < 0.003, "loop residual={loop_residual}");

    let saddle = |a: f64, b: f64, _c: f64| (a - 0.27) * (b - 0.49);
    let saddle_set =
        ContourSet::compute(&field(17, saddle), &[0.0], ContourOptions::linear()).unwrap();
    assert!(saddle_set.levels[0].paths.len() >= 2);
    assert!(saddle_set.levels[0].paths.iter().all(|path| !path.closed));
    let saddle_residual = max_residual(&saddle_set, saddle);
    assert!(saddle_residual < 0.002, "saddle residual={saddle_residual}");
}

#[test]
fn near_tangent_levels_remain_finite_without_zero_length_paths() {
    let function =
        |a: f64, b: f64, c: f64| (a - 0.31).powi(2) + (b - 0.34).powi(2) + (c - 0.35).powi(2);
    let set =
        ContourSet::compute(&field(21, function), &[0.0012], ContourOptions::linear()).unwrap();
    for path in &set.levels[0].paths {
        assert!(path.points.len() >= 2);
        assert!(
            path.points
                .iter()
                .all(|point| point.as_array().into_iter().all(f64::is_finite))
        );
    }
}

#[cfg(feature = "cubic-alpha")]
mod cubic {
    use super::*;
    use ternary_contours::interpolation::{
        AlphaInterval, CubicAlphaTriangle, DirectedAlphaInterval, evaluate_pair,
    };
    use ternary_contours::{
        BinaryExtrapolation, ContourInterpolation, CubicAlphaMethod, CubicAlphaOptions,
    };

    const INTERVAL: AlphaInterval = AlphaInterval::new(1.7, -0.9);

    #[test]
    fn stable_binary_policies_match_on_edges_differ_inside_and_reverse() {
        let y0 = 1.25;
        let y1 = -0.75;
        for t in [0.0, 0.13, 0.49, 0.81, 1.0] {
            let expected = INTERVAL.value(y0, y1, t);
            for policy in [BinaryExtrapolation::Muggianu, BinaryExtrapolation::Kohler] {
                let model = CubicAlphaTriangle::new(
                    [y0, y1, 0.2],
                    [
                        DirectedAlphaInterval::new(0, 1, INTERVAL).unwrap(),
                        DirectedAlphaInterval::new(1, 2, AlphaInterval::default()).unwrap(),
                        DirectedAlphaInterval::new(0, 2, AlphaInterval::default()).unwrap(),
                    ],
                    policy,
                )
                .unwrap();
                assert!((model.value([1.0 - t, t, 0.0]) - expected).abs() < 1.0e-12);
            }
        }
        let muggianu = evaluate_pair(0.2, 0.3, 0.5, INTERVAL, BinaryExtrapolation::Muggianu);
        let kohler = evaluate_pair(0.2, 0.3, 0.5, INTERVAL, BinaryExtrapolation::Kohler);
        assert_ne!(muggianu.value, kohler.value);
        for policy in [BinaryExtrapolation::Muggianu, BinaryExtrapolation::Kohler] {
            let forward = evaluate_pair(0.2, 0.3, 0.5, INTERVAL, policy);
            let reverse = evaluate_pair(0.3, 0.2, 0.5, INTERVAL.reversed(), policy);
            assert!((forward.value - reverse.value).abs() < 1.0e-12);
        }
    }

    #[test]
    fn constant_alpha_reduces_to_the_same_regular_solution_for_stable_policies() {
        let interval = AlphaInterval::new(2.4, 0.0);
        let point = [0.2, 0.3, 0.5];
        let expected = 2.4 * point[0] * point[1];
        for policy in [BinaryExtrapolation::Muggianu, BinaryExtrapolation::Kohler] {
            assert!(
                (evaluate_pair(point[0], point[1], point[2], interval, policy).value - expected)
                    .abs()
                    < 1.0e-12
            );
        }
    }

    #[test]
    fn bounded_adaptive_diagnostics_are_deterministic() {
        let nonlinear = field(8, |a, b, c| a.powi(3) - 0.6 * b.powi(2) + 0.4 * c + a * b);
        let options = CubicAlphaOptions {
            method: CubicAlphaMethod::Makima,
            adaptive: ternary_contours::AdaptiveContourOptions {
                max_depth: 2,
                flatness_tolerance: 1.0e-12,
                ..Default::default()
            },
            regularization: None,
            ..Default::default()
        };
        let compute = || {
            ContourSet::compute(
                &nonlinear,
                &[0.15],
                ContourOptions {
                    interpolation: ContourInterpolation::CubicAlpha(options),
                    regularization: None,
                    ..ContourOptions::linear()
                },
            )
            .unwrap()
        };
        let first = compute();
        let second = compute();
        assert_eq!(first, second);
        assert!(first.diagnostics().unwrap().maximum_depth_hits > 0);
    }
}
