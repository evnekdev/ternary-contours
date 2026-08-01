//! Numerical irregular-Delaunay contour workflow with no rendering dependency.

use ternary_contours::{
    BinaryExtrapolation, ContourRegularization, InterpolatedIrregularTernaryField,
    IrregularAdaptiveContourOptions, IrregularContourGeometryOptions,
    IrregularContourInterpolation, IrregularContourOptions, IrregularContourSet,
    IrregularCubicAlphaOptions, IrregularFieldInterpolation, IrregularTernaryMesh,
    IrregularTernaryScalarField,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mesh = IrregularTernaryMesh::new([
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.78, 0.13, 0.09],
        [0.57, 0.28, 0.15],
        [0.18, 0.61, 0.21],
        [0.23, 0.16, 0.61],
        [0.31, 0.42, 0.27],
        [0.47, 0.12, 0.41],
        [0.14, 0.37, 0.49],
    ])?;
    println!(
        "mesh: vertices={}, edges={}, triangles={}, boundary_edges={}",
        mesh.vertex_count(),
        mesh.edge_count(),
        mesh.triangle_count(),
        mesh.boundary_edges().count(),
    );
    for edge in mesh.edges().take(3) {
        println!(
            "edge {:?}: {:?}, incident={:?}",
            edge.id, edge.vertices, edge.triangles
        );
    }

    let field = IrregularTernaryScalarField::from_fn(mesh, |[a, b, c]| {
        (a - 0.31).powi(2) + 0.7 * (b - 0.27).powi(2) + 0.2 * c + 0.15 * a * b
    })?;
    let levels = [0.14, 0.18, 0.24];

    let linear = IrregularContourSet::compute(&field, &levels, IrregularContourOptions::linear())?;
    println!(
        "linear components at first level: {}",
        linear.levels[0].paths.len()
    );

    let cubic_options = IrregularCubicAlphaOptions {
        extrapolation: BinaryExtrapolation::Kohler,
        ..IrregularCubicAlphaOptions::default()
    };
    let prepared = InterpolatedIrregularTernaryField::new(
        &field,
        IrregularFieldInterpolation::CubicAlpha(cubic_options),
    )?;
    let alpha = prepared
        .cubic_diagnostics()
        .expect("cubic evaluator selected");
    println!(
        "alpha: convergence={:?}, sweeps={}, residual={:?}",
        alpha.convergence, alpha.sweep_count, alpha.residual
    );

    let geometry = IrregularContourGeometryOptions {
        adaptive: IrregularAdaptiveContourOptions {
            max_depth: 7,
            maximum_microtriangle_diameter: 0.03,
            ..IrregularAdaptiveContourOptions::default()
        },
        regularization: Some(ContourRegularization {
            spacing: 0.035,
            ..ContourRegularization::default()
        }),
        ..IrregularContourGeometryOptions::default()
    };
    let contours = IrregularContourSet::compute_prepared(&prepared, &levels, geometry)?;
    for (level, diagnostics) in contours.levels.iter().zip(&contours.diagnostics().levels) {
        println!(
            "level {}: open={}, closed={}, refined={}, projected={}, max residual={:e}",
            level.value,
            diagnostics.open_paths,
            diagnostics.closed_paths,
            diagnostics.refined_microtriangles,
            diagnostics.projected_points,
            diagnostics.maximum_final_residual,
        );
        for path in &level.paths {
            for point in &path.points {
                let residual = (prepared.value(point.as_array())? - level.value).abs();
                assert!(residual <= geometry.regularization.unwrap().projection_tolerance);
            }
        }
    }

    // The convenience workflow prepares an equivalent cubic field once for the
    // call; use it when pointwise alpha inspection and evaluator reuse are not needed.
    let convenience = IrregularContourSet::compute(
        &field,
        &levels,
        IrregularContourOptions {
            interpolation: IrregularContourInterpolation::CubicAlpha(cubic_options),
            geometry,
        },
    )?;
    assert_eq!(contours, convenience);
    Ok(())
}
