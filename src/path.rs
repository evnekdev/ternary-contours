//! Quantity-independent path cleanup, redistribution, and implicit projection.

use crate::{TernaryCoordinate, simplex::logical_distance};

/// Logical-plane path redistribution and safeguarded implicit projection options.
///
/// These controls are shared by ordinary contours and stable univariants. All
/// distances use the canonical equilateral ternary plane except
/// `max_normal_step`, which is a correction in independent semantic `(a,b)`
/// coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathRegularizationOptions {
    /// Target logical spacing between redistributed path points.
    pub spacing: f64,
    /// Number of redistribution/project passes; zero still performs one pass.
    pub redistribution_passes: usize,
    /// Accepted absolute implicit-equation residual after projection.
    pub projection_tolerance: f64,
    /// Maximum damped normal/Newton iterations for one point.
    pub max_projection_iterations: usize,
    /// Maximum candidate reductions for each Newton correction.
    pub max_backtracking_steps: usize,
    /// Maximum semantic `(a,b)` correction length per iteration.
    pub max_normal_step: f64,
    /// Logical distance from either fixed endpoint in which projection is skipped.
    pub protected_endpoint_distance: f64,
}

impl Default for PathRegularizationOptions {
    fn default() -> Self {
        Self {
            spacing: 0.0125,
            redistribution_passes: 2,
            projection_tolerance: 1.0e-9,
            max_projection_iterations: 16,
            max_backtracking_steps: 24,
            max_normal_step: 0.05,
            protected_endpoint_distance: 0.025,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PathProcessingError {
    InvalidSpacing { spacing: f64 },
    InvalidProjection,
    InvalidCoordinate,
    ZeroLength,
}

pub(crate) fn validate_regularization(
    options: PathRegularizationOptions,
) -> Result<(), PathProcessingError> {
    if !options.spacing.is_finite() || options.spacing <= 0.0 {
        return Err(PathProcessingError::InvalidSpacing {
            spacing: options.spacing,
        });
    }
    if !options.projection_tolerance.is_finite()
        || options.projection_tolerance <= 0.0
        || options.max_projection_iterations == 0
        || options.max_backtracking_steps == 0
        || !options.max_normal_step.is_finite()
        || options.max_normal_step <= 0.0
        || !options.protected_endpoint_distance.is_finite()
        || options.protected_endpoint_distance < 0.0
    {
        return Err(PathProcessingError::InvalidProjection);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RedistributedPoint {
    pub point: TernaryCoordinate,
    pub source_segment: usize,
    pub source_fraction: f64,
    pub source_arclength: f64,
}

pub(crate) fn cleanup(
    points: &[TernaryCoordinate],
    closed: bool,
    tolerance: f64,
) -> Result<Vec<TernaryCoordinate>, PathProcessingError> {
    let mut cleaned: Vec<TernaryCoordinate> = Vec::with_capacity(points.len());
    for &point in points {
        let point = normalize(point, tolerance).ok_or(PathProcessingError::InvalidCoordinate)?;
        if cleaned.last().is_none_or(|previous| {
            logical_distance(previous.as_array(), point.as_array()) > tolerance
        }) {
            cleaned.push(point);
        }
    }
    if closed
        && cleaned.len() > 1
        && logical_distance(
            cleaned[0].as_array(),
            cleaned.last().copied().unwrap().as_array(),
        ) <= tolerance
    {
        cleaned.pop();
    }
    let minimum = if closed { 3 } else { 2 };
    if cleaned.len() < minimum {
        return Err(PathProcessingError::ZeroLength);
    }
    Ok(cleaned)
}

pub(crate) fn redistribute(
    points: &[TernaryCoordinate],
    closed: bool,
    spacing: f64,
) -> Result<Vec<RedistributedPoint>, PathProcessingError> {
    if points.len() < 2 {
        return Err(PathProcessingError::ZeroLength);
    }
    let edge_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let mut cumulative = vec![0.0];
    for index in 0..edge_count {
        let next = (index + 1) % points.len();
        let length = logical_distance(points[index].as_array(), points[next].as_array());
        if !length.is_finite() {
            return Err(PathProcessingError::InvalidCoordinate);
        }
        cumulative.push(cumulative.last().copied().unwrap_or_default() + length);
    }
    let total = cumulative.last().copied().unwrap_or_default();
    if total <= f64::EPSILON {
        return Err(PathProcessingError::ZeroLength);
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
        let fraction = if span <= f64::EPSILON {
            0.0
        } else {
            (target - cumulative[edge]) / span
        };
        result.push(RedistributedPoint {
            point: lerp(points[edge], points[(edge + 1) % points.len()], fraction),
            source_segment: edge,
            source_fraction: fraction,
            source_arclength: target,
        });
    }
    Ok(result)
}

pub(crate) fn has_self_intersection(
    points: &[TernaryCoordinate],
    closed: bool,
    tolerance: f64,
) -> bool {
    if points.len() < 4 {
        return false;
    }
    let edge_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for first in 0..edge_count {
        let first_next = (first + 1) % points.len();
        for second in (first + 1)..edge_count {
            let second_next = (second + 1) % points.len();
            if first == second
                || first_next == second
                || second_next == first
                || (closed && first == 0 && second_next == 0)
            {
                continue;
            }
            if segments_intersect(
                points[first],
                points[first_next],
                points[second],
                points[second_next],
                tolerance,
            ) {
                return true;
            }
        }
    }
    false
}

fn segments_intersect(
    first_start: TernaryCoordinate,
    first_end: TernaryCoordinate,
    second_start: TernaryCoordinate,
    second_end: TernaryCoordinate,
    tolerance: f64,
) -> bool {
    let p = crate::simplex::logical_from_composition(first_start.as_array());
    let p2 = crate::simplex::logical_from_composition(first_end.as_array());
    let q = crate::simplex::logical_from_composition(second_start.as_array());
    let q2 = crate::simplex::logical_from_composition(second_end.as_array());
    let orientation = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let first_left = orientation(p, p2, q);
    let first_right = orientation(p, p2, q2);
    let second_left = orientation(q, q2, p);
    let second_right = orientation(q, q2, p2);
    first_left * first_right < -tolerance * tolerance
        && second_left * second_right < -tolerance * tolerance
}
pub(crate) fn path_length(points: &[TernaryCoordinate], closed: bool) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    let edge_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    (0..edge_count)
        .map(|index| {
            logical_distance(
                points[index].as_array(),
                points[(index + 1) % points.len()].as_array(),
            )
        })
        .sum()
}

pub(crate) fn spacing_coefficient_of_variation(points: &[TernaryCoordinate], closed: bool) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    let edge_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let lengths = (0..edge_count)
        .map(|index| {
            logical_distance(
                points[index].as_array(),
                points[(index + 1) % points.len()].as_array(),
            )
        })
        .collect::<Vec<_>>();
    let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
    if mean <= f64::EPSILON {
        return 0.0;
    }
    let variance = lengths
        .iter()
        .map(|length| (length - mean).powi(2))
        .sum::<f64>()
        / lengths.len() as f64;
    variance.sqrt() / mean
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImplicitSample {
    pub residual: f64,
    pub gradient_ab: [f64; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProjectionEvaluationError<E> {
    Reject,
    Fatal(E),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImplicitProjectionError<E> {
    Evaluation(E),
    RejectedInitialPoint,
    ZeroGradient { residual: f64 },
    NonConvergence { residual: f64, iterations: usize },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImplicitProjectionOutcome {
    pub point: TernaryCoordinate,
    pub iterations: usize,
    pub backtracking_steps: usize,
}

pub(crate) fn project_implicit<E>(
    mut point: TernaryCoordinate,
    options: PathRegularizationOptions,
    mut evaluate: impl FnMut(TernaryCoordinate) -> Result<ImplicitSample, ProjectionEvaluationError<E>>,
) -> Result<ImplicitProjectionOutcome, ImplicitProjectionError<E>> {
    let mut backtracking_steps = 0;
    for iteration in 0..options.max_projection_iterations {
        let located = match evaluate(point) {
            Ok(located) => located,
            Err(ProjectionEvaluationError::Reject) => {
                return Err(ImplicitProjectionError::RejectedInitialPoint);
            }
            Err(ProjectionEvaluationError::Fatal(error)) => {
                return Err(ImplicitProjectionError::Evaluation(error));
            }
        };
        let residual = located.residual;
        if residual.abs() <= options.projection_tolerance {
            return Ok(ImplicitProjectionOutcome {
                point,
                iterations: iteration + 1,
                backtracking_steps,
            });
        }
        let norm2 = located.gradient_ab[0].powi(2) + located.gradient_ab[1].powi(2);
        if !norm2.is_finite() || norm2 <= 1.0e-24 {
            return Err(ImplicitProjectionError::ZeroGradient { residual });
        }
        let factor = -residual / norm2;
        let mut delta = [
            factor * located.gradient_ab[0],
            factor * located.gradient_ab[1],
        ];
        let magnitude = delta[0].hypot(delta[1]);
        if magnitude > options.max_normal_step {
            let scale = options.max_normal_step / magnitude;
            delta[0] *= scale;
            delta[1] *= scale;
        }
        let source = point.as_array();
        let mut damping = 1.0;
        let mut accepted = None;
        for _ in 0..options.max_backtracking_steps {
            let candidate = normalize(
                TernaryCoordinate::new(
                    source[0] + damping * delta[0],
                    source[1] + damping * delta[1],
                    source[2] - damping * (delta[0] + delta[1]),
                ),
                options.projection_tolerance.max(1.0e-12),
            );
            if let Some(candidate) = candidate {
                match evaluate(candidate) {
                    Ok(next) if next.residual.abs() < residual.abs() => {
                        accepted = Some(candidate);
                        break;
                    }
                    Ok(_) | Err(ProjectionEvaluationError::Reject) => {}
                    Err(ProjectionEvaluationError::Fatal(error)) => {
                        return Err(ImplicitProjectionError::Evaluation(error));
                    }
                }
            }
            backtracking_steps += 1;
            damping *= 0.5;
        }
        let Some(candidate) = accepted else {
            return Err(ImplicitProjectionError::NonConvergence {
                residual,
                iterations: iteration + 1,
            });
        };
        point = candidate;
    }
    let residual = match evaluate(point) {
        Ok(sample) => sample.residual,
        Err(ProjectionEvaluationError::Reject) => {
            return Err(ImplicitProjectionError::RejectedInitialPoint);
        }
        Err(ProjectionEvaluationError::Fatal(error)) => {
            return Err(ImplicitProjectionError::Evaluation(error));
        }
    };
    Err(ImplicitProjectionError::NonConvergence {
        residual,
        iterations: options.max_projection_iterations,
    })
}

pub(crate) fn normalize(point: TernaryCoordinate, tolerance: f64) -> Option<TernaryCoordinate> {
    let [a, b, c] = point.as_array();
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

fn lerp(left: TernaryCoordinate, right: TernaryCoordinate, fraction: f64) -> TernaryCoordinate {
    let left = left.as_array();
    let right = right.as_array();
    TernaryCoordinate::new(
        left[0] + fraction * (right[0] - left[0]),
        left[1] + fraction * (right[1] - left[1]),
        left[2] + fraction * (right[2] - left[2]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_projection_backtracks_after_an_invalid_domain_candidate() {
        let start = TernaryCoordinate::new(0.1, 0.4, 0.5);
        let outcome = project_implicit(
            start,
            PathRegularizationOptions {
                max_normal_step: 0.5,
                ..PathRegularizationOptions::default()
            },
            |point| {
                let a = point.as_array()[0];
                if a < 0.05 {
                    return Err(ProjectionEvaluationError::<()>::Reject);
                }
                Ok(ImplicitSample {
                    residual: a - 0.06,
                    gradient_ab: [0.5, 0.0],
                })
            },
        )
        .unwrap();
        assert!(outcome.backtracking_steps > 0);
        assert!((outcome.point.as_array()[0] - 0.06).abs() <= 1.0e-9);
    }

    #[test]
    fn cleanup_and_redistribution_are_open_path_deterministic() {
        let start = TernaryCoordinate::new(0.8, 0.1, 0.1);
        let middle = TernaryCoordinate::new(0.5, 0.4, 0.1);
        let end = TernaryCoordinate::new(0.2, 0.7, 0.1);
        let cleaned = cleanup(&[start, start, middle, end], false, 1.0e-12).unwrap();
        assert_eq!(cleaned.len(), 3);
        for (actual, expected) in cleaned.iter().zip([start, middle, end]) {
            assert!(logical_distance(actual.as_array(), expected.as_array()) <= 1.0e-15);
        }
        let redistributed = redistribute(&cleaned, false, 0.05).unwrap();
        assert!(
            logical_distance(
                redistributed.first().unwrap().point.as_array(),
                start.as_array()
            ) <= 1.0e-15
        );
        assert!(
            logical_distance(
                redistributed.last().unwrap().point.as_array(),
                end.as_array()
            ) <= 1.0e-15
        );
        assert!(
            redistributed
                .windows(2)
                .all(|pair| pair[1].source_arclength > pair[0].source_arclength)
        );
    }
}
