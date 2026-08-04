#[path = "support/numerical.rs"]
mod numerical;

use numerical::{
    ANALYTIC_TOLERANCE, AnalyticField, ErrorMetrics, FieldMetrics, Lcg,
    approximate_symmetric_hausdorff, assert_within, contour_residual, path_length, spacing_cv,
};
use ternary_contours::{
    ContourOptions, ContourSet, FieldEvaluationError, FieldInterpolation, InterpolatedTernaryField,
    RegularTernaryScalarField,
};

fn regular_field(subdivisions: usize, analytic: AnalyticField) -> RegularTernaryScalarField {
    RegularTernaryScalarField::from_fn(subdivisions, |point| analytic.value(point)).unwrap()
}

fn regular_reconstruction_error(
    field: &RegularTernaryScalarField,
    point: [f64; 3],
) -> ErrorMetrics {
    let location = field.grid().locate(point).unwrap();
    let vertices = location
        .triangle
        .vertices
        .map(|vertex| field.grid().composition(vertex).unwrap());
    let mut metrics = ErrorMetrics::default();
    for component in 0..3 {
        let reconstructed = location
            .barycentric
            .into_iter()
            .zip(vertices)
            .map(|(weight, vertex)| weight * vertex[component])
            .sum();
        metrics.record(reconstructed, point[component]);
    }
    assert_within(
        location.barycentric.into_iter().sum(),
        1.0,
        ANALYTIC_TOLERANCE,
    );
    metrics
}

#[test]
fn regular_linear_affine_catalogue_normalization_and_scale_are_exact() {
    let mut generator = Lcg::new(0x8b7a_19d3_5e21_04cf);
    for subdivisions in [1, 2, 7, 29] {
        let field = regular_field(subdivisions, AnalyticField::Affine);
        let evaluator = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear).unwrap();
        let mut metrics = FieldMetrics::default();
        let mut reconstruction = ErrorMetrics::default();
        for _ in 0..257 {
            let point = generator.simplex();
            let sample = evaluator.evaluate(point).unwrap();
            metrics.record(
                sample.value,
                sample.gradient_ab,
                point,
                AnalyticField::Affine,
            );
            let located = regular_reconstruction_error(&field, point);
            reconstruction.record(located.max_abs(), 0.0);
        }
        assert!(metrics.value.max_abs() <= ANALYTIC_TOLERANCE);
        assert!(metrics.gradient_a.max_abs() <= ANALYTIC_TOLERANCE);
        assert!(metrics.gradient_b.max_abs() <= ANALYTIC_TOLERANCE);
        assert!(reconstruction.max_abs() <= ANALYTIC_TOLERANCE);

        let almost_normalized = [0.21, 0.34, 0.45 + 0.5e-10];
        let sum = almost_normalized.into_iter().sum::<f64>();
        let normalized = almost_normalized.map(|component| component / sum);
        assert_within(
            evaluator.value(almost_normalized).unwrap(),
            evaluator.value(normalized).unwrap(),
            ANALYTIC_TOLERANCE,
        );

        let scale = -7.5e5;
        let offset = 2.0e6;
        let scaled = RegularTernaryScalarField::from_fn(subdivisions, |point| {
            scale * AnalyticField::Affine.value(point) + offset
        })
        .unwrap();
        let scaled_evaluator =
            InterpolatedTernaryField::new(&scaled, FieldInterpolation::Linear).unwrap();
        let point = [0.23, 0.31, 0.46];
        let base = evaluator.evaluate(point).unwrap();
        let transformed = scaled_evaluator.evaluate(point).unwrap();
        assert_within(transformed.value, scale * base.value + offset, 1.0e-7);
        assert_within(
            transformed.gradient_ab[0],
            scale * base.gradient_ab[0],
            1.0e-7,
        );
        assert_within(
            transformed.gradient_ab[1],
            scale * base.gradient_ab[1],
            1.0e-7,
        );
    }
}

#[test]
fn regular_linear_quadratic_error_decreases_under_refinement() {
    let mut maxima = Vec::new();
    let mut gradient_maxima = Vec::new();
    for subdivisions in [7, 14, 28] {
        let field = regular_field(subdivisions, AnalyticField::PairwiseQuadratic);
        let evaluator = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear).unwrap();
        let mut generator = Lcg::new(0x4e55_4d33_5249_4341);
        let mut metrics = FieldMetrics::default();
        for _ in 0..1_001 {
            let point = generator.simplex();
            let sample = evaluator.evaluate(point).unwrap();
            metrics.record(
                sample.value,
                sample.gradient_ab,
                point,
                AnalyticField::PairwiseQuadratic,
            );
        }
        maxima.push(metrics.value.max_abs());
        gradient_maxima.push(
            metrics
                .gradient_a
                .max_abs()
                .max(metrics.gradient_b.max_abs()),
        );
        assert!(metrics.value.rms().is_finite());
    }
    assert!(maxima[1] < maxima[0] * 0.35, "value errors={maxima:?}");
    assert!(maxima[2] < maxima[1] * 0.35, "value errors={maxima:?}");
    assert!(
        gradient_maxima[1] < gradient_maxima[0] * 0.6,
        "gradient errors={gradient_maxima:?}"
    );
    assert!(
        gradient_maxima[2] < gradient_maxima[1] * 0.6,
        "gradient errors={gradient_maxima:?}"
    );
}

#[test]
fn regular_linear_contours_are_byte_stable_and_have_small_residuals() {
    let field = regular_field(19, AnalyticField::Affine);
    let compute =
        || ContourSet::compute(&field, &[-0.75, 0.5, 1.75], ContourOptions::linear()).unwrap();
    let first = compute();
    let second = compute();
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    assert_eq!(first.levels(), first.levels.as_slice());
    assert_eq!(first.levels(), second.levels());
    for level in first.levels() {
        let residual = contour_residual(&level.paths, level.value, |point| {
            AnalyticField::Affine.value(point)
        });
        assert!(
            residual.max_abs() < 2.0e-10,
            "level={} residual={residual:?}",
            level.value
        );
        for path in &level.paths {
            assert!(path_length(path).is_finite() && path_length(path) > 0.0);
            assert!(spacing_cv(path).is_none_or(f64::is_finite));
        }
    }
    assert_eq!(
        approximate_symmetric_hausdorff(&first.levels()[0].paths, &second.levels()[0].paths),
        0.0
    );
}

#[test]
fn regular_evaluation_rejects_non_finite_derived_output() {
    let field = RegularTernaryScalarField::new(1, vec![f64::MAX, 0.0, -f64::MAX]).unwrap();
    let evaluator = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear).unwrap();
    assert!(matches!(
        evaluator.evaluate([0.2, 0.3, 0.5]),
        Err(FieldEvaluationError::NonFiniteEvaluation)
    ));
}

#[cfg(feature = "cubic-alpha")]
mod regular_cubic {
    use std::collections::BTreeMap;

    use super::*;
    use ternary_contours::{
        AdaptiveContourOptions, BinaryExtrapolation, ContourInterpolation, CubicAlphaBuildOptions,
        CubicAlphaMethod, CubicAlphaOptions, CubicBoundaryPolicy, CubicGridField, GridTriangle,
    };

    fn barycentric(
        field: &RegularTernaryScalarField,
        triangle: GridTriangle,
        point: [f64; 3],
    ) -> [f64; 3] {
        let vertices = triangle
            .vertices
            .map(|vertex| field.grid().composition(vertex).unwrap());
        let determinant = (vertices[0][0] - vertices[2][0]) * (vertices[1][1] - vertices[2][1])
            - (vertices[1][0] - vertices[2][0]) * (vertices[0][1] - vertices[2][1]);
        let da = point[0] - vertices[2][0];
        let db = point[1] - vertices[2][1];
        let first = (da * (vertices[1][1] - vertices[2][1])
            - (vertices[1][0] - vertices[2][0]) * db)
            / determinant;
        let second = ((vertices[0][0] - vertices[2][0]) * db
            - da * (vertices[0][1] - vertices[2][1]))
            / determinant;
        [first, second, 1.0 - first - second]
    }

    #[test]
    fn all_regular_cubic_families_are_deterministic_c0_and_have_analytic_gradients() {
        let field = regular_field(7, AnalyticField::PairwiseQuadratic);
        let point = [0.28, 0.31, 0.41];
        let mut difference_between_stable_policies = false;
        for method in [
            CubicAlphaMethod::Akima,
            CubicAlphaMethod::Makima,
            CubicAlphaMethod::Pchip,
            CubicAlphaMethod::Steffen,
        ] {
            let mut values = BTreeMap::new();
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
                for (vertex, composition) in field.grid().indexed_compositions() {
                    assert_within(
                        evaluator.value(composition).unwrap(),
                        field.value(vertex).unwrap(),
                        8.0e-10,
                    );
                }
                let first = evaluator.evaluate(point).unwrap();
                let second = evaluator.evaluate(point).unwrap();
                assert_eq!(first, second);
                let h = 2.0e-7;
                let derivative_a = (evaluator
                    .value([point[0] + h, point[1], point[2] - h])
                    .unwrap()
                    - evaluator
                        .value([point[0] - h, point[1], point[2] + h])
                        .unwrap())
                    / (2.0 * h);
                let derivative_b = (evaluator
                    .value([point[0], point[1] + h, point[2] - h])
                    .unwrap()
                    - evaluator
                        .value([point[0], point[1] - h, point[2] + h])
                        .unwrap())
                    / (2.0 * h);
                assert_within(first.gradient_ab[0], derivative_a, 2.0e-5);
                assert_within(first.gradient_ab[1], derivative_b, 2.0e-5);
                values.insert(format!("{extrapolation:?}"), first.value);

                let model = CubicGridField::new(&field, options).unwrap();
                let mut owners = BTreeMap::new();
                for triangle in field.grid().elementary_triangles().unwrap() {
                    for [left, right] in [[0, 1], [1, 2], [2, 0]] {
                        let first = triangle.vertices[left];
                        let second = triangle.vertices[right];
                        let key = if first < second {
                            (first, second)
                        } else {
                            (second, first)
                        };
                        owners.entry(key).or_insert_with(Vec::new).push(triangle);
                    }
                }
                for ((first, second), triangles) in owners {
                    if triangles.len() != 2 {
                        continue;
                    }
                    let start = field.grid().composition(first).unwrap();
                    let end = field.grid().composition(second).unwrap();
                    let on_edge = [
                        0.5 * (start[0] + end[0]),
                        0.5 * (start[1] + end[1]),
                        0.5 * (start[2] + end[2]),
                    ];
                    let left = model
                        .value_in_triangle(
                            triangles[0].id,
                            barycentric(&field, triangles[0], on_edge),
                        )
                        .unwrap();
                    let right = model
                        .value_in_triangle(
                            triangles[1].id,
                            barycentric(&field, triangles[1], on_edge),
                        )
                        .unwrap();
                    assert_within(left, right, 2.0e-10);
                }
            }
            difference_between_stable_policies |=
                (values["Muggianu"] - values["Kohler"]).abs() > 1.0e-9;
        }
        assert!(difference_between_stable_policies);
    }

    #[test]
    fn regular_cubic_contours_report_bounded_refinement_and_stable_geometry() {
        let field = regular_field(8, AnalyticField::Saddle);
        let cubic = CubicAlphaOptions {
            method: CubicAlphaMethod::Makima,
            adaptive: AdaptiveContourOptions {
                max_depth: 2,
                flatness_tolerance: 1.0e-12,
                ..Default::default()
            },
            regularization: None,
            ..Default::default()
        };
        let options = ContourOptions {
            interpolation: ContourInterpolation::CubicAlpha(cubic),
            regularization: None,
            ..ContourOptions::linear()
        };
        let first = ContourSet::compute(&field, &[0.1], options).unwrap();
        let second = ContourSet::compute(&field, &[0.1], options).unwrap();
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
        assert!(first.diagnostics().unwrap().maximum_depth_hits > 0);
        let evaluator = InterpolatedTernaryField::new(
            &field,
            FieldInterpolation::CubicAlpha(CubicAlphaBuildOptions {
                method: cubic.method,
                boundary_policy: cubic.boundary_policy,
                partial_domain_policy: Default::default(),
                extrapolation: cubic.extrapolation,
            }),
        )
        .unwrap();
        let residual = contour_residual(&first.levels()[0].paths, 0.1, |point| {
            evaluator.value(point).unwrap()
        });
        assert!(residual.max_abs() < 3.0e-2, "residual={residual:?}");
    }
}

#[cfg(feature = "irregular-delaunay")]
mod irregular_linear {
    use super::*;
    use ternary_contours::{
        InterpolatedIrregularTernaryField, IrregularContourOptions, IrregularContourSet,
        IrregularFieldEvaluationError, IrregularFieldInterpolation, IrregularTernaryMesh,
        IrregularTernaryScalarField,
    };

    fn samples() -> Vec<[f64; 3]> {
        vec![
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.72, 0.18, 0.10],
            [0.14, 0.71, 0.15],
            [0.17, 0.21, 0.62],
            [0.51, 0.37, 0.12],
            [0.27, 0.52, 0.21],
            [0.31, 0.16, 0.53],
            [0.36, 0.34, 0.30],
        ]
    }

    fn triangle_keys(mesh: &IrregularTernaryMesh) -> Vec<[[u64; 3]; 3]> {
        let mut keys = mesh
            .triangles()
            .map(|triangle| {
                let mut vertices = mesh
                    .triangle_compositions(triangle.id)
                    .unwrap()
                    .map(|point| point.map(f64::to_bits));
                vertices.sort_unstable();
                vertices
            })
            .collect::<Vec<_>>();
        keys.sort_unstable();
        keys
    }

    #[test]
    fn irregular_linear_is_order_independent_for_nondegenerate_topology_and_exact_for_affine_data()
    {
        let input = samples();
        let first = IrregularTernaryMesh::new(input.clone()).unwrap();
        let second = IrregularTernaryMesh::new(input.clone()).unwrap();
        let mut reversed = input;
        reversed.reverse();
        let reordered = IrregularTernaryMesh::new(reversed).unwrap();
        assert_eq!(triangle_keys(&first), triangle_keys(&second));
        assert_eq!(triangle_keys(&first), triangle_keys(&reordered));

        let irregular_field =
            IrregularTernaryScalarField::from_fn(first, |point| AnalyticField::Affine.value(point))
                .unwrap();
        let irregular = InterpolatedIrregularTernaryField::new(
            &irregular_field,
            IrregularFieldInterpolation::Linear,
        )
        .unwrap();
        let regular_field = regular_field(17, AnalyticField::Affine);
        let regular =
            InterpolatedTernaryField::new(&regular_field, FieldInterpolation::Linear).unwrap();
        let mut generator = Lcg::new(0x1a2b_3c4d_5e6f_7081);
        let mut irregular_metrics = FieldMetrics::default();
        let mut regular_metrics = FieldMetrics::default();
        for _ in 0..401 {
            let point = generator.simplex();
            let irregular_sample = irregular.evaluate(point).unwrap();
            let regular_sample = regular.evaluate(point).unwrap();
            irregular_metrics.record(
                irregular_sample.value,
                irregular_sample.gradient_ab,
                point,
                AnalyticField::Affine,
            );
            regular_metrics.record(
                regular_sample.value,
                regular_sample.gradient_ab,
                point,
                AnalyticField::Affine,
            );
            assert_within(
                irregular_sample.value,
                regular_sample.value,
                ANALYTIC_TOLERANCE,
            );
        }
        assert!(irregular_metrics.value.max_abs() <= ANALYTIC_TOLERANCE);
        assert!(irregular_metrics.gradient_a.max_abs() <= ANALYTIC_TOLERANCE);
        assert!(irregular_metrics.gradient_b.max_abs() <= ANALYTIC_TOLERANCE);
        assert!(regular_metrics.value.max_abs() <= ANALYTIC_TOLERANCE);

        let contours = IrregularContourSet::compute(
            &irregular_field,
            &[-0.75, 0.5, 1.75],
            IrregularContourOptions::linear(),
        )
        .unwrap();
        let repeated = IrregularContourSet::compute(
            &irregular_field,
            &[-0.75, 0.5, 1.75],
            IrregularContourOptions::linear(),
        )
        .unwrap();
        assert_eq!(format!("{contours:?}"), format!("{repeated:?}"));
        assert_eq!(contours.levels(), contours.levels.as_slice());
        for level in contours.levels() {
            let residual = contour_residual(&level.paths, level.value, |point| {
                AnalyticField::Affine.value(point)
            });
            assert!(residual.max_abs() < 2.0e-10, "residual={residual:?}");
            for path in &level.paths {
                assert!(path_length(path).is_finite());
                assert!(spacing_cv(path).is_none_or(f64::is_finite));
            }
        }
        assert_eq!(
            approximate_symmetric_hausdorff(
                &contours.levels()[0].paths,
                &repeated.levels()[0].paths
            ),
            0.0
        );
    }

    #[test]
    fn irregular_linear_handles_a_high_aspect_ratio_cell_with_finite_affine_results() {
        let mesh = IrregularTernaryMesh::new([
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.499_999_99, 0.5, 0.000_000_01],
            [0.51, 0.489_999_99, 0.000_000_01],
        ])
        .unwrap();
        let field =
            IrregularTernaryScalarField::from_fn(mesh, |point| AnalyticField::Affine.value(point))
                .unwrap();
        let evaluator =
            InterpolatedIrregularTernaryField::new(&field, IrregularFieldInterpolation::Linear)
                .unwrap();
        for triangle in field.mesh().triangles() {
            let vertices = field.mesh().triangle_compositions(triangle.id).unwrap();
            let point = vertices.into_iter().fold([0.0; 3], |mut total, vertex| {
                for component in 0..3 {
                    total[component] += vertex[component] / 3.0;
                }
                total
            });
            let sample = evaluator.evaluate(point).unwrap();
            assert!(sample.value.is_finite() && sample.gradient_ab.into_iter().all(f64::is_finite));
            assert_within(sample.value, AnalyticField::Affine.value(point), 2.0e-8);
            let expected = AnalyticField::Affine.gradient_ab(point);
            assert_within(sample.gradient_ab[0], expected[0], 2.0e-6);
            assert_within(sample.gradient_ab[1], expected[1], 2.0e-6);
        }
    }
    #[test]
    fn irregular_linear_rejects_non_finite_derived_output() {
        let mesh = IrregularTernaryMesh::new(samples()).unwrap();
        let values = mesh
            .vertex_ids()
            .map(|vertex| match vertex.0 {
                0 => f64::MAX,
                1 => -f64::MAX,
                _ => 0.0,
            })
            .collect();
        let field = IrregularTernaryScalarField::new(mesh, values).unwrap();
        let evaluator =
            InterpolatedIrregularTernaryField::new(&field, IrregularFieldInterpolation::Linear)
                .unwrap();
        assert!(matches!(
            evaluator.evaluate([0.5, 0.5, 0.0]),
            Err(IrregularFieldEvaluationError::NonFiniteEvaluation)
        ));
    }
}

#[cfg(feature = "irregular-cubic-alpha")]
mod irregular_cubic {
    use super::*;
    use ternary_contours::{
        BinaryExtrapolation, CubicAlphaMethod, InterpolatedIrregularTernaryField,
        IrregularAdaptiveContourOptions, IrregularContourGeometryOptions, IrregularContourSet,
        IrregularCubicAlphaOptions, IrregularFieldInterpolation, IrregularTernaryMesh,
        IrregularTernaryScalarField,
    };

    fn samples() -> [[f64; 3]; 13] {
        [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.72, 0.18, 0.10],
            [0.14, 0.71, 0.15],
            [0.17, 0.21, 0.62],
            [0.51, 0.37, 0.12],
            [0.27, 0.52, 0.21],
            [0.31, 0.16, 0.53],
            [0.36, 0.34, 0.30],
            [0.44, 0.11, 0.45],
            [0.11, 0.46, 0.43],
            [0.43, 0.44, 0.13],
        ]
    }

    #[test]
    fn irregular_cubic_preparation_contours_and_diagnostics_are_deterministic() {
        let mesh = IrregularTernaryMesh::new(samples()).unwrap();
        let field = IrregularTernaryScalarField::from_fn(mesh, |point| {
            AnalyticField::PairwiseQuadratic.value(point)
        })
        .unwrap();
        let options = IrregularCubicAlphaOptions {
            method: CubicAlphaMethod::Pchip,
            extrapolation: BinaryExtrapolation::Kohler,
            ..Default::default()
        };
        let first = InterpolatedIrregularTernaryField::new(
            &field,
            IrregularFieldInterpolation::CubicAlpha(options),
        )
        .unwrap();
        let second = InterpolatedIrregularTernaryField::new(
            &field,
            IrregularFieldInterpolation::CubicAlpha(options),
        )
        .unwrap();
        assert_eq!(first.cubic_diagnostics(), second.cubic_diagnostics());
        for vertex in field.mesh().vertex_ids() {
            let point = field.mesh().composition(vertex).unwrap();
            assert_within(
                first.value(point).unwrap(),
                field.value(vertex).unwrap(),
                1.0e-9,
            );
        }
        let mut generator = Lcg::new(0xa889_1234_5aa5_55aa);
        for _ in 0..127 {
            let sample = first.evaluate(generator.simplex()).unwrap();
            assert!(sample.value.is_finite() && sample.gradient_ab.into_iter().all(f64::is_finite));
        }

        let geometry = IrregularContourGeometryOptions {
            adaptive: IrregularAdaptiveContourOptions {
                max_depth: 3,
                flatness_tolerance: 1.0e-10,
                maximum_microtriangle_diameter: 0.04,
            },
            regularization: None,
            ..Default::default()
        };
        let contours = IrregularContourSet::compute_prepared(&first, &[0.42], geometry).unwrap();
        let repeated = IrregularContourSet::compute_prepared(&first, &[0.42], geometry).unwrap();
        assert_eq!(format!("{contours:?}"), format!("{repeated:?}"));
        let diagnostics = contours.diagnostics();
        assert_eq!(diagnostics.requested_level_count, 1);
        assert!(diagnostics.cubic_source.is_some());
        assert!(diagnostics.levels[0].evaluated_microtriangles > 0);
    }
}

