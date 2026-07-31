use std::collections::BTreeMap;

use crate::{GridVertexId, RegularTernaryScalarField, TernaryCoordinate};

use super::{
    ContourError,
    paths::{ContourPath, ContourSegment, join_segments},
};

pub(crate) fn linear_paths(
    field: &RegularTernaryScalarField,
    level: f64,
    value_tolerance: f64,
    geometry_tolerance: f64,
) -> Result<Vec<ContourPath>, ContourError> {
    let triangles = field.elementary_triangles()?;
    let mut owners = BTreeMap::new();
    for triangle in &triangles {
        for (left, right) in [(0, 1), (1, 2), (2, 0)] {
            owners
                .entry(edge_key(triangle.vertices[left], triangle.vertices[right]))
                .or_insert(triangle.id);
        }
    }
    let mut segments = Vec::new();
    for triangle in triangles {
        let [v0, v1, v2] = triangle.vertices;
        let values = [field.value(v0)?, field.value(v1)?, field.value(v2)?];
        let points = [
            field.composition(v0)?.into(),
            field.composition(v1)?.into(),
            field.composition(v2)?.into(),
        ];
        let on = values.map(|value| (value - level).abs() <= value_tolerance);
        if on.into_iter().all(|value| value) {
            return Err(ContourError::FlatTriangle {
                triangle: triangle.id,
                level,
            });
        }
        if let Some((left, right)) = [(0, 1), (1, 2), (2, 0)]
            .into_iter()
            .find(|(left, right)| on[*left] && on[*right])
        {
            let key = edge_key(triangle.vertices[left], triangle.vertices[right]);
            if owners[&key] == triangle.id {
                segments.push(ContourSegment {
                    start: points[left],
                    end: points[right],
                });
            }
            continue;
        }
        let mut crossings = Vec::new();
        for (left, right) in [(0, 1), (1, 2), (2, 0)] {
            match (on[left], on[right]) {
                (true, false) => push_unique(&mut crossings, points[left], geometry_tolerance),
                (false, true) => push_unique(&mut crossings, points[right], geometry_tolerance),
                (false, false) => {
                    let d0 = values[left] - level;
                    let d1 = values[right] - level;
                    if d0.is_sign_positive() != d1.is_sign_positive() {
                        let denominator = values[right] - values[left];
                        if denominator.abs() > value_tolerance {
                            let mut t = (level - values[left]) / denominator;
                            if t.abs() <= value_tolerance {
                                t = 0.0;
                            } else if (1.0 - t).abs() <= value_tolerance {
                                t = 1.0;
                            }
                            push_unique(
                                &mut crossings,
                                lerp(points[left], points[right], t),
                                geometry_tolerance,
                            );
                        }
                    }
                }
                (true, true) => unreachable!(),
            }
        }
        if crossings.len() == 2 && !points_close(crossings[0], crossings[1], geometry_tolerance) {
            segments.push(ContourSegment {
                start: crossings[0],
                end: crossings[1],
            });
        }
    }
    join_segments(segments, geometry_tolerance)
}

#[cfg(feature = "cubic-alpha")]
pub(crate) fn march_sampled_triangle(
    points: [TernaryCoordinate; 3],
    values: [f64; 3],
    level: f64,
    tolerance: f64,
    segments: &mut Vec<ContourSegment>,
) {
    let mut crossings = Vec::new();
    for (left, right) in [(0, 1), (1, 2), (2, 0)] {
        let d0 = values[left] - level;
        let d1 = values[right] - level;
        let on0 = d0.abs() <= tolerance;
        let on1 = d1.abs() <= tolerance;
        if on0 {
            push_unique(&mut crossings, points[left], tolerance);
        }
        if on1 {
            push_unique(&mut crossings, points[right], tolerance);
        }
        if !on0 && !on1 && d0.is_sign_positive() != d1.is_sign_positive() {
            let t = (level - values[left]) / (values[right] - values[left]);
            push_unique(
                &mut crossings,
                lerp(points[left], points[right], t),
                tolerance,
            );
        }
    }
    if crossings.len() == 2 && !points_close(crossings[0], crossings[1], tolerance) {
        segments.push(ContourSegment {
            start: crossings[0],
            end: crossings[1],
        });
    }
}

fn edge_key(left: GridVertexId, right: GridVertexId) -> (GridVertexId, GridVertexId) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
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
fn push_unique(points: &mut Vec<TernaryCoordinate>, point: TernaryCoordinate, tolerance: f64) {
    if points
        .iter()
        .all(|existing| !points_close(*existing, point, tolerance))
    {
        points.push(point);
    }
}
fn points_close(left: TernaryCoordinate, right: TernaryCoordinate, tolerance: f64) -> bool {
    left.as_array()
        .into_iter()
        .zip(right.as_array())
        .all(|(a, b)| (a - b).abs() <= tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(n: usize, fun: impl Fn(f64, f64, f64) -> f64) -> RegularTernaryScalarField {
        let count = (n + 1) * (n + 2) / 2;
        let blank = RegularTernaryScalarField::new(n, vec![0.0; count]).unwrap();
        let values = (0..count)
            .map(|index| {
                let [a, b, c] = blank.composition_at(index).unwrap();
                fun(a, b, c)
            })
            .collect();
        RegularTernaryScalarField::new(n, values).unwrap()
    }
    #[test]
    fn analytic_linear_field_recovers_straight_isolines() {
        let field = field(8, |a, b, c| 2.0 * a - 3.0 * b + 5.0 * c);
        for level in [-1.0, 0.0, 1.0, 3.0] {
            for path in linear_paths(&field, level, 1e-12, 1e-9).unwrap() {
                for point in path.points {
                    let [a, b, c] = point.as_array();
                    assert!((2.0 * a - 3.0 * b + 5.0 * c - level).abs() < 1e-10);
                }
            }
        }
    }
    #[test]
    fn endpoint_and_coincident_edge_degeneracies_are_deterministic() {
        let field = field(2, |a, _, _| a);
        let paths = linear_paths(&field, 0.5, 1e-12, 1e-9).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].points.len() >= 2);
    }
    #[test]
    fn non_finite_and_flat_inputs_are_explicit() {
        let field = field(1, |_, _, _| 2.0);
        assert!(matches!(
            linear_paths(&field, 2.0, 1e-12, 1e-9),
            Err(ContourError::FlatTriangle { .. })
        ));
    }
}
