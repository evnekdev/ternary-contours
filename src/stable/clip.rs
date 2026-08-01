use crate::{TernaryCoordinate, simplex::logical_from_composition};

use super::sample::dot;

pub(crate) fn clip_half_plane(
    polygon: &mut Vec<[f64; 3]>,
    scratch: &mut Vec<[f64; 3]>,
    differences: [f64; 3],
    stability_tolerance: f64,
    geometry_tolerance: f64,
) -> bool {
    scratch.clear();
    if polygon.is_empty() {
        return true;
    }
    let mut previous = *polygon.last().unwrap();
    let mut previous_difference = dot(differences, previous);
    let mut previous_inside = previous_difference >= -stability_tolerance;
    for &current in polygon.iter() {
        let current_difference = dot(differences, current);
        let current_inside = current_difference >= -stability_tolerance;
        if previous_inside != current_inside {
            let denominator = previous_difference - current_difference;
            if !denominator.is_finite() || denominator == 0.0 {
                return false;
            }
            let parameter = (previous_difference / denominator).clamp(0.0, 1.0);
            let point = canonical_barycentric(
                interpolate(previous, current, parameter),
                geometry_tolerance,
            );
            push_unique(scratch, point, geometry_tolerance);
        }
        if current_inside {
            push_unique(
                scratch,
                canonical_barycentric(current, geometry_tolerance),
                geometry_tolerance,
            );
        }
        previous = current;
        previous_difference = current_difference;
        previous_inside = current_inside;
    }
    if scratch.len() > 1 && close(scratch[0], *scratch.last().unwrap(), geometry_tolerance) {
        scratch.pop();
    }
    std::mem::swap(polygon, scratch);
    true
}

pub(crate) fn canonical_barycentric(mut barycentric: [f64; 3], tolerance: f64) -> [f64; 3] {
    for weight in &mut barycentric {
        if weight.abs() <= tolerance {
            *weight = 0.0;
        } else if (1.0 - *weight).abs() <= tolerance {
            *weight = 1.0;
        }
        *weight = weight.clamp(0.0, 1.0);
    }
    let sum = barycentric.into_iter().sum::<f64>();
    if sum.is_finite() && sum > 0.0 {
        for weight in &mut barycentric {
            *weight /= sum;
        }
    }
    barycentric
}

pub(crate) fn interpolate(left: [f64; 3], right: [f64; 3], t: f64) -> [f64; 3] {
    [
        left[0] + (right[0] - left[0]) * t,
        left[1] + (right[1] - left[1]) * t,
        left[2] + (right[2] - left[2]) * t,
    ]
}

pub(crate) fn composition(vertices: [[f64; 3]; 3], barycentric: [f64; 3]) -> TernaryCoordinate {
    let mut result = [0.0; 3];
    for local in 0..3 {
        for component in 0..3 {
            result[component] += vertices[local][component] * barycentric[local];
        }
    }
    let sum = result.into_iter().sum::<f64>();
    if sum.is_finite() && sum != 0.0 {
        result = result.map(|value| value / sum);
    }
    result.into()
}

pub(crate) fn polygon_area(vertices: [[f64; 3]; 3], polygon: &[[f64; 3]]) -> f64 {
    if polygon.len() < 3 {
        return 0.0;
    }
    let logical: Vec<_> = polygon
        .iter()
        .copied()
        .map(|barycentric| logical_from_composition(composition(vertices, barycentric).as_array()))
        .collect();
    let double_area: f64 = logical
        .iter()
        .copied()
        .zip(logical.iter().copied().cycle().skip(1))
        .take(logical.len())
        .map(|(left, right)| left[0] * right[1] - left[1] * right[0])
        .sum();
    0.5 * double_area.abs()
}

pub(crate) fn close(left: [f64; 3], right: [f64; 3], tolerance: f64) -> bool {
    left.into_iter()
        .zip(right)
        .all(|(left, right)| (left - right).abs() <= tolerance)
}

fn push_unique(points: &mut Vec<[f64; 3]>, point: [f64; 3], tolerance: f64) {
    if points
        .last()
        .is_none_or(|previous| !close(*previous, point, tolerance))
    {
        points.push(point);
    }
}
