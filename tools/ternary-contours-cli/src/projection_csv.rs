//! Deterministic projection geometry records for native file and clipboard CSV export.

use ternary_contours::{StableInvariantNode, StablePhaseId};

use crate::{LiquidusProjection, RenderOptions, RenderPathMode, TabulatedTernaryDataset};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionLineType {
    StableIsotherm,
    StableUnivariant,
    BinaryInvariant,
    InteriorInvariant,
    StableBoundaryContact,
}

impl ProjectionLineType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StableIsotherm => "stable_isotherm",
            Self::StableUnivariant => "stable_univariant",
            Self::BinaryInvariant => "binary_invariant",
            Self::InteriorInvariant => "interior_invariant",
            Self::StableBoundaryContact => "stable_boundary_contact",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionPathSource {
    Raw,
    Regularized,
}

impl ProjectionPathSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Regularized => "regularized",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectionCsvLayerFilter {
    #[default]
    VisibleCalculatedLayers,
    AllCalculatedLayers,
    StableIsothermsOnly,
    StableUnivariantsOnly,
    InvariantsOnly,
}

impl ProjectionCsvLayerFilter {
    pub const fn label(self) -> &'static str {
        match self {
            Self::VisibleCalculatedLayers => "Visible calculated layers",
            Self::AllCalculatedLayers => "All calculated layers",
            Self::StableIsothermsOnly => "Stable isotherms only",
            Self::StableUnivariantsOnly => "Stable univariants only",
            Self::InvariantsOnly => "Invariants only",
        }
    }

    const fn include_isotherms(self, render: &RenderOptions) -> bool {
        match self {
            Self::VisibleCalculatedLayers => render.show_isotherms,
            Self::AllCalculatedLayers | Self::StableIsothermsOnly => true,
            Self::StableUnivariantsOnly | Self::InvariantsOnly => false,
        }
    }

    const fn include_univariants(self, render: &RenderOptions) -> bool {
        match self {
            Self::VisibleCalculatedLayers => render.show_univariants,
            Self::AllCalculatedLayers | Self::StableUnivariantsOnly => true,
            Self::StableIsothermsOnly | Self::InvariantsOnly => false,
        }
    }

    const fn include_binary_invariants(self, render: &RenderOptions) -> bool {
        match self {
            Self::VisibleCalculatedLayers => render.show_binary_invariants,
            Self::AllCalculatedLayers | Self::InvariantsOnly => true,
            Self::StableIsothermsOnly | Self::StableUnivariantsOnly => false,
        }
    }

    const fn include_interior_invariants(self, render: &RenderOptions) -> bool {
        match self {
            Self::VisibleCalculatedLayers => render.show_invariants,
            Self::AllCalculatedLayers | Self::InvariantsOnly => true,
            Self::StableIsothermsOnly | Self::StableUnivariantsOnly => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionCsvOptions {
    pub layers: ProjectionCsvLayerFilter,
    pub path_mode: RenderPathMode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectionCsvRecord {
    pub line_id: String,
    /// Zero-based in memory; written as a one-based point_number column.
    pub point_index: usize,
    /// Semantic A/B/C composition in dataset component order.
    pub composition: [f64; 3],
    pub temperature: Option<f64>,
    pub line_type: ProjectionLineType,
    pub phase: Option<String>,
    pub phase_1: Option<String>,
    pub phase_2: Option<String>,
    pub level: Option<f64>,
    pub path_source: ProjectionPathSource,
    pub closed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectionCsvError {
    #[error("there are no calculated lines to export")]
    NoProjection,
    #[error("the selected CSV export filter produced no calculated rows")]
    NoRows,
    #[error("{line_id} point {point_index} has no finite {what}")]
    NonFinite {
        line_id: String,
        point_index: usize,
        what: &'static str,
    },
    #[error(
        "stable univariant {line_id} has {point_count} points but {temperature_count} temperatures"
    )]
    UnivariantTemperatureCount {
        line_id: String,
        point_count: usize,
        temperature_count: usize,
    },
}

pub fn projection_csv_records(
    dataset: &TabulatedTernaryDataset,
    projection: Option<&LiquidusProjection>,
    raw_projection: Option<&LiquidusProjection>,
    render: &RenderOptions,
    options: ProjectionCsvOptions,
) -> Result<Vec<ProjectionCsvRecord>, ProjectionCsvError> {
    let projection = projection.ok_or(ProjectionCsvError::NoProjection)?;
    let mut records = Vec::new();
    for (source_projection, source) in
        selected_sources(projection, raw_projection, options.path_mode)
    {
        append_projection_records(
            &mut records,
            dataset,
            source_projection,
            source,
            render,
            options.layers,
        )?;
    }
    (!records.is_empty())
        .then_some(records)
        .ok_or(ProjectionCsvError::NoRows)
}

fn selected_sources<'a>(
    projection: &'a LiquidusProjection,
    raw_projection: Option<&'a LiquidusProjection>,
    path_mode: RenderPathMode,
) -> Vec<(&'a LiquidusProjection, ProjectionPathSource)> {
    let raw = raw_projection.unwrap_or(projection);
    match path_mode {
        RenderPathMode::Raw => vec![(raw, ProjectionPathSource::Raw)],
        RenderPathMode::Regularized => vec![(projection, ProjectionPathSource::Regularized)],
        RenderPathMode::Overlay => vec![
            (raw, ProjectionPathSource::Raw),
            (projection, ProjectionPathSource::Regularized),
        ],
    }
}

fn append_projection_records(
    records: &mut Vec<ProjectionCsvRecord>,
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    source: ProjectionPathSource,
    render: &RenderOptions,
    filter: ProjectionCsvLayerFilter,
) -> Result<(), ProjectionCsvError> {
    let source_name = source.as_str();
    if filter.include_isotherms(render) {
        for (level_index, level) in projection.stable_contours.levels.iter().enumerate() {
            for (path_index, path) in level.paths.iter().enumerate() {
                let line_id = format!(
                    "{source_name}-isotherm-{level_index}-phase-{}-path-{path_index}",
                    path.phase.0
                );
                let phase = phase_name(dataset, path.phase);
                for (point_index, point) in path.points.iter().enumerate() {
                    push_record(
                        records,
                        ProjectionCsvRecord {
                            line_id: line_id.clone(),
                            point_index,
                            composition: point.as_array(),
                            temperature: Some(level.value),
                            line_type: ProjectionLineType::StableIsotherm,
                            phase: Some(phase.clone()),
                            phase_1: None,
                            phase_2: None,
                            level: Some(level.value),
                            path_source: source,
                            closed: path.closed,
                        },
                    )?;
                }
            }
        }
    }

    if filter.include_univariants(render) {
        for path in &projection.stable_boundaries.univariants {
            let line_id = format!("{source_name}-univariant-{}", path.id.0);
            if path.points.len() != path.temperatures.len() {
                return Err(ProjectionCsvError::UnivariantTemperatureCount {
                    line_id,
                    point_count: path.points.len(),
                    temperature_count: path.temperatures.len(),
                });
            }
            let phase_1 = phase_name(dataset, path.phases.first);
            let phase_2 = phase_name(dataset, path.phases.second);
            for (point_index, (point, temperature)) in
                path.points.iter().zip(&path.temperatures).enumerate()
            {
                push_record(
                    records,
                    ProjectionCsvRecord {
                        line_id: line_id.clone(),
                        point_index,
                        composition: point.as_array(),
                        temperature: Some(*temperature),
                        line_type: ProjectionLineType::StableUnivariant,
                        phase: None,
                        phase_1: Some(phase_1.clone()),
                        phase_2: Some(phase_2.clone()),
                        level: None,
                        path_source: source,
                        closed: false,
                    },
                )?;
            }
        }
    }

    for node in &projection.stable_boundaries.nodes {
        let (line_type, include, phase_1, phase_2) = match node {
            StableInvariantNode::Binary(node) => (
                ProjectionLineType::BinaryInvariant,
                filter.include_binary_invariants(render),
                node.phases.first().copied(),
                node.phases.get(1).copied(),
            ),
            StableInvariantNode::Interior(node) => (
                ProjectionLineType::InteriorInvariant,
                filter.include_interior_invariants(render),
                node.phases.first().copied(),
                node.phases.get(1).copied(),
            ),
        };
        if !include {
            continue;
        }
        let line_id = format!("{source_name}-{}-{}", line_type.as_str(), node.id().0);
        push_record(
            records,
            ProjectionCsvRecord {
                line_id,
                point_index: 0,
                composition: node.point().as_array(),
                temperature: Some(node.temperature()),
                line_type,
                phase: None,
                phase_1: phase_1.map(|id| phase_name(dataset, id)),
                phase_2: phase_2.map(|id| phase_name(dataset, id)),
                level: None,
                path_source: source,
                closed: false,
            },
        )?;
    }
    Ok(())
}

fn phase_name(dataset: &TabulatedTernaryDataset, phase_id: StablePhaseId) -> String {
    dataset
        .phases
        .iter()
        .find(|phase| phase.id == phase_id)
        .map(|phase| phase.name.clone())
        .unwrap_or_else(|| format!("Phase {}", phase_id.0))
}

fn push_record(
    records: &mut Vec<ProjectionCsvRecord>,
    record: ProjectionCsvRecord,
) -> Result<(), ProjectionCsvError> {
    for coordinate in record.composition {
        if !coordinate.is_finite() {
            return Err(ProjectionCsvError::NonFinite {
                line_id: record.line_id,
                point_index: record.point_index,
                what: "composition coordinate",
            });
        }
    }
    if !record.temperature.is_some_and(f64::is_finite) {
        return Err(ProjectionCsvError::NonFinite {
            line_id: record.line_id,
            point_index: record.point_index,
            what: "temperature",
        });
    }
    records.push(record);
    Ok(())
}

/// Semantic section selection for the compact thermodynamic geometry CSV.
///
/// Unlike [`ProjectionCsvRecord`], this format deliberately contains no render
/// IDs, point indices, or styling data.  It is the export used by the Qt
/// Projection CSV dialog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionGeometryCsvSelection {
    pub invariants: bool,
    pub univariants: bool,
    pub isotherms: bool,
}

impl Default for ProjectionGeometryCsvSelection {
    fn default() -> Self {
        Self {
            invariants: true,
            univariants: true,
            isotherms: true,
        }
    }
}

impl ProjectionGeometryCsvSelection {
    pub const fn any(self) -> bool {
        self.invariants || self.univariants || self.isotherms
    }
}

/// Serialize an accepted projection as simple thermodynamic geometry.
///
/// Numeric cells use `f64::to_string`, Rust's shortest round-trip-safe decimal
/// representation.  This is a data serialization boundary, intentionally
/// distinct from the fixed-decimal GUI presentation contract.
pub fn serialize_projection_geometry_csv(
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    selection: ProjectionGeometryCsvSelection,
) -> Result<String, ProjectionCsvError> {
    if !selection.any() {
        return Err(ProjectionCsvError::NoRows);
    }

    let temperature = dataset
        .properties
        .iter()
        .find(|property| property.name.eq_ignore_ascii_case("T"))
        .or_else(|| dataset.properties.iter().find(|property| property.required));
    let temperature_name = temperature
        .map(|property| property.name.as_str())
        .unwrap_or("T");
    let temperature_unit = temperature
        .map(|property| property.unit.as_str())
        .unwrap_or("");
    let temperature_heading = if temperature_unit.is_empty() {
        temperature_name.to_owned()
    } else {
        format!("{temperature_name}, {temperature_unit}")
    };
    let mut output = [
        csv_text(&dataset.components[0].name),
        csv_text(&dataset.components[1].name),
        csv_text(&dataset.components[2].name),
        csv_text(&temperature_heading),
        "phase1".to_owned(),
        "phase2".to_owned(),
        "phase3".to_owned(),
    ]
    .join(",");
    output.push_str("\r\n");

    // The flag retains the required physical blank line after every complete
    // univariant, including the final selected path.
    let mut sections = Vec::<(String, bool)>::new();
    if selection.invariants {
        let mut nodes = projection
            .stable_boundaries
            .nodes
            .iter()
            .collect::<Vec<_>>();
        nodes.sort_by(|left, right| {
            compare_invariant_sort_keys(
                invariant_sort_key(dataset, left),
                invariant_sort_key(dataset, right),
            )
        });
        let mut section = String::new();
        for node in nodes {
            let mut phases = node
                .phases()
                .iter()
                .copied()
                .map(|phase| phase_name(dataset, phase))
                .collect::<Vec<_>>();
            phases.sort();
            phases.truncate(3);
            write_geometry_row(
                &mut section,
                node.point().as_array(),
                node.temperature(),
                [
                    phases.first().map(String::as_str),
                    phases.get(1).map(String::as_str),
                    phases.get(2).map(String::as_str),
                ],
            )?;
        }
        if !section.is_empty() {
            sections.push((section.trim_end_matches("\r\n").to_owned(), false));
        }
    }

    if selection.univariants {
        let mut paths = projection
            .stable_boundaries
            .univariants
            .iter()
            .collect::<Vec<_>>();
        paths.sort_by(|left, right| {
            univariant_sort_key(dataset, left).cmp(&univariant_sort_key(dataset, right))
        });
        let mut section = String::new();
        for path in paths {
            if path.points.len() != path.temperatures.len() {
                return Err(ProjectionCsvError::UnivariantTemperatureCount {
                    line_id: format!("univariant-{}", path.id.0),
                    point_count: path.points.len(),
                    temperature_count: path.temperatures.len(),
                });
            }
            let first = phase_name(dataset, path.phases.first);
            let second = phase_name(dataset, path.phases.second);
            for (point, temperature) in path.points.iter().zip(&path.temperatures) {
                write_geometry_row(
                    &mut section,
                    point.as_array(),
                    *temperature,
                    [Some(first.as_str()), Some(second.as_str()), None],
                )?;
            }
            // A physically empty line, not seven empty CSV fields, separates
            // every complete phase-pair path.
            section.push_str("\r\n");
        }
        if !section.is_empty() {
            sections.push((section.trim_end_matches("\r\n").to_owned(), true));
        }
    }

    if selection.isotherms {
        let mut paths = projection
            .stable_contours
            .levels
            .iter()
            .flat_map(|level| level.paths.iter().map(move |path| (level.value, path)))
            .collect::<Vec<_>>();
        paths.sort_by(|(left_level, left_path), (right_level, right_path)| {
            left_level
                .total_cmp(right_level)
                .then_with(|| {
                    phase_name(dataset, left_path.phase).cmp(&phase_name(dataset, right_path.phase))
                })
                .then_with(|| {
                    first_point_key(left_path.points.first())
                        .cmp(&first_point_key(right_path.points.first()))
                })
        });
        let mut section = String::new();
        for (level, path) in paths {
            let phase = phase_name(dataset, path.phase);
            for point in &path.points {
                write_geometry_row(
                    &mut section,
                    point.as_array(),
                    level,
                    [Some(phase.as_str()), None, None],
                )?;
            }
            // A path can be empty only for an invalid calculation, but keeping
            // separators path-based preserves component boundaries faithfully.
            section.push_str("\r\n");
        }
        if !section.is_empty() {
            sections.push((section.trim_end_matches("\r\n").to_owned(), false));
        }
    }

    if sections.is_empty() {
        return Err(ProjectionCsvError::NoRows);
    }
    for (index, (section, ends_with_blank_line)) in sections.iter().enumerate() {
        output.push_str(section);
        if *ends_with_blank_line || index + 1 < sections.len() {
            // A physical blank line is two record terminators, not a row of
            // seven empty CSV cells.  A univariant's own terminal separator
            // also separates it from a following isotherm section.
            output.push_str("\r\n\r\n");
        } else {
            output.push_str("\r\n");
        }
    }
    Ok(output)
}

fn invariant_sort_key(
    dataset: &TabulatedTernaryDataset,
    node: &StableInvariantNode,
) -> (f64, f64, f64, String) {
    let point = node.point().as_array();
    let mut phases = node
        .phases()
        .iter()
        .copied()
        .map(|phase| phase_name(dataset, phase))
        .collect::<Vec<_>>();
    phases.sort();
    (point[0], point[1], point[2], phases.join("\u{1f}"))
}

fn compare_invariant_sort_keys(
    left: (f64, f64, f64, String),
    right: (f64, f64, f64, String),
) -> std::cmp::Ordering {
    left.0
        .total_cmp(&right.0)
        .then_with(|| left.1.total_cmp(&right.1))
        .then_with(|| left.2.total_cmp(&right.2))
        .then_with(|| left.3.cmp(&right.3))
}

fn univariant_sort_key(
    dataset: &TabulatedTernaryDataset,
    path: &ternary_contours::StableUnivariantPath,
) -> (String, String, [u64; 3], [u64; 3]) {
    let first = phase_name(dataset, path.phases.first);
    let second = phase_name(dataset, path.phases.second);
    let (first, second) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    let start = first_point_key(path.points.first());
    let end = first_point_key(path.points.last());
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    (first, second, start, end)
}

fn first_point_key(point: Option<&ternary_contours::TernaryCoordinate>) -> [u64; 3] {
    point.map_or([0; 3], |point| point.as_array().map(f64::to_bits))
}

fn write_geometry_row(
    output: &mut String,
    composition: [f64; 3],
    temperature: f64,
    phases: [Option<&str>; 3],
) -> Result<(), ProjectionCsvError> {
    if composition.iter().any(|value| !value.is_finite()) || !temperature.is_finite() {
        return Err(ProjectionCsvError::NonFinite {
            line_id: "projection geometry".into(),
            point_index: 0,
            what: "geometry CSV value",
        });
    }
    let row = [
        number(composition[0]),
        number(composition[1]),
        number(composition[2]),
        number(temperature),
        optional_text(phases[0]),
        optional_text(phases[1]),
        optional_text(phases[2]),
    ];
    output.push_str(&row.join(","));
    output.push_str("\r\n");
    Ok(())
}

pub fn serialize_projection_csv(
    records: &[ProjectionCsvRecord],
) -> Result<String, ProjectionCsvError> {
    if records.is_empty() {
        return Err(ProjectionCsvError::NoRows);
    }
    let mut output = String::from(
        "line_id,point_number,A,B,C,T,line_type,phase,phase_1,phase_2,level,path_source,closed\r\n",
    );
    for record in records {
        validate_record(record)?;
        let cells = [
            csv_text(&record.line_id),
            (record.point_index + 1).to_string(),
            number(record.composition[0]),
            number(record.composition[1]),
            number(record.composition[2]),
            number(record.temperature.expect("validated temperature")),
            record.line_type.as_str().to_owned(),
            optional_text(record.phase.as_deref()),
            optional_text(record.phase_1.as_deref()),
            optional_text(record.phase_2.as_deref()),
            record.level.map(number).unwrap_or_default(),
            record.path_source.as_str().to_owned(),
            record.closed.to_string(),
        ];
        output.push_str(&cells.join(","));
        output.push_str("\r\n");
    }
    Ok(output)
}

fn validate_record(record: &ProjectionCsvRecord) -> Result<(), ProjectionCsvError> {
    let mut ignored = Vec::new();
    push_record(&mut ignored, record.clone())
}

fn number(value: f64) -> String {
    value.to_string()
}

fn optional_text(value: Option<&str>) -> String {
    value.map(csv_text).unwrap_or_default()
}

fn csv_text(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectionOptions, calculate_projection, parse_str};
    fn fixture() -> (TabulatedTernaryDataset, LiquidusProjection) {
        let dataset = parse_str(include_str!("../fixtures/interior-invariant.tct")).unwrap();
        let projection = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
        (dataset, projection)
    }
    #[test]
    fn csv_has_required_columns_finite_rows_and_one_record_per_visible_point() {
        let (dataset, projection) = fixture();
        let records = projection_csv_records(
            &dataset,
            Some(&projection),
            None,
            &RenderOptions::default(),
            ProjectionCsvOptions::default(),
        )
        .unwrap();
        let expected = projection
            .stable_contours
            .levels
            .iter()
            .flat_map(|level| &level.paths)
            .map(|path| path.points.len())
            .sum::<usize>()
            + projection
                .stable_boundaries
                .univariants
                .iter()
                .map(|path| path.points.len())
                .sum::<usize>()
            + projection.stable_boundaries.nodes.len();
        assert_eq!(records.len(), expected);
        assert!(records.iter().all(|record| {
            record.composition.into_iter().all(f64::is_finite)
                && record.temperature.is_some_and(f64::is_finite)
        }));
        let csv = serialize_projection_csv(&records).unwrap();
        assert!(csv.starts_with(
            "line_id,point_number,A,B,C,T,line_type,phase,phase_1,phase_2,level,path_source,closed\r\n"
        ));
        assert!(csv.ends_with("\r\n"));
        assert!(!csv.contains("NaN"));
        assert!(!csv.contains("inf"));
    }
    #[test]
    fn isotherms_use_requested_levels_and_overlay_retains_path_provenance() {
        let (dataset, raw) = fixture();
        let raw_records = projection_csv_records(
            &dataset,
            Some(&raw),
            None,
            &RenderOptions::default(),
            ProjectionCsvOptions::default(),
        )
        .unwrap();
        assert!(
            raw_records
                .iter()
                .filter(|record| record.line_type == ProjectionLineType::StableIsotherm)
                .all(|record| record.temperature == record.level)
        );
        let options = ProjectionOptions {
            regularize: true,
            ..ProjectionOptions::default()
        };
        let regularized = calculate_projection(&dataset, &options).unwrap();
        let render = RenderOptions {
            show_isotherms: false,
            show_binary_invariants: false,
            show_invariants: false,
            ..RenderOptions::default()
        };
        let overlay = projection_csv_records(
            &dataset,
            Some(&regularized),
            Some(&raw),
            &render,
            ProjectionCsvOptions {
                layers: ProjectionCsvLayerFilter::VisibleCalculatedLayers,
                path_mode: RenderPathMode::Overlay,
            },
        )
        .unwrap();
        assert!(
            overlay
                .iter()
                .any(|record| record.path_source == ProjectionPathSource::Raw)
        );
        assert!(
            overlay
                .iter()
                .any(|record| record.path_source == ProjectionPathSource::Regularized)
        );
        assert!(
            overlay
                .iter()
                .all(|record| record.line_type == ProjectionLineType::StableUnivariant)
        );
    }
    #[test]
    fn geometry_csv_has_the_compact_schema_sections_and_round_trip_numbers() {
        let (mut dataset, projection) = fixture();
        dataset.components[0].name = "CaO".into();
        dataset.components[1].name = "PbO".into();
        dataset.components[2].name = "ZnO".into();
        let temperature = dataset
            .properties
            .iter_mut()
            .find(|property| property.name == "T")
            .expect("temperature property");
        temperature.unit = "C".into();
        let csv = serialize_projection_geometry_csv(
            &dataset,
            &projection,
            ProjectionGeometryCsvSelection::default(),
        )
        .unwrap();
        assert!(csv.starts_with("CaO,PbO,ZnO,\"T, C\",phase1,phase2,phase3\r\n"));
        assert!(
            csv.contains("\r\n\r\n"),
            "sections and paths use physical blank lines"
        );
        for row in csv.lines().skip(1).filter(|row| !row.is_empty()) {
            assert_eq!(
                row.split(',').count(),
                7,
                "row must have exactly seven fields: {row}"
            );
        }
        let precise = 0.094_813_237_512_345_f64;
        let serialized = number(precise);
        assert_eq!(
            serialized.parse::<f64>().unwrap().to_bits(),
            precise.to_bits()
        );
        assert_ne!(serialized, "0.09481", "CSV is not GUI display precision");
    }

    #[test]
    fn geometry_csv_honours_section_selection_without_placeholder_rows() {
        let (dataset, projection) = fixture();
        let csv = serialize_projection_geometry_csv(
            &dataset,
            &projection,
            ProjectionGeometryCsvSelection {
                invariants: true,
                univariants: false,
                isotherms: false,
            },
        )
        .unwrap();
        assert!(!csv.contains(",,,,,,"));
        assert!(
            csv.lines()
                .skip(1)
                .all(|row| row.is_empty() || row.split(',').count() == 7)
        );
        assert!(matches!(
            serialize_projection_geometry_csv(
                &dataset,
                &projection,
                ProjectionGeometryCsvSelection {
                    invariants: false,
                    univariants: false,
                    isotherms: false
                },
            ),
            Err(ProjectionCsvError::NoRows)
        ));
    }

    #[test]
    fn univariant_only_geometry_csv_keeps_a_terminal_physical_blank_line() {
        let (dataset, projection) = fixture();
        let csv = serialize_projection_geometry_csv(
            &dataset,
            &projection,
            ProjectionGeometryCsvSelection {
                invariants: false,
                univariants: true,
                isotherms: false,
            },
        )
        .unwrap();
        assert!(csv.ends_with("\r\n\r\n"));
        assert!(!csv.contains(",,,,,,"));
    }

    #[test]
    fn csv_quotes_text_and_rejects_empty_records() {
        assert_eq!(csv_text("phase, \"quoted\""), "\"phase, \"\"quoted\"\"\"");
        assert!(matches!(
            serialize_projection_csv(&[]),
            Err(ProjectionCsvError::NoRows)
        ));
    }
}
