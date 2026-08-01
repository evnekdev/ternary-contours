//! Prepare and inspect an irregular self-consistent cubic-alpha field.

use ternary_contours::{
    BinaryExtrapolation, CubicAlphaMethod, CubicBoundaryPolicy, InterpolatedIrregularTernaryField,
    IrregularCubicAlphaOptions, IrregularFieldInterpolation, IrregularTernaryMesh,
    IrregularTernaryScalarField,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mesh = IrregularTernaryMesh::new([
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.57, 0.28, 0.15],
        [0.18, 0.61, 0.21],
        [0.23, 0.16, 0.61],
        [0.31, 0.42, 0.27],
    ])?;
    let field = IrregularTernaryScalarField::from_fn(mesh, |[a, b, c]| {
        1.2 * a * a - 0.8 * b * b + 0.45 * c * c + 0.6 * a * b - 0.3 * b * c
    })?;

    // Muggianu and Kohler are interior-continuation policies inside the same
    // cubic-alpha family. Try `Kohler` here to select that continuation.
    let options = IrregularCubicAlphaOptions {
        method: CubicAlphaMethod::Pchip,
        boundary_policy: CubicBoundaryPolicy::LinearFallback,
        extrapolation: BinaryExtrapolation::Muggianu,
        ..IrregularCubicAlphaOptions::default()
    };
    let evaluator = InterpolatedIrregularTernaryField::new(
        &field,
        IrregularFieldInterpolation::CubicAlpha(options),
    )?;

    let composition = [0.23, 0.31, 0.46];
    let location = field.mesh().locate(composition)?;
    let sample = evaluator.evaluate_at_location(&location)?;
    println!("triangle: {:?}", sample.location.triangle.id);
    println!("barycentric: {:?}", sample.location.barycentric);
    println!("cubic value: {}", sample.value);
    println!("global (df/da, df/db): {:?}", sample.gradient_ab);

    let diagnostics = evaluator.cubic_diagnostics().expect("cubic model selected");
    println!(
        "alpha solve: {:?}, sweeps={}, complete={}, fallback={}",
        diagnostics.convergence,
        diagnostics.sweep_count,
        diagnostics.complete_stencil_edges,
        diagnostics.linear_fallback_edges,
    );
    for edge in field.mesh().edges() {
        println!(
            "edge {:?}: {:?}",
            edge.id,
            evaluator
                .cubic_alpha_interval(edge.id)
                .expect("dense interval")
        );
    }

    let points = [[0.2, 0.3, 0.5], [0.5, 0.25, 0.25], [0.0, 1.0, 0.0]];
    let values = evaluator.values(points).collect::<Result<Vec<_>, _>>()?;
    println!("lazy batch: {values:?}");
    let mut output = vec![0.0; points.len()];
    evaluator.values_into(&points, &mut output)?;
    println!("allocation-reusing batch: {output:?}");
    Ok(())
}
