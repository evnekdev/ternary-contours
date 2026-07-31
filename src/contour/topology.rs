#[cfg(feature = "cubic-alpha")]
use crate::TernaryCoordinate;

#[cfg(feature = "cubic-alpha")]
use super::{
    AdaptiveContourOptions, ContourError,
    linear::march_sampled_triangle,
    locate::ContourCubicField,
    paths::{ContourPath, ContourSegment, join_segments},
};

#[cfg(feature = "cubic-alpha")]
#[derive(Clone, Copy)]
struct Sample {
    local: [f64; 3],
    composition: TernaryCoordinate,
    value: f64,
}

#[cfg(feature = "cubic-alpha")]
pub(crate) fn cubic_paths(
    model: &mut ContourCubicField<'_>,
    level: f64,
    options: AdaptiveContourOptions,
) -> Result<Vec<ContourPath>, ContourError> {
    options.validate()?;
    let mut segments = Vec::new();
    for triangle_index in 0..model.triangles().len() {
        let vertices = model.triangle_vertices(triangle_index)?;
        let samples = [
            sample(model, triangle_index, [1.0, 0.0, 0.0], vertices),
            sample(model, triangle_index, [0.0, 1.0, 0.0], vertices),
            sample(model, triangle_index, [0.0, 0.0, 1.0], vertices),
        ];
        refine(
            model,
            triangle_index,
            samples,
            vertices,
            level,
            options,
            0,
            &mut segments,
        )?;
    }
    join_segments(segments, options.geometry_tolerance)
}

#[cfg(feature = "cubic-alpha")]
#[allow(clippy::too_many_arguments)]
fn refine(
    model: &mut ContourCubicField<'_>,
    triangle_index: usize,
    cell: [Sample; 3],
    vertices: [TernaryCoordinate; 3],
    level: f64,
    options: AdaptiveContourOptions,
    depth: u8,
    segments: &mut Vec<ContourSegment>,
) -> Result<(), ContourError> {
    let mids = [
        midpoint(model, triangle_index, cell[0], cell[1], vertices),
        midpoint(model, triangle_index, cell[1], cell[2], vertices),
        midpoint(model, triangle_index, cell[2], cell[0], vertices),
    ];
    let centre_local = [
        (cell[0].local[0] + cell[1].local[0] + cell[2].local[0]) / 3.0,
        (cell[0].local[1] + cell[1].local[1] + cell[2].local[1]) / 3.0,
        (cell[0].local[2] + cell[1].local[2] + cell[2].local[2]) / 3.0,
    ];
    let centre = sample(model, triangle_index, centre_local, vertices);
    let all = [
        cell[0].value,
        cell[1].value,
        cell[2].value,
        mids[0].value,
        mids[1].value,
        mids[2].value,
        centre.value,
    ];
    let minimum = all.into_iter().fold(f64::INFINITY, f64::min);
    let maximum = all.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let bracket =
        minimum <= level + options.value_tolerance && maximum >= level - options.value_tolerance;
    let flatness = [
        (mids[0].value - (cell[0].value + cell[1].value) / 2.0).abs(),
        (mids[1].value - (cell[1].value + cell[2].value) / 2.0).abs(),
        (mids[2].value - (cell[2].value + cell[0].value) / 2.0).abs(),
    ]
    .into_iter()
    .fold(0.0, f64::max);
    if depth >= 2 && !bracket && flatness <= options.flatness_tolerance {
        return Ok(());
    }
    if depth < options.max_depth && (depth < 2 || bracket || flatness > options.flatness_tolerance)
    {
        model.diagnostics_mut().refined_triangles += 1;
        let children = [
            [cell[0], mids[0], mids[2]],
            [mids[0], cell[1], mids[1]],
            [mids[2], mids[1], cell[2]],
            [mids[0], mids[1], mids[2]],
        ];
        for child in children {
            refine(
                model,
                triangle_index,
                child,
                vertices,
                level,
                options,
                depth + 1,
                segments,
            )?;
        }
        return Ok(());
    }
    if bracket {
        if depth == options.max_depth && flatness > options.flatness_tolerance {
            model.diagnostics_mut().maximum_depth_hits += 1;
        }
        march_sampled_triangle(
            cell.map(|sample| sample.composition),
            cell.map(|sample| sample.value),
            level,
            options.value_tolerance,
            segments,
        );
    }
    Ok(())
}

#[cfg(feature = "cubic-alpha")]
fn midpoint(
    model: &ContourCubicField<'_>,
    triangle_index: usize,
    left: Sample,
    right: Sample,
    vertices: [TernaryCoordinate; 3],
) -> Sample {
    let local = [
        (left.local[0] + right.local[0]) / 2.0,
        (left.local[1] + right.local[1]) / 2.0,
        (left.local[2] + right.local[2]) / 2.0,
    ];
    sample(model, triangle_index, local, vertices)
}
#[cfg(feature = "cubic-alpha")]
fn sample(
    model: &ContourCubicField<'_>,
    triangle_index: usize,
    local: [f64; 3],
    vertices: [TernaryCoordinate; 3],
) -> Sample {
    let composition = combine(vertices, local);
    let value = model.value_in_triangle(triangle_index, local);
    Sample {
        local,
        composition,
        value,
    }
}
#[cfg(feature = "cubic-alpha")]
fn combine(vertices: [TernaryCoordinate; 3], weights: [f64; 3]) -> TernaryCoordinate {
    let values = vertices.map(TernaryCoordinate::as_array);
    TernaryCoordinate::new(
        values[0][0] * weights[0] + values[1][0] * weights[1] + values[2][0] * weights[2],
        values[0][1] * weights[0] + values[1][1] * weights[1] + values[2][1] * weights[2],
        values[0][2] * weights[0] + values[1][2] * weights[1] + values[2][2] * weights[2],
    )
}

#[cfg(all(test, feature = "cubic-alpha"))]
mod tests {
    use super::*;
    use crate::{CubicAlphaOptions, RegularTernaryScalarField};
    fn radial(a: f64, b: f64, c: f64) -> f64 {
        (a - 0.34).powi(2) + (b - 0.33).powi(2) + (c - 0.33).powi(2)
    }
    fn field(n: usize) -> RegularTernaryScalarField {
        let count = (n + 1) * (n + 2) / 2;
        let blank = RegularTernaryScalarField::new(n, vec![0.0; count]).unwrap();
        let values = (0..count)
            .map(|i| {
                let [a, b, c] = blank.composition_at(i).unwrap();
                radial(a, b, c)
            })
            .collect();
        RegularTernaryScalarField::new(n, values).unwrap()
    }
    #[test]
    fn adaptive_refinement_detects_closed_interior_contour() {
        let field = field(5);
        let mut model = ContourCubicField::new(&field, CubicAlphaOptions::default()).unwrap();
        let paths = cubic_paths(&mut model, 0.08, AdaptiveContourOptions::default()).unwrap();
        assert!(paths.iter().any(|path| path.closed));
        assert!(model.diagnostics().refined_triangles > 0);
    }
    #[test]
    fn adaptive_output_points_have_small_local_level_residual() {
        let field = field(6);
        let mut model = ContourCubicField::new(&field, CubicAlphaOptions::default()).unwrap();
        let paths = cubic_paths(&mut model, 0.10, AdaptiveContourOptions::default()).unwrap();
        for path in paths {
            for point in path.points {
                let located = model.locate(point).unwrap();
                assert!((located.value - 0.10).abs() < 5e-3);
            }
        }
    }
}
