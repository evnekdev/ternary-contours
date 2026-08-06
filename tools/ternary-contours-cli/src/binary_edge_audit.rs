//! Exact binary-edge overlap audit for regular linear liquidus fields.
//!
//! This developer utility deliberately shares the runtime linear evaluator with
//! projection preparation. Its independent root isolation uses only finite
//! source-edge segments, so classified gaps cannot accidentally form brackets.

use std::fmt::Write;

use ternary_contours::{BinaryBoundary, StablePhaseEvaluation, StablePhaseId};

use crate::{
    ProjectionOptions, RegularTabulatedGrid, SourceInterpolation, TabulatedField, TabulatedGrid,
    TabulatedTernaryDataset, TabulatedValue, projection::evaluate_regular_linear_field,
};

const ZERO_TOLERANCE: f64 = 1.0e-10;

#[derive(Clone, Debug, PartialEq)]
pub struct BinaryEdgeAudit {
    pub edge: BinaryBoundary,
    pub component_start: String,
    pub component_end: String,
    pub phase_one: AuditPhase,
    pub phase_two: AuditPhase,
    pub raw_rows: Vec<BinaryEdgeAuditRow>,
    pub intervals: Vec<BinaryOverlapInterval>,
    pub effective_samples: Vec<EffectiveEdgeSample>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditPhase {
    pub id: StablePhaseId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BinaryEdgeAuditRow {
    pub source_row: usize,
    pub parameter: f64,
    pub composition: [f64; 3],
    pub phase_one: TabulatedValue,
    pub phase_two: TabulatedValue,
    pub difference: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BinaryOverlapInterval {
    pub start_parameter: f64,
    pub end_parameter: f64,
    pub start_difference: f64,
    pub end_difference: f64,
    pub minimum_difference: f64,
    pub maximum_difference: f64,
    pub sign_change: bool,
    pub exact_zero: bool,
    pub root: Option<LinearEdgeRoot>,
    pub termination: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearEdgeRoot {
    pub parameter: f64,
    pub composition: [f64; 3],
    pub residual: f64,
    pub stable: RootStability,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RootStability {
    StableCandidate {
        phase_ids: Vec<StablePhaseId>,
    },
    Metastable {
        stable_phase_ids: Vec<StablePhaseId>,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveEdgeSample {
    pub parameter: f64,
    pub phase_one: EvaluatedCell,
    pub phase_two: EvaluatedCell,
    pub difference: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluatedCell {
    pub value: Option<f64>,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BinaryEdgeAuditReport {
    pub edges: Vec<BinaryEdgeAudit>,
}

impl BinaryEdgeAuditReport {
    pub fn to_csv(&self) -> String {
        let mut output = String::from(
            "edge,parameter,a,b,c,phase_1,phase_1_state,phase_1_value,phase_2,phase_2_state,phase_2_value,difference,source_row\n",
        );
        for edge in &self.edges {
            for row in &edge.raw_rows {
                let _ = writeln!(
                    output,
                    "{:?},{:.12},{:.12},{:.12},{:.12},{},{},{},{},{},{},{},{}",
                    edge.edge,
                    row.parameter,
                    row.composition[0],
                    row.composition[1],
                    row.composition[2],
                    edge.phase_one.name,
                    row.phase_one.state.token(),
                    format_value(row.phase_one.defined_value()),
                    edge.phase_two.name,
                    row.phase_two.state.token(),
                    format_value(row.phase_two.defined_value()),
                    format_value(row.difference),
                    row.source_row + 1,
                );
            }
        }
        output
    }

    pub fn to_markdown(&self) -> String {
        let mut output = String::from("# CaO–PbO–ZnO binary-edge audit\n\n");
        output.push_str("Linear source interpolation; roots are isolated exactly on connected finite source-edge segments.\n\n");
        for edge in &self.edges {
            let _ = writeln!(
                output,
                "## {}–{} ({:?}): {}.T − {}.T\n",
                edge.component_start,
                edge.component_end,
                edge.edge,
                edge.phase_one.name,
                edge.phase_two.name
            );
            if edge.intervals.is_empty() {
                output.push_str(
                    "**UNSUPPORTED:** no connected interval has both finite phase fields.\n\n",
                );
                continue;
            }
            output.push_str("| finite interval | D(start) | D(end) | min D | max D | sign change | root | classification |\n| --- | ---: | ---: | ---: | ---: | --- | --- | --- |\n");
            for interval in &edge.intervals {
                let root = interval
                    .root
                    .as_ref()
                    .map_or_else(|| "—".to_owned(), |root| format!("{:.12}", root.parameter));
                let classification = interval.root.as_ref().map_or_else(
                    || "not confirmed".to_owned(),
                    |root| match &root.stable {
                        RootStability::StableCandidate { .. } => {
                            "stable binary invariant candidate".to_owned()
                        }
                        RootStability::Metastable { .. } => {
                            "metastable pairwise equality".to_owned()
                        }
                        RootStability::Unsupported { reason } => format!("unsupported: {reason}"),
                    },
                );
                let _ = writeln!(
                    output,
                    "| [{:.12}, {:.12}] | {:.9} | {:.9} | {:.9} | {:.9} | {} | {} | {} |",
                    interval.start_parameter,
                    interval.end_parameter,
                    interval.start_difference,
                    interval.end_difference,
                    interval.minimum_difference,
                    interval.maximum_difference,
                    if interval.sign_change { "yes" } else { "no" },
                    root,
                    classification,
                );
            }
            let roots = edge
                .intervals
                .iter()
                .filter(|interval| interval.root.is_some())
                .count();
            let sign_changes = edge
                .intervals
                .iter()
                .filter(|interval| interval.sign_change)
                .count();
            let conclusion = if sign_changes > 0 {
                "**CONFIRMED:** a continuous finite interval exists and D changes sign."
            } else {
                "**NOT CONFIRMED:** finite overlap exists but no sign change was found."
            };
            let _ = writeln!(output, "\n{conclusion} Roots: {roots}.\n");
        }
        output
    }
}

/// Audit the three physical binaries of the named CaO–PbO–ZnO liquidus data.
/// The normal Viewer configuration is linear, so this intentionally refuses a
/// cubic request until a polynomial root-isolator is added.
pub fn audit_cao_pbo_zno_binary_edges(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
) -> Result<BinaryEdgeAuditReport, String> {
    if options.interpolation.source != SourceInterpolation::Linear {
        return Err(
            "binary-edge audit currently supports exact Linear source interpolation only".into(),
        );
    }
    let grid = dataset
        .grids
        .iter()
        .find_map(|grid| match grid {
            TabulatedGrid::Regular(grid) => Some(grid),
            TabulatedGrid::Irregular(_) => None,
        })
        .ok_or_else(|| "binary-edge audit requires a regular grid".to_owned())?;
    let lime = named_phase(dataset, "Lime")?;
    let pbo = named_phase(dataset, "PbO")?;
    let zno = named_phase(dataset, "ZnO")?;
    Ok(BinaryEdgeAuditReport {
        edges: vec![
            audit_edge(dataset, grid, BinaryBoundary::Ab, lime.clone(), pbo.clone())?,
            audit_edge(dataset, grid, BinaryBoundary::Bc, pbo, zno.clone())?,
            audit_edge(dataset, grid, BinaryBoundary::Ca, zno, lime)?,
        ],
    })
}

fn named_phase(dataset: &TabulatedTernaryDataset, name: &str) -> Result<AuditPhase, String> {
    dataset
        .phases
        .iter()
        .find(|phase| phase.name == name)
        .map(|phase| AuditPhase {
            id: phase.id,
            name: phase.name.clone(),
        })
        .ok_or_else(|| format!("audit requires phase `{name}`"))
}

fn audit_edge(
    dataset: &TabulatedTernaryDataset,
    grid: &RegularTabulatedGrid,
    edge: BinaryBoundary,
    phase_one: AuditPhase,
    phase_two: AuditPhase,
) -> Result<BinaryEdgeAudit, String> {
    let field_one = field(grid, phase_one.id)?;
    let field_two = field(grid, phase_two.id)?;
    let mut raw_rows = Vec::new();
    for step in 0..=grid.subdivisions {
        let parameter = step as f64 / grid.subdivisions as f64;
        let composition: [f64; 3] = edge
            .composition(parameter)
            .expect("grid parameter is valid")
            .into();
        let source_row = grid
            .compositions
            .iter()
            .position(|candidate| close_composition(*candidate, composition))
            .ok_or_else(|| format!("canonical row is missing for {edge:?} at {parameter}"))?;
        let first = field_one.values[source_row].clone();
        let second = field_two.values[source_row].clone();
        let difference = finite_difference(&first, &second);
        raw_rows.push(BinaryEdgeAuditRow {
            source_row,
            parameter,
            composition,
            phase_one: first,
            phase_two: second,
            difference,
        });
    }
    let intervals = connected_intervals(dataset, grid, edge, &phase_one, &phase_two, &raw_rows);
    let effective_samples = raw_rows
        .iter()
        .map(|row| {
            let first = evaluated(evaluate_regular_linear_field(
                grid,
                field_one,
                row.composition,
            ));
            let second = evaluated(evaluate_regular_linear_field(
                grid,
                field_two,
                row.composition,
            ));
            let difference = first
                .value
                .zip(second.value)
                .map(|(left, right)| left - right);
            EffectiveEdgeSample {
                parameter: row.parameter,
                phase_one: first,
                phase_two: second,
                difference,
            }
        })
        .collect();
    let (start, end) = edge_components(dataset, edge);
    Ok(BinaryEdgeAudit {
        edge,
        component_start: start,
        component_end: end,
        phase_one,
        phase_two,
        raw_rows,
        intervals,
        effective_samples,
    })
}

fn connected_intervals(
    dataset: &TabulatedTernaryDataset,
    grid: &RegularTabulatedGrid,
    edge: BinaryBoundary,
    phase_one: &AuditPhase,
    phase_two: &AuditPhase,
    rows: &[BinaryEdgeAuditRow],
) -> Vec<BinaryOverlapInterval> {
    let mut result = Vec::new();
    let mut start = None;
    for index in 0..=rows.len() {
        let finite = rows.get(index).is_some_and(|row| row.difference.is_some());
        if finite && start.is_none() {
            start = Some(index);
        }
        if (!finite || index == rows.len()) && start.is_some() {
            let first = start.take().expect("set above");
            let last = index - 1;
            let segment = &rows[first..=last];
            let values = segment
                .iter()
                .filter_map(|row| row.difference)
                .collect::<Vec<_>>();
            let root = segment
                .windows(2)
                .find_map(|pair| {
                    linear_root(
                        pair[0].parameter,
                        pair[0].difference.unwrap(),
                        pair[1].parameter,
                        pair[1].difference.unwrap(),
                    )
                })
                .map(|parameter| {
                    let composition: [f64; 3] = edge
                        .composition(parameter)
                        .expect("isolated root remains on edge")
                        .into();
                    let residual =
                        evaluate_raw_difference(grid, phase_one.id, phase_two.id, composition)
                            .unwrap_or(f64::NAN);
                    LinearEdgeRoot {
                        parameter,
                        composition,
                        residual,
                        stable: classify_root(
                            dataset,
                            grid,
                            composition,
                            phase_one.id,
                            phase_two.id,
                        ),
                    }
                });
            let exact_zero = values.iter().any(|value| value.abs() <= ZERO_TOLERANCE);
            result.push(BinaryOverlapInterval {
                start_parameter: segment.first().unwrap().parameter,
                end_parameter: segment.last().unwrap().parameter,
                start_difference: segment.first().unwrap().difference.unwrap(),
                end_difference: segment.last().unwrap().difference.unwrap(),
                minimum_difference: values.iter().copied().reduce(f64::min).unwrap(),
                maximum_difference: values.iter().copied().reduce(f64::max).unwrap(),
                sign_change: values.windows(2).any(|pair| pair[0] * pair[1] < 0.0),
                exact_zero,
                root,
                termination: None,
            });
        }
    }
    result
}

fn classify_root(
    dataset: &TabulatedTernaryDataset,
    grid: &RegularTabulatedGrid,
    composition: [f64; 3],
    first: StablePhaseId,
    second: StablePhaseId,
) -> RootStability {
    let values = dataset
        .phases
        .iter()
        .filter_map(|phase| {
            field(grid, phase.id).ok().and_then(|field| {
                defined_value(evaluate_regular_linear_field(grid, field, composition))
                    .map(|value| (phase.id, value))
            })
        })
        .collect::<Vec<_>>();
    let Some(maximum) = values.iter().map(|(_, value)| *value).reduce(f64::max) else {
        return RootStability::Unsupported {
            reason: "no phase is finite at root".into(),
        };
    };
    let stable = values
        .iter()
        .filter_map(|(id, value)| ((value - maximum).abs() <= 1.0e-7).then_some(*id))
        .collect::<Vec<_>>();
    if stable.contains(&first) && stable.contains(&second) {
        RootStability::StableCandidate { phase_ids: stable }
    } else {
        RootStability::Metastable {
            stable_phase_ids: stable,
        }
    }
}

fn field(grid: &RegularTabulatedGrid, phase: StablePhaseId) -> Result<&TabulatedField, String> {
    grid.fields
        .iter()
        .find(|field| field.phase_id == phase && field.property == "T")
        .ok_or_else(|| format!("grid `{}` has no T field for phase {}", grid.name, phase.0))
}

fn evaluate_raw_difference(
    grid: &RegularTabulatedGrid,
    first: StablePhaseId,
    second: StablePhaseId,
    composition: [f64; 3],
) -> Option<f64> {
    let left = defined_value(evaluate_regular_linear_field(
        grid,
        field(grid, first).ok()?,
        composition,
    ))?;
    let right = defined_value(evaluate_regular_linear_field(
        grid,
        field(grid, second).ok()?,
        composition,
    ))?;
    Some(left - right)
}

fn defined_value(value: StablePhaseEvaluation) -> Option<f64> {
    match value {
        StablePhaseEvaluation::Defined { value } => Some(value),
        StablePhaseEvaluation::Undefined { .. } | _ => None,
    }
}

fn evaluated(value: StablePhaseEvaluation) -> EvaluatedCell {
    match value {
        StablePhaseEvaluation::Defined { value } => EvaluatedCell {
            value: Some(value),
            state: "calculated".into(),
        },
        StablePhaseEvaluation::Undefined { reason } => EvaluatedCell {
            value: None,
            state: format!("undefined: {reason:?}"),
        },
        _ => EvaluatedCell {
            value: None,
            state: "undefined: unsupported evaluation state".into(),
        },
    }
}

fn finite_difference(first: &TabulatedValue, second: &TabulatedValue) -> Option<f64> {
    first
        .defined_value()
        .zip(second.defined_value())
        .map(|(left, right)| left - right)
}

fn linear_root(x0: f64, y0: f64, x1: f64, y1: f64) -> Option<f64> {
    if y0.abs() <= ZERO_TOLERANCE {
        return Some(x0);
    }
    if y1.abs() <= ZERO_TOLERANCE {
        return Some(x1);
    }
    (y0 * y1 < 0.0).then_some(x0 - y0 * (x1 - x0) / (y1 - y0))
}

fn edge_components(dataset: &TabulatedTernaryDataset, edge: BinaryBoundary) -> (String, String) {
    match edge {
        BinaryBoundary::Ab => (
            dataset.components[0].name.clone(),
            dataset.components[1].name.clone(),
        ),
        BinaryBoundary::Bc => (
            dataset.components[1].name.clone(),
            dataset.components[2].name.clone(),
        ),
        BinaryBoundary::Ca => (
            dataset.components[2].name.clone(),
            dataset.components[0].name.clone(),
        ),
    }
}

fn close_composition(left: [f64; 3], right: [f64; 3]) -> bool {
    left.into_iter()
        .zip(right)
        .all(|(a, b)| (a - b).abs() <= 1.0e-12)
}

fn format_value(value: Option<f64>) -> String {
    value.map_or_else(|| "".into(), |value| format!("{value:.12}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InterpolationOptions, parse_str};

    #[test]
    fn cao_pbo_zno_raw_edge_intervals_and_linear_roots_are_exact() {
        let dataset = parse_str(include_str!("../../../calculations/CaO-PbO-ZnO.tct")).unwrap();
        let options = ProjectionOptions {
            interpolation: InterpolationOptions {
                source: SourceInterpolation::Linear,
                ..InterpolationOptions::default()
            },
            ..ProjectionOptions::default()
        };
        let report = audit_cao_pbo_zno_binary_edges(&dataset, &options).unwrap();
        assert_eq!(report.edges.len(), 3);
        let sign_changes = report
            .edges
            .iter()
            .map(|edge| {
                edge.intervals
                    .iter()
                    .filter(|interval| interval.sign_change)
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(sign_changes, vec![0, 0, 1]);
        let ab = &report.edges[0].intervals[0];
        assert_eq!((ab.start_parameter, ab.end_parameter), (0.6, 0.9));
        assert!((ab.start_difference - 1596.45).abs() < 1.0e-9);
        assert!((ab.end_difference - 13.22).abs() < 1.0e-9);
        let bc = &report.edges[1].intervals[0];
        assert_eq!((bc.start_parameter, bc.end_parameter), (0.1, 0.9));
        assert!((bc.start_difference + 183.79).abs() < 1.0e-9);
        assert!((bc.end_difference + 1566.12).abs() < 1.0e-9);
        let ca = &report.edges[2].intervals[0];
        assert_eq!((ca.start_parameter, ca.end_parameter), (0.0, 0.5));
        let ca_root = ca.root.as_ref().unwrap();
        assert!((ca_root.parameter - 0.347_741_780_302_329_44).abs() < 1.0e-12);
        assert_eq!(ca_root.residual, 0.0);
        assert!(matches!(
            ca_root.stable,
            RootStability::StableCandidate { .. }
        ));
    }
}
