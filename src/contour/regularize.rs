#[cfg(test)]
use crate::simplex::logical_from_composition;
use crate::{RegularTernaryScalarField, TernaryCoordinate};

#[cfg(feature = "cubic-alpha")]
use super::locate::ContourCubicField;
use super::{
    ContourError, ContourPath, ContourRegularization,
    locate::{LocatedValue, locate_linear},
};

pub(crate) enum FieldEvaluator<'a> {
    Linear(&'a RegularTernaryScalarField),
    #[cfg(feature = "cubic-alpha")]
    Cubic(&'a ContourCubicField<'a>),
}
impl FieldEvaluator<'_> {
    fn locate(&self, point: TernaryCoordinate) -> Result<LocatedValue, ContourError> {
        match self {
            Self::Linear(field) => locate_linear(field, point),
            #[cfg(feature = "cubic-alpha")]
            Self::Cubic(field) => field.locate(point),
        }
    }
}

pub(crate) fn regularize_paths(
    paths: &mut [ContourPath],
    level: f64,
    options: ContourRegularization,
    evaluator: &FieldEvaluator<'_>,
) -> Result<(), ContourError> {
    match crate::path::validate_regularization(options) {
        Ok(()) => {}
        Err(crate::path::PathProcessingError::InvalidSpacing { spacing }) => {
            return Err(ContourError::InvalidRegularizationSpacing { spacing });
        }
        Err(_) => {
            return Err(ContourError::InvalidProjectionOptions {
                tolerance: options.projection_tolerance,
                iterations: options.max_projection_iterations,
                max_step: options.max_normal_step,
            });
        }
    }
    for path in paths {
        let original_start = path.points.first().copied();
        let original_end = path.points.last().copied();
        for _ in 0..options.redistribution_passes.max(1) {
            path.points = redistribute(&path.points, path.closed, options.spacing)?;
            let len = path.points.len();
            for (index, point) in path.points.iter_mut().enumerate() {
                if !path.closed && (index == 0 || index + 1 == len) {
                    continue;
                }
                *point = project_point(*point, level, options, evaluator, None)?;
            }
        }
        if !path.closed {
            if let Some(start) = original_start {
                path.points[0] = start;
            }
            if let Some(end) = original_end {
                *path.points.last_mut().unwrap() = end;
            }
        }
    }
    Ok(())
}

fn redistribute(
    points: &[TernaryCoordinate],
    closed: bool,
    spacing: f64,
) -> Result<Vec<TernaryCoordinate>, ContourError> {
    let cleaned = crate::path::cleanup(points, closed, 1.0e-12).map_err(path_error)?;
    crate::path::redistribute(&cleaned, closed, spacing)
        .map(|points| points.into_iter().map(|point| point.point).collect())
        .map_err(path_error)
}

fn path_error(error: crate::path::PathProcessingError) -> ContourError {
    match error {
        crate::path::PathProcessingError::InvalidSpacing { spacing } => {
            ContourError::InvalidRegularizationSpacing { spacing }
        }
        crate::path::PathProcessingError::InvalidProjection => {
            ContourError::InvalidProjectionOptions {
                tolerance: f64::NAN,
                iterations: 0,
                max_step: f64::NAN,
            }
        }
        crate::path::PathProcessingError::InvalidCoordinate
        | crate::path::PathProcessingError::ZeroLength => ContourError::ZeroLengthPath,
    }
}

fn project_point(
    point: TernaryCoordinate,
    level: f64,
    options: ContourRegularization,
    evaluator: &FieldEvaluator<'_>,
    mut trace: Option<&mut Vec<f64>>,
) -> Result<TernaryCoordinate, ContourError> {
    let result = crate::path::project_implicit(point, options, |candidate| {
        let located = evaluator
            .locate(candidate)
            .map_err(crate::path::ProjectionEvaluationError::Fatal)?;
        let residual = located.value - level;
        if let Some(values) = trace.as_deref_mut() {
            values.push(residual.abs());
        }
        Ok(crate::path::ImplicitSample {
            residual,
            gradient_ab: located.gradient_ab,
        })
    });
    match result {
        Ok(outcome) => Ok(outcome.point),
        Err(crate::path::ImplicitProjectionError::Evaluation(error)) => Err(error),
        Err(crate::path::ImplicitProjectionError::ZeroGradient { residual }) => {
            Err(ContourError::ProjectionZeroGradient { residual })
        }
        Err(crate::path::ImplicitProjectionError::NonConvergence {
            residual,
            iterations,
        }) => Err(ContourError::ProjectionNonConvergence {
            residual,
            iterations,
        }),
        Err(crate::path::ImplicitProjectionError::RejectedInitialPoint) => {
            Err(ContourError::ProjectionNonConvergence {
                residual: f64::NAN,
                iterations: 0,
            })
        }
    }
}
#[cfg(test)]
fn xy(point: TernaryCoordinate) -> [f64; 2] {
    logical_from_composition(point.as_array())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContourPath, RegularTernaryScalarField};
    fn field(n: usize) -> RegularTernaryScalarField {
        let count = (n + 1) * (n + 2) / 2;
        let blank = RegularTernaryScalarField::new(n, vec![0.0; count]).unwrap();
        let values = (0..count)
            .map(|i| {
                let [a, b, c] = blank.composition_at(i).unwrap();
                2.0 * a - 3.0 * b + 5.0 * c
            })
            .collect();
        RegularTernaryScalarField::new(n, values).unwrap()
    }
    #[test]
    fn redistribution_uniforms_spacing_preserves_open_endpoints_and_level() {
        let field = field(8);
        let evaluator = FieldEvaluator::Linear(&field);
        let start = TernaryCoordinate::new(0.1, 0.5, 0.4);
        let middle = TernaryCoordinate::new(0.12, 0.4925, 0.3875);
        let end = TernaryCoordinate::new(0.5, 0.35, 0.15);
        let level = {
            let [a, b, c] = start.as_array();
            2.0 * a - 3.0 * b + 5.0 * c
        };
        let mut paths = vec![ContourPath {
            points: vec![start, middle, end],
            closed: false,
        }];
        let options = ContourRegularization {
            spacing: 0.04,
            ..ContourRegularization::default()
        };
        regularize_paths(&mut paths, level, options, &evaluator).unwrap();
        assert_eq!(paths[0].points[0], start);
        assert_eq!(*paths[0].points.last().unwrap(), end);
        for point in &paths[0].points {
            let located = evaluator.locate(*point).unwrap();
            assert!((located.value - level).abs() < 1e-8);
        }
        let lengths: Vec<_> = paths[0]
            .points
            .windows(2)
            .map(|pair| distance(pair[0], pair[1]))
            .collect();
        let min = lengths.iter().copied().fold(f64::INFINITY, f64::min);
        let max = lengths.iter().copied().fold(0.0, f64::max);
        assert!(max - min < 0.01);
    }
    #[test]
    fn projection_residual_decreases_monotonically() {
        let field = field(5);
        let evaluator = FieldEvaluator::Linear(&field);
        let options = ContourRegularization::default();
        let mut trace = Vec::new();
        let _ = project_point(
            TernaryCoordinate::new(0.3, 0.3, 0.4),
            0.5,
            options,
            &evaluator,
            Some(&mut trace),
        );
        for pair in trace.windows(2) {
            assert!(pair[1] <= pair[0] + 1e-15);
        }
    }
    #[test]
    fn closed_redistribution_is_periodic_without_duplicate_endpoint_or_orientation_change() {
        let source = vec![
            TernaryCoordinate::new(0.6, 0.2, 0.2),
            TernaryCoordinate::new(0.2, 0.6, 0.2),
            TernaryCoordinate::new(0.2, 0.2, 0.6),
        ];
        let redistributed = redistribute(&source, true, 0.08).unwrap();
        assert!(redistributed.len() >= 3);
        assert_ne!(redistributed.first(), redistributed.last());
        assert_eq!(
            signed_area(&source).signum(),
            signed_area(&redistributed).signum()
        );
    }

    #[test]
    fn zero_gradient_and_maximum_step_protection_are_controlled() {
        let constant = RegularTernaryScalarField::new(2, vec![1.0; 6]).unwrap();
        let constant_evaluator = FieldEvaluator::Linear(&constant);
        assert!(matches!(
            project_point(
                TernaryCoordinate::new(0.3, 0.3, 0.4),
                0.0,
                ContourRegularization::default(),
                &constant_evaluator,
                None,
            ),
            Err(ContourError::ProjectionZeroGradient { .. })
        ));

        let varying = field(8);
        let varying_evaluator = FieldEvaluator::Linear(&varying);
        let start = TernaryCoordinate::new(0.1, 0.45, 0.45);
        let restricted = ContourRegularization {
            max_projection_iterations: 1,
            max_normal_step: 1.0e-4,
            ..ContourRegularization::default()
        };
        assert!(matches!(
            project_point(start, 2.0, restricted, &varying_evaluator, None),
            Err(ContourError::ProjectionNonConvergence { iterations: 1, .. })
        ));
        let unrestricted = ContourRegularization {
            max_normal_step: 1.0,
            ..ContourRegularization::default()
        };
        let projected = project_point(start, 2.0, unrestricted, &varying_evaluator, None).unwrap();
        assert!((varying_evaluator.locate(projected).unwrap().value - 2.0).abs() < 1.0e-8);
    }

    #[test]
    fn projection_relocates_across_regular_grid_triangle_boundaries() {
        let n = 8;
        let count = (n + 1) * (n + 2) / 2;
        let blank = RegularTernaryScalarField::new(n, vec![0.0; count]).unwrap();
        let values = (0..count)
            .map(|index| blank.composition_at(index).unwrap()[0])
            .collect();
        let field = RegularTernaryScalarField::new(n, values).unwrap();
        let evaluator = FieldEvaluator::Linear(&field);
        let start = TernaryCoordinate::new(0.05, 0.45, 0.50);
        let projected = project_point(
            start,
            0.35,
            ContourRegularization {
                max_normal_step: 0.5,
                ..ContourRegularization::default()
            },
            &evaluator,
            None,
        )
        .unwrap();
        assert!((projected.as_array()[0] - 0.35).abs() < 1.0e-9);
        assert!((projected.as_array()[0] - start.as_array()[0]).abs() > 2.0 / n as f64);
    }

    fn distance(left: TernaryCoordinate, right: TernaryCoordinate) -> f64 {
        crate::simplex::logical_distance(left.as_array(), right.as_array())
    }

    fn signed_area(points: &[TernaryCoordinate]) -> f64 {
        (0..points.len())
            .map(|index| {
                let left = xy(points[index]);
                let right = xy(points[(index + 1) % points.len()]);
                left[0] * right[1] - right[0] * left[1]
            })
            .sum::<f64>()
            / 2.0
    }

    #[test]
    fn disabled_regularization_is_represented_by_none_without_mutation() {
        let path = ContourPath {
            points: vec![
                TernaryCoordinate::new(0.2, 0.3, 0.5),
                TernaryCoordinate::new(0.4, 0.2, 0.4),
            ],
            closed: false,
        };
        let original = path.clone();
        let options: Option<ContourRegularization> = None;
        if options.is_none() {
            assert_eq!(path, original);
        }
    }
}
