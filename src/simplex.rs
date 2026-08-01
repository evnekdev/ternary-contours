//! Shared two-dimensional simplex barycentric and gradient helpers.

/// Height of the unit-side canonical equilateral ternary triangle.
pub(crate) const EQUILATERAL_HEIGHT: f64 = 0.866_025_403_784_438_6;

/// Convert semantic `(a, b, c)` components to the canonical logical plane.
///
/// The pure-component corners are `A=(0,0)`, `B=(1,0)`, and
/// `C=(1/2, sqrt(3)/2)`. This embedding is used for Delaunay construction,
/// contour lengths, and scale-aware numerical geometry; rendering crates are
/// free to apply their own display projection afterwards.
pub(crate) fn logical_from_composition([_a, b, c]: [f64; 3]) -> [f64; 2] {
    [b + 0.5 * c, EQUILATERAL_HEIGHT * c]
}

/// Euclidean distance in the canonical logical ternary plane.
pub(crate) fn logical_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let left = logical_from_composition(left);
    let right = logical_from_composition(right);
    (left[0] - right[0]).hypot(left[1] - right[1])
}
#[cfg(feature = "irregular-delaunay")]
pub(crate) fn barycentric_ab(vertices: [[f64; 3]; 3], composition: [f64; 3]) -> Option<[f64; 3]> {
    let da0 = vertices[0][0] - vertices[2][0];
    let da1 = vertices[1][0] - vertices[2][0];
    let db0 = vertices[0][1] - vertices[2][1];
    let db1 = vertices[1][1] - vertices[2][1];
    let determinant = da0 * db1 - da1 * db0;
    if !determinant.is_finite() || determinant == 0.0 {
        return None;
    }
    let da = composition[0] - vertices[2][0];
    let db = composition[1] - vertices[2][1];
    let first = (da * db1 - da1 * db) / determinant;
    let second = (da0 * db - da * db0) / determinant;
    let barycentric = [first, second, 1.0 - first - second];
    barycentric
        .into_iter()
        .all(f64::is_finite)
        .then_some(barycentric)
}

pub(crate) fn global_gradient_ab(
    vertices: [[f64; 3]; 3],
    local_gradient: [f64; 2],
) -> Option<[f64; 2]> {
    let da0 = vertices[0][0] - vertices[2][0];
    let da1 = vertices[1][0] - vertices[2][0];
    let db0 = vertices[0][1] - vertices[2][1];
    let db1 = vertices[1][1] - vertices[2][1];
    let determinant = da0 * db1 - da1 * db0;
    if !determinant.is_finite() || determinant == 0.0 {
        return None;
    }
    let gradient = [
        (local_gradient[0] * db1 - db0 * local_gradient[1]) / determinant,
        (da0 * local_gradient[1] - local_gradient[0] * da1) / determinant,
    ];
    gradient.into_iter().all(f64::is_finite).then_some(gradient)
}

#[cfg(feature = "irregular-delaunay")]
pub(crate) fn canonical_barycentric(mut barycentric: [f64; 3], tolerance: f64) -> Option<[f64; 3]> {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return None;
    }
    for weight in &mut barycentric {
        if !weight.is_finite() || *weight < -tolerance || *weight > 1.0 + tolerance {
            return None;
        }
        if weight.abs() <= tolerance {
            *weight = 0.0;
        } else if (1.0 - *weight).abs() <= tolerance {
            *weight = 1.0;
        }
    }
    let sum = barycentric.into_iter().sum::<f64>();
    if !sum.is_finite() || sum <= 0.0 {
        return None;
    }
    for weight in &mut barycentric {
        *weight /= sum;
    }
    barycentric
        .into_iter()
        .all(|weight| weight >= 0.0)
        .then_some(barycentric)
}

#[cfg(feature = "irregular-delaunay")]
pub(crate) fn valid_barycentric(barycentric: [f64; 3], tolerance: f64) -> bool {
    let sum = barycentric.into_iter().sum::<f64>();
    barycentric
        .into_iter()
        .all(|weight| weight.is_finite() && weight >= 0.0)
        && (sum - 1.0).abs() <= tolerance
}
