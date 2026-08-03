use std::{fs, process::Command};

use ternary_contours_cli::{ProjectionOptions, calculate_projection, parse_path};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures");

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ternary-contours-cli"))
}

#[test]
fn valid_fixtures_parse_deterministically() {
    for fixture in [
        "minimal-regular.tct",
        "regular-guidance.tct",
        "regular-authoritative-shuffled.tct",
        "different-subdivisions.tct",
        "irregular-phase-grids.tct",
        "partial-phase-domain.tct",
        "secondary-property.tct",
        "hidden-metastable-equality.tct",
        "interior-invariant.tct",
        "binary-invariants.tct",
    ] {
        let first = parse_path(format!("{FIXTURES}/{fixture}")).unwrap();
        let second = parse_path(format!("{FIXTURES}/{fixture}")).unwrap();
        assert_eq!(first, second, "{fixture}");
    }
}

#[test]
fn malformed_fixtures_fail_with_nonzero_exit_status() {
    for fixture in [
        "malformed-row-width.tct",
        "duplicate-regular-point.tct",
        "missing-regular-point.tct",
        "invalid-irregular-composition.tct",
        "unknown-phase-property.tct",
        "unsupported-version.tct",
    ] {
        let output = binary()
            .args(["validate", &format!("{FIXTURES}/{fixture}")])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{fixture}");
    }
}

#[test]
fn inspect_and_static_outputs_are_created() {
    let input = format!("{FIXTURES}/minimal-regular.tct");
    let inspect = binary().args(["inspect", &input]).output().unwrap();
    assert!(inspect.status.success());
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("Format: TCT 1.0"));
    let folder = std::env::temp_dir().join(format!("ternary-contours-cli-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    for extension in ["svg", "png"] {
        let output = folder.join(format!("projection.{extension}"));
        let result = binary()
            .args(["plot", &input, "--output", output.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(fs::metadata(output).unwrap().len() > 0);
    }
}

#[test]
fn projection_exposes_invariants_and_univariants() {
    let dataset = parse_path(format!("{FIXTURES}/minimal-regular.tct")).unwrap();
    let projection = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
    assert!(projection.stable_boundaries.nodes.len() >= 3);
    assert!(!projection.stable_boundaries.univariants.is_empty());
    assert!(
        projection
            .stable_contours
            .levels
            .iter()
            .flat_map(|level| &level.paths)
            .any(|path| path.phase.0 == 10)
    );
}

#[cfg(not(feature = "viewer"))]
#[test]
fn view_without_feature_reports_enablement_guidance() {
    let output = binary()
        .args(["view", &format!("{FIXTURES}/minimal-regular.tct")])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires the optional `viewer` feature")
    );
}

#[test]
fn composition_and_template_commands_emit_deterministic_tsv() {
    let output = binary()
        .args([
            "compositions",
            "--subdivisions",
            "2",
            "--components",
            "A,B,C",
            "--header",
            "--precision",
            "1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "A\tB\tC\n0.0\t0.0\t1.0\n0.0\t0.5\t0.5\n0.0\t1.0\t0.0\n0.5\t0.0\t0.5\n0.5\t0.5\t0.0\n1.0\t0.0\t0.0\n"
    );

    let folder =
        std::env::temp_dir().join(format!("ternary-contours-template-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let regular = folder.join("regular.tct");
    let result = binary()
        .args([
            "template",
            "regular",
            "--subdivisions",
            "2",
            "--components",
            "A,B,C",
            "--fields",
            "alpha.T,beta.T",
            "--output",
            regular.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    let template = fs::read_to_string(&regular).unwrap();
    assert!(template.contains("subdivisions = 2"));
    assert_eq!(template.lines().filter(|line| *line == "NA\tNA").count(), 6);

    let irregular = binary()
        .args([
            "template",
            "irregular",
            "--components",
            "A,B,C",
            "--fields",
            "alpha.T",
            "--style",
            "tsv-header",
        ])
        .output()
        .unwrap();
    assert!(irregular.status.success());
    assert_eq!(
        String::from_utf8_lossy(&irregular.stdout),
        "A\tB\tC\talpha.T\n"
    );
}
