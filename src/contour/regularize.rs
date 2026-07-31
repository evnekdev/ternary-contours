use crate::{RegularTernaryScalarField, TernaryCoordinate};

const EQUILATERAL_TRIANGLE_HEIGHT: f64 = 0.866_025_403_784_438_6;

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
    options.validate()?;
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
    if points.len() < 2 {
        return Err(ContourError::ZeroLengthPath);
    }
    let edge_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let mut cumulative = vec![0.0];
    for index in 0..edge_count {
        let next = (index + 1) % points.len();
        cumulative.push(cumulative.last().unwrap() + distance(points[index], points[next]));
    }
    let total = *cumulative.last().unwrap();
    if total <= f64::EPSILON {
        return Err(ContourError::ZeroLengthPath);
    }
    let intervals = (total / spacing).ceil().max(if closed { 3.0 } else { 1.0 }) as usize;
    let sample_count = if closed { intervals } else { intervals + 1 };
    let mut result = Vec::with_capacity(sample_count);
    for sample in 0..sample_count {
        let target = total * sample as f64 / intervals as f64;
        let edge = cumulative
            .windows(2)
            .position(|window| target <= window[1])
            .unwrap_or(edge_count - 1);
        let span = cumulative[edge + 1] - cumulative[edge];
        let t = if span == 0.0 {
            0.0
        } else {
            (target - cumulative[edge]) / span
        };
        result.push(lerp(points[edge], points[(edge + 1) % points.len()], t));
    }
    Ok(result)
}

fn project_point(
    mut point: TernaryCoordinate,
    level: f64,
    options: ContourRegularization,
    evaluator: &FieldEvaluator<'_>,
    mut trace: Option<&mut Vec<f64>>,
) -> Result<TernaryCoordinate, ContourError> {
    for iteration in 0..options.max_projection_iterations {
        let located = evaluator.locate(point)?;
        let residual = located.value - level;
        if let Some(values) = trace.as_deref_mut() {
            values.push(residual.abs());
        }
        if residual.abs() <= options.projection_tolerance {
            return Ok(point);
        }
        let norm2 = located.gradient_ab[0].powi(2) + located.gradient_ab[1].powi(2);
        if !norm2.is_finite() || norm2 <= 1e-24 {
            return Err(ContourError::ProjectionZeroGradient { residual });
        }
        let factor = -residual / norm2;
        let mut delta = [
            factor * located.gradient_ab[0],
            factor * located.gradient_ab[1],
        ];
        let length = delta[0].hypot(delta[1]);
        if length > options.max_normal_step {
            let scale = options.max_normal_step / length;
            delta[0] *= scale;
            delta[1] *= scale;
        }
        let source = point.as_array();
        let mut accepted = None;
        let mut damping = 1.0;
        for _ in 0..24 {
            let a = source[0] + delta[0] * damping;
            let b = source[1] + delta[1] * damping;
            let c = 1.0 - a - b;
            let candidate =
                normalized_candidate(a, b, c, options.projection_tolerance.max(1.0e-12));
            if let Some(candidate) = candidate
                && let Ok(next) = evaluator.locate(candidate)
                && (next.value - level).abs() < residual.abs()
            {
                accepted = Some(candidate);
                break;
            }
            damping *= 0.5;
        }
        let Some(candidate) = accepted else {
            return Err(ContourError::ProjectionNonConvergence {
                residual,
                iterations: iteration + 1,
            });
        };
        point = candidate;
    }
    let residual = evaluator.locate(point)?.value - level;
    Err(ContourError::ProjectionNonConvergence {
        residual,
        iterations: options.max_projection_iterations,
    })
}

fn normalized_candidate(a: f64, b: f64, c: f64, tolerance: f64) -> Option<TernaryCoordinate> {
    if ![a, b, c].into_iter().all(f64::is_finite)
        || [a, b, c].into_iter().any(|value| value < -tolerance)
    {
        return None;
    }
    let components = [a.max(0.0), b.max(0.0), c.max(0.0)];
    let sum = components.into_iter().sum::<f64>();
    if !sum.is_finite() || sum <= tolerance {
        return None;
    }
    Some(TernaryCoordinate::new(
        components[0] / sum,
        components[1] / sum,
        components[2] / sum,
    ))
}
fn xy(point: TernaryCoordinate) -> [f64; 2] {
    let [a, _b, c] = point.as_array();
    [c + 0.5 * a, EQUILATERAL_TRIANGLE_HEIGHT * a]
}
fn distance(left: TernaryCoordinate, right: TernaryCoordinate) -> f64 {
    let left = xy(left);
    let right = xy(right);
    (left[0] - right[0]).hypot(left[1] - right[1])
}
fn lerp(left: TernaryCoordinate, right: TernaryCoordinate, t: f64) -> TernaryCoordinate {
    let a = left.as_array();
    let b = right.as_array();
    TernaryCoordinate::new(
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    )
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
