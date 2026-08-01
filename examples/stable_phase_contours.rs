use ternary_contours::{
    FieldInterpolation, IrregularFieldInterpolation, IrregularTernaryMesh,
    IrregularTernaryScalarField, PreparedStablePhaseEnsemble, RegularTernaryScalarField,
    StableContourQuantity, StableGridOptions, StableGridVerification, StablePhaseId,
    StablePhaseSource, StableScalarSource,
};

fn regular(field: &RegularTernaryScalarField) -> StableScalarSource<'_> {
    StableScalarSource::regular(field, FieldInterpolation::Linear)
}

fn phase<'a>(id: u32, field: &'a RegularTernaryScalarField) -> StablePhaseSource<'a> {
    StablePhaseSource::new(StablePhaseId(id), regular(field))
}

fn mesh() -> Result<IrregularTernaryMesh, Box<dyn std::error::Error>> {
    Ok(IrregularTernaryMesh::new([
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.62, 0.23, 0.15],
        [0.19, 0.64, 0.17],
        [0.16, 0.21, 0.63],
        [0.31, 0.41, 0.28],
    ])?)
}

fn report(
    label: &str,
    prepared: &PreparedStablePhaseEnsemble<'_>,
    levels: &[f64],
) -> Result<(), Box<dyn std::error::Error>> {
    let contours = prepared.contours(levels)?;
    println!("\n{label}");
    println!(
        "groups={} fields={} locations={} reuse={} evaluations={}",
        contours.diagnostics.geometry_group_count,
        contours.diagnostics.source_scalar_layer_count,
        contours.diagnostics.source_point_location_count,
        contours.diagnostics.reused_source_locations,
        contours.diagnostics.source_scalar_evaluation_count,
    );
    for level in contours.levels {
        println!(
            "level={} paths={} junctions={}",
            level.value,
            level.paths.len(),
            level.junctions.len()
        );
        for path in level.paths {
            println!(
                "  phase={:?} points={} closed={} {:?}->{:?}",
                path.phase,
                path.points.len(),
                path.closed,
                path.start_junction,
                path.end_junction
            );
        }
        for junction in level.junctions {
            println!(
                "  {:?} {:?} at {:?}",
                junction.kind,
                junction.phases,
                junction.point.as_array()
            );
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = StableGridOptions {
        subdivisions: 12,
        ..StableGridOptions::default()
    };
    let height_a = RegularTernaryScalarField::from_fn(12, |[a, _, _]| a)?;
    let height_b = RegularTernaryScalarField::from_fn(12, |[_, b, _]| b)?;
    let regular_prepared = PreparedStablePhaseEnsemble::new(
        [phase(1, &height_a), phase(2, &height_b)],
        StableContourQuantity::Height,
        options,
    )?;
    report("regular height envelope", &regular_prepared, &[0.3, 0.4])?;

    let irregular_height =
        IrregularTernaryScalarField::from_fn(mesh()?, |[a, b, c]| 0.2 + 0.5 * a - b + c)?;
    let irregular_source =
        StableScalarSource::irregular(&irregular_height, IrregularFieldInterpolation::Linear);
    let irregular = PreparedStablePhaseEnsemble::new(
        [StablePhaseSource::new(StablePhaseId(3), irregular_source)],
        StableContourQuantity::Height,
        options,
    )?;
    report("irregular source", &irregular, &[0.4])?;
    let mixed = PreparedStablePhaseEnsemble::new(
        [
            phase(1, &height_a),
            StablePhaseSource::new(StablePhaseId(3), irregular_source),
        ],
        StableContourQuantity::Height,
        options,
    )?;
    report("mixed source geometries", &mixed, &[0.35])?;

    // A=B at 0.2 is metastable because phase C is higher.
    let high_c = RegularTernaryScalarField::from_fn(12, |_| 0.8)?;
    let metastable = PreparedStablePhaseEnsemble::new(
        [phase(1, &height_a), phase(2, &height_b), phase(3, &high_c)],
        StableContourQuantity::Height,
        options,
    )?;
    report("metastable equality removed", &metastable, &[0.2])?;

    // Phase 40 wins no triangle vertex but owns a central clipped polygon.
    let base = |[a, b, _]: [f64; 3]| a + 2.0 * b;
    let centre = RegularTernaryScalarField::from_fn(1, base)?;
    let outside_a = RegularTernaryScalarField::from_fn(1, |p @ [a, _, _]| base(p) + a - 0.6)?;
    let outside_b = RegularTernaryScalarField::from_fn(1, |p @ [_, b, _]| base(p) + b - 0.6)?;
    let outside_c = RegularTernaryScalarField::from_fn(1, |p @ [_, _, c]| base(p) + c - 0.6)?;
    let narrow = PreparedStablePhaseEnsemble::new(
        [
            phase(10, &outside_a),
            phase(20, &outside_b),
            phase(30, &outside_c),
            phase(40, &centre),
        ],
        StableContourQuantity::Height,
        StableGridOptions {
            subdivisions: 1,
            ..StableGridOptions::default()
        },
    )?;
    report("interior narrow stable phase", &narrow, &[1.0])?;
    println!(
        "interior polygons without vertex winner={}",
        narrow
            .diagnostics()
            .interior_stable_polygons_without_vertex_winner
    );

    // Secondary values do not affect ownership and need not meet across phases.
    let secondary_a = RegularTernaryScalarField::from_fn(12, |[_, _, c]| c)?;
    let secondary_b = RegularTernaryScalarField::from_fn(12, |[a, _, c]| c + 0.2 * a)?;
    let secondary = PreparedStablePhaseEnsemble::new(
        [
            phase(1, &height_a).with_secondary(regular(&secondary_a)),
            phase(2, &height_b).with_secondary(regular(&secondary_b)),
        ],
        StableContourQuantity::Secondary,
        options,
    )?;
    report("phase-specific secondary contours", &secondary, &[0.2])?;

    // Verification is a practical resolution check, not interval certification.
    let nonlinear = RegularTernaryScalarField::from_fn(8, |[a, b, c]| a * a + 0.5 * b * b - c * c)?;
    let refined = PreparedStablePhaseEnsemble::new(
        [phase(1, &nonlinear)],
        StableContourQuantity::Height,
        StableGridOptions {
            subdivisions: 1,
            verification: StableGridVerification {
                enabled: true,
                maximum_refinement_passes: 3,
                maximum_subdivisions: 8,
                height_error_tolerance: 1.0e-12,
                ..StableGridVerification::default()
            },
            ..StableGridOptions::default()
        },
    )?;
    println!(
        "\nrefinement passes={} final={} residuals={:?}",
        refined.diagnostics().refinement_passes,
        refined.diagnostics().final_subdivisions,
        refined
            .diagnostics()
            .verification_passes
            .iter()
            .map(|pass| pass.maximum_height_approximation_error)
            .collect::<Vec<_>>()
    );
    Ok(())
}
