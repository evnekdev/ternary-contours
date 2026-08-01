use std::collections::BTreeSet;

use ternary_contours::{
    FieldInterpolation, LiquidusFieldSpec, PreparedStablePhaseEnsemble, RegularTernaryScalarField,
    StableContourQuantity, StableGridOptions, StablePhaseId, StablePhaseSource, StableScalarSource,
    TernaryCoordinate,
};

fn options(subdivisions: usize) -> StableGridOptions {
    StableGridOptions {
        subdivisions,
        value_tolerance: 1.0e-9,
        stability_tolerance: 1.0e-9,
        geometry_tolerance: 1.0e-9,
        parameter_tolerance: 1.0e-12,
        ..StableGridOptions::default()
    }
}

fn run(
    specs: &[LiquidusFieldSpec],
    subdivisions: usize,
    levels: &[f64],
) -> ternary_contours::StableContourSet {
    let fields: Vec<_> = specs
        .iter()
        .map(|spec| spec.sample(subdivisions).unwrap())
        .collect();
    let phases = specs
        .iter()
        .zip(&fields)
        .map(|(spec, field)| {
            StablePhaseSource::new(
                spec.phase,
                StableScalarSource::regular(field, FieldInterpolation::Linear),
            )
        })
        .collect::<Vec<_>>();
    PreparedStablePhaseEnsemble::new(phases, StableContourQuantity::Height, options(subdivisions))
        .unwrap()
        .contours(levels)
        .unwrap()
}

fn phase_ids(set: &ternary_contours::StableContourSet) -> BTreeSet<StablePhaseId> {
    set.levels
        .iter()
        .flat_map(|level| level.paths.iter().map(|path| path.phase))
        .collect()
}

fn assert_geometry(set: &ternary_contours::StableContourSet) {
    for level in &set.levels {
        for path in &level.paths {
            assert!(!path.points.is_empty());
            for point in &path.points {
                let [a, b, c] = point.as_array();
                assert!(a.is_finite() && b.is_finite() && c.is_finite());
                assert!((a + b + c - 1.0).abs() < 1.0e-8);
                assert!(a >= -1.0e-8 && b >= -1.0e-8 && c >= -1.0e-8);
            }
            for pair in path.points.windows(2) {
                assert_ne!(pair[0], pair[1]);
            }
        }
    }
}

fn corner_specs() -> Vec<LiquidusFieldSpec> {
    vec![
        LiquidusFieldSpec::corner_a(StablePhaseId(1), 100.0, 80.0),
        LiquidusFieldSpec::corner_b(StablePhaseId(2), 100.0, 80.0),
        LiquidusFieldSpec::corner_c(StablePhaseId(3), 100.0, 80.0),
    ]
}

#[test]
fn symmetric_corner_gallery_case_has_all_phase_owners_and_valid_geometry() {
    let set = run(&corner_specs(), 24, &[25.0, 40.0, 55.0, 70.0, 80.0, 90.0]);
    assert_eq!(
        phase_ids(&set),
        [StablePhaseId(1), StablePhaseId(2), StablePhaseId(3)].into()
    );
    assert!(set.levels.iter().any(|level| !level.junctions.is_empty()));
    assert_geometry(&set);
}

#[test]
fn narrow_phase_becomes_visible_after_sampling_refinement() {
    let specs = vec![
        LiquidusFieldSpec::isotropic(
            StablePhaseId(1),
            TernaryCoordinate::new(0.34, 0.33, 0.33),
            100.0,
            52.0,
            4.0,
        ),
        LiquidusFieldSpec::isotropic(
            StablePhaseId(2),
            TernaryCoordinate::new(0.34, 0.33, 0.33),
            100.8,
            900.0,
            1_200.0,
        ),
        LiquidusFieldSpec::corner_c(StablePhaseId(3), 99.0, 60.0),
    ];
    let coarse = run(&specs, 8, &[100.2]);
    let refined = run(&specs, 32, &[100.2]);
    assert!(!phase_ids(&coarse).contains(&StablePhaseId(2)));
    assert!(phase_ids(&refined).contains(&StablePhaseId(2)));
    assert_geometry(&coarse);
    assert_geometry(&refined);
}

#[test]
fn metastable_pairwise_equality_is_not_rendered_as_stable_transition() {
    let specs = vec![
        LiquidusFieldSpec::corner_a(StablePhaseId(1), 100.0, 46.0),
        LiquidusFieldSpec::corner_b(StablePhaseId(2), 100.0, 46.0),
        LiquidusFieldSpec::isotropic(
            StablePhaseId(3),
            TernaryCoordinate::new(0.34, 0.33, 0.33),
            108.0,
            34.0,
            4.0,
        ),
    ];
    let set = run(&specs, 24, &[70.0, 82.0, 92.0, 99.0]);
    assert_eq!(phase_ids(&set), [StablePhaseId(3)].into());
    assert_geometry(&set);
}

#[test]
fn repeated_generation_is_deterministic_for_mixed_topology_case() {
    let specs = vec![
        LiquidusFieldSpec::corner_a(StablePhaseId(1), 106.0, 70.0),
        LiquidusFieldSpec::edge_bc(StablePhaseId(2), 0.44, 104.0, 92.0),
        LiquidusFieldSpec::isotropic(
            StablePhaseId(3),
            TernaryCoordinate::new(0.30, 0.36, 0.34),
            105.0,
            115.0,
            110.0,
        ),
        LiquidusFieldSpec::isotropic(
            StablePhaseId(4),
            TernaryCoordinate::new(0.50, 0.28, 0.22),
            103.0,
            125.0,
            160.0,
        ),
    ];
    let first = run(&specs, 30, &[35.0, 52.0, 68.0, 80.0, 90.0, 98.0]);
    let second = run(&specs, 30, &[35.0, 52.0, 68.0, 80.0, 90.0, 98.0]);
    assert_eq!(first, second);
    assert_geometry(&first);
}

#[test]
fn secondary_gallery_case_keeps_height_ownership_and_boundary_contacts_independent() {
    let heights = [
        LiquidusFieldSpec::corner_a(StablePhaseId(1), 104.0, 58.0),
        LiquidusFieldSpec::corner_b(StablePhaseId(2), 103.0, 62.0),
        LiquidusFieldSpec::isotropic(
            StablePhaseId(3),
            TernaryCoordinate::new(0.34, 0.33, 0.33),
            105.0,
            105.0,
            80.0,
        ),
    ];
    let height_fields: Vec<RegularTernaryScalarField> = heights
        .iter()
        .map(|spec| spec.sample(28).unwrap())
        .collect();
    let secondary_fields: Vec<RegularTernaryScalarField> = heights
        .iter()
        .map(|spec| {
            let phase = spec.phase;
            RegularTernaryScalarField::from_fn(28, move |[a, b, c]| match phase.0 {
                1 => 0.05 + 0.85 * b + 0.10 * c,
                2 => 0.08 + 0.20 * a + 0.72 * c,
                _ => 0.12 + 0.62 * a + 0.28 * b,
            })
            .unwrap()
        })
        .collect();
    let phases = heights
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            StablePhaseSource::new(
                spec.phase,
                StableScalarSource::regular(&height_fields[index], FieldInterpolation::Linear),
            )
            .with_secondary(StableScalarSource::regular(
                &secondary_fields[index],
                FieldInterpolation::Linear,
            ))
        })
        .collect::<Vec<_>>();
    let set =
        PreparedStablePhaseEnsemble::new(phases, StableContourQuantity::Secondary, options(28))
            .unwrap()
            .contours(&[0.20, 0.35, 0.50, 0.65, 0.80])
            .unwrap();
    assert!(
        set.levels
            .iter()
            .flat_map(|level| level.paths.iter())
            .any(|path| path.phase == StablePhaseId(1))
    );
    assert!(
        set.levels
            .iter()
            .flat_map(|level| level.paths.iter())
            .any(|path| path.phase == StablePhaseId(2))
    );
    assert!(
        set.levels
            .iter()
            .flat_map(|level| level.junctions.iter())
            .all(|junction| matches!(
                junction.kind,
                ternary_contours::StableContourJunctionKind::StableBoundaryContact
            ))
    );
    assert_geometry(&set);
}
