//! Inspect Delaunay-only mesh quality and shared scalar-field metrics.

use ternary_contours::{
    DerivedFieldQuantity, InterpolatedIrregularTernaryField, IrregularFieldInterpolation,
    IrregularTernaryMesh, IrregularTernaryScalarField, LocalQuadraticOptions,
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

    // Delaunay-only quality and topology records use canonical logical geometry.
    let mesh_metrics = mesh.metrics();
    println!("mesh,vertices,edges,triangles,hull_edges,hull_area");
    println!(
        "summary,{},{},{},{},{}",
        mesh_metrics.summary.vertex_count,
        mesh_metrics.summary.edge_count,
        mesh_metrics.summary.triangle_count,
        mesh_metrics.summary.hull_edge_count,
        mesh_metrics.summary.convex_hull_area,
    );
    for edge in &mesh_metrics.edges {
        println!(
            "edge,{:?},{},{:?}",
            edge.edge.id, edge.length, edge.adjacent_area_ratio
        );
    }

    let field =
        IrregularTernaryScalarField::from_fn(mesh, |[a, b, c]| a * a - 0.5 * b * c + 0.2 * a * b)?;
    let evaluator =
        InterpolatedIrregularTernaryField::new(&field, IrregularFieldInterpolation::Linear)?;

    let point = [0.31, 0.34, 0.35];
    let sample = evaluator.evaluate(point)?;
    println!("sample,value,reduced_a,reduced_b,logical_x,logical_y,gradient_norm");
    println!(
        "point,{},{},{},{},{},{}",
        sample.value,
        sample.gradient_ab[0],
        sample.gradient_ab[1],
        sample.gradient_logical_xy()[0],
        sample.gradient_logical_xy()[1],
        sample.gradient_norm(),
    );
    println!(
        "location,triangle={:?},barycentric={:?}",
        sample.location.triangle.id, sample.location.barycentric
    );

    // The derived adapter shares the already prepared evaluator.
    let gradient_norm = evaluator.derived(DerivedFieldQuantity::GradientNorm);
    let values = gradient_norm
        .values([[0.23, 0.31, 0.46], [0.31, 0.34, 0.35]])
        .collect::<Result<Vec<_>, _>>()?;
    println!("derived_gradient_norms,{values:?}");

    let field_metrics = evaluator.metrics()?;
    println!("field,triangles,interior_jumps,mean_gradient_norm");
    println!(
        "summary,{},{},{:?}",
        field_metrics.triangles.len(),
        field_metrics.gradient_jumps.len(),
        field_metrics.distributions.gradient_norms.mean,
    );
    for jump in &field_metrics.gradient_jumps {
        println!(
            "jump,{:?},{},{},{}",
            jump.edge.id, jump.jump.magnitude, jump.jump.tangential_jump, jump.jump.normal_jump
        );
    }

    // Both backends use the same logical-plane QR estimator and result type.
    for vertex in field.mesh().vertex_ids() {
        if let Ok(curvature) =
            field.local_quadratic_estimate(vertex, LocalQuadraticOptions::default())
        {
            println!(
                "curvature,{:?},{:?},{:?},{}",
                vertex,
                curvature.hessian_logical_xy,
                curvature.eigenvalues,
                curvature.condition_estimate
            );
        }
    }
    for alignment in field.triangle_field_alignment(LocalQuadraticOptions::default()) {
        println!(
            "alignment,{:?},{:?}",
            alignment.triangle, alignment.alignment
        );
    }
    Ok(())
}
