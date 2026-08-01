//! Build and evaluate an irregular Delaunay-backed ternary scalar field.

use ternary_contours::{
    IrregularTernaryMesh, IrregularTernaryScalarField, PreparedIrregularTernaryField,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Samples stay in semantic A/B/C order. The mesh embeds them privately in
    // an equilateral logical plane and owns the resulting immutable topology.
    let mesh = IrregularTernaryMesh::new([
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.57, 0.28, 0.15],
        [0.18, 0.61, 0.21],
        [0.23, 0.16, 0.61],
        [0.31, 0.42, 0.27],
    ])?;
    println!(
        "vertices={}, edges={}, triangles={}, hull edges={}",
        mesh.vertex_count(),
        mesh.edge_count(),
        mesh.triangle_count(),
        mesh.boundary_edges().count(),
    );
    for edge in mesh.edges() {
        println!(
            "edge {:?}: {:?}, incident={:?}",
            edge.id, edge.vertices, edge.triangles
        );
    }

    let field = IrregularTernaryScalarField::from_fn(mesh, |[a, b, c]| {
        2.0 * a - 3.0 * b + 5.0 * c + 0.4 * a * b
    })?;
    let evaluator = PreparedIrregularTernaryField::new(&field);

    let point = [0.23, 0.31, 0.46];
    let location = field.mesh().locate(point)?;
    println!("triangle: {:?}", location.triangle);
    println!("barycentric: {:?}", location.barycentric);
    let sample = evaluator.evaluate_at_location(&location)?;
    println!("linear value: {}", sample.value);
    println!("global (a, b) gradient: {:?}", sample.gradient_ab);

    let points = [[0.2, 0.3, 0.5], [0.5, 0.25, 0.25], [0.0, 1.0, 0.0]];
    let lazy_values: Result<Vec<_>, _> = evaluator.values(points).collect();
    println!("lazy batch: {:?}", lazy_values?);

    let mut output = vec![0.0; points.len()];
    evaluator.values_into(&points, &mut output)?;
    println!("allocation-reusing batch: {output:?}");
    Ok(())
}
