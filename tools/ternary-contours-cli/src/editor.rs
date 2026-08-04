//! Pure TSV preview and dataset-draft helpers for the data editor.

use std::fmt;

use ternary_contours::{RegularTernaryGrid, StablePhaseId};

use crate::{HeaderMode, NumericFormat, ParsedRow, ParsedTable};

const TOLERANCE: f64 = 1.0e-8;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FieldKey {
    pub phase_id: StablePhaseId,
    pub property: String,
}

impl fmt::Display for FieldKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.phase_id.0, self.property)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldColumnMapping {
    pub source_column: usize,
    pub destination: FieldKey,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RegularCompositionPasteMode {
    #[default]
    ValuesOnly,
    Guidance,
    Authoritative,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CompositionNormalization {
    RejectNonNormalized,
    #[default]
    NormalizeWithinTolerance,
    NormalizeAllPositiveRows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldReplacement {
    AddNewField,
    ReplaceExistingField,
    ReplaceEntireGrid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorIssue {
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub token: Option<String>,
    pub message: String,
}

impl fmt::Display for EditorIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.row, self.column) {
            (Some(row), Some(column)) => write!(f, "row {row}, column {column}: {}", self.message),
            (Some(row), None) => write!(f, "row {row}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorValidationReport {
    pub errors: Vec<EditorIssue>,
    pub warnings: Vec<EditorIssue>,
    pub normalized_rows: usize,
    pub canonical_reorder_count: usize,
    pub maximum_guidance_residual: f64,
    pub missing_values: usize,
}

impl EditorValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn as_text(&self) -> String {
        let mut text = format!(
            "errors: {}; warnings: {}; missing values: {}; normalized rows: {}; canonical reorders: {}; max guidance residual: {:.3e}",
            self.errors.len(),
            self.warnings.len(),
            self.missing_values,
            self.normalized_rows,
            self.canonical_reorder_count,
            self.maximum_guidance_residual,
        );
        for issue in self.errors.iter().chain(&self.warnings) {
            text.push('\n');
            text.push_str(&issue.to_string());
        }
        text
    }

    fn error(
        &mut self,
        row: Option<usize>,
        column: Option<usize>,
        token: Option<String>,
        message: impl Into<String>,
    ) {
        self.errors.push(EditorIssue {
            row,
            column,
            token,
            message: message.into(),
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingField {
    pub key: FieldKey,
    pub column_name: String,
    pub values: Vec<TabulatedValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegularPasteMapping {
    pub header_mode: HeaderMode,
    pub composition_mode: RegularCompositionPasteMode,
    pub composition_columns: Option<[usize; 3]>,
    pub fields: Vec<FieldColumnMapping>,
    pub missing_tokens: Vec<String>,
    pub blank_cells_are_missing: bool,
    pub coordinate_tolerance: f64,
    pub allow_guidance_warnings: bool,
}

impl Default for RegularPasteMapping {
    fn default() -> Self {
        Self {
            header_mode: HeaderMode::Detect,
            composition_mode: RegularCompositionPasteMode::ValuesOnly,
            composition_columns: None,
            fields: Vec::new(),
            missing_tokens: vec!["NA".into()],
            blank_cells_are_missing: false,
            coordinate_tolerance: TOLERANCE,
            allow_guidance_warnings: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IrregularPasteMapping {
    pub header_mode: HeaderMode,
    pub composition_columns: [usize; 3],
    pub fields: Vec<FieldColumnMapping>,
    pub missing_tokens: Vec<String>,
    pub blank_cells_are_missing: bool,
    pub coordinate_tolerance: f64,
    pub normalization: CompositionNormalization,
}

impl Default for IrregularPasteMapping {
    fn default() -> Self {
        Self {
            header_mode: HeaderMode::Detect,
            composition_columns: [0, 1, 2],
            fields: Vec::new(),
            missing_tokens: vec!["NA".into()],
            blank_cells_are_missing: false,
            coordinate_tolerance: TOLERANCE,
            normalization: CompositionNormalization::NormalizeWithinTolerance,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegularPastePreview {
    pub subdivisions: usize,
    pub canonical_compositions: Vec<[f64; 3]>,
    pub fields: Vec<PendingField>,
    pub pasted_rows: usize,
    pub report: EditorValidationReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IrregularPastePreview {
    pub compositions: Vec<[f64; 3]>,
    pub fields: Vec<PendingField>,
    pub pasted_rows: usize,
    pub report: EditorValidationReport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PastePreview {
    Regular(RegularPastePreview),
    Irregular(IrregularPastePreview),
}

impl PastePreview {
    pub fn report(&self) -> &EditorValidationReport {
        match self {
            Self::Regular(value) => &value.report,
            Self::Irregular(value) => &value.report,
        }
    }
}

pub fn regular_compositions(subdivisions: usize) -> Result<Vec<[f64; 3]>, String> {
    RegularTernaryGrid::new(subdivisions)
        .map(|grid| grid.compositions().collect())
        .map_err(|error| error.to_string())
}

pub fn regular_row_count(subdivisions: usize) -> Result<usize, String> {
    RegularTernaryGrid::new(subdivisions)
        .map(|grid| grid.vertex_count())
        .map_err(|error| error.to_string())
}

pub fn compositions_tsv(
    subdivisions: usize,
    components: [&str; 3],
    header: bool,
    numeric: NumericFormat,
) -> Result<String, String> {
    let mut rows = Vec::new();
    if header {
        rows.push(components.join("\t"));
    }
    rows.extend(
        regular_compositions(subdivisions)?
            .into_iter()
            .map(|point| point.map(|value| numeric.format(value)).join("\t")),
    );
    Ok(rows.join("\n") + "\n")
}

pub fn preview_regular_paste(
    text: &str,
    subdivisions: usize,
    mapping: &RegularPasteMapping,
) -> RegularPastePreview {
    let canonical = match regular_compositions(subdivisions) {
        Ok(value) => value,
        Err(message) => return bad_regular(subdivisions, Vec::new(), message),
    };
    let table = match ParsedTable::parse_tsv(text, mapping.header_mode) {
        Ok(value) => value,
        Err(error) => return bad_regular(subdivisions, canonical, error.to_string()),
    };
    let mut report = EditorValidationReport::default();
    validate_mapping(&mapping.fields, table.width(), &mut report);
    if table.height() != canonical.len() {
        report.error(
            None,
            None,
            None,
            format!(
                "regular paste has {} rows; expected {}",
                table.height(),
                canonical.len()
            ),
        );
    }
    if !matches!(
        mapping.composition_mode,
        RegularCompositionPasteMode::ValuesOnly
    ) {
        validate_composition_columns(mapping.composition_columns, table.width(), &mut report);
    }
    let mut fields = empty_fields(&mapping.fields, canonical.len());
    let mut seen = vec![false; canonical.len()];
    for (source_index, row) in table.rows.iter().enumerate() {
        let index = match regular_row_index(
            row,
            source_index,
            &canonical,
            mapping,
            &mut seen,
            &mut report,
        ) {
            Some(value) => value,
            None => continue,
        };
        read_fields(
            row,
            index,
            &mapping.fields,
            &mapping.missing_tokens,
            mapping.blank_cells_are_missing,
            &mut fields,
            &mut report,
        );
    }
    if matches!(
        mapping.composition_mode,
        RegularCompositionPasteMode::Authoritative
    ) {
        for (index, found) in seen.iter().enumerate() {
            if !found {
                report.error(
                    None,
                    None,
                    None,
                    format!("missing canonical regular point at row {}", index + 1),
                );
            }
        }
    }
    RegularPastePreview {
        subdivisions,
        canonical_compositions: canonical,
        fields,
        pasted_rows: table.height(),
        report,
    }
}

pub fn preview_irregular_paste(
    text: &str,
    mapping: &IrregularPasteMapping,
) -> IrregularPastePreview {
    let table = match ParsedTable::parse_tsv(text, mapping.header_mode) {
        Ok(value) => value,
        Err(error) => {
            return IrregularPastePreview {
                compositions: Vec::new(),
                fields: Vec::new(),
                pasted_rows: 0,
                report: report_with_error(error.to_string()),
            };
        }
    };
    let mut report = EditorValidationReport::default();
    validate_mapping(&mapping.fields, table.width(), &mut report);
    validate_composition_columns(
        Some(mapping.composition_columns),
        table.width(),
        &mut report,
    );
    let mut compositions = Vec::new();
    let mut fields = empty_fields(&mapping.fields, 0);
    for row in &table.rows {
        let Some(point) = parse_composition(
            row,
            mapping.composition_columns,
            mapping.coordinate_tolerance,
            mapping.normalization,
            &mut report,
        ) else {
            continue;
        };
        let index = compositions.len();
        compositions.push(point);
        for field in &mut fields {
            field.values.push(TabulatedValue::missing());
        }
        read_fields(
            row,
            index,
            &mapping.fields,
            &mapping.missing_tokens,
            mapping.blank_cells_are_missing,
            &mut fields,
            &mut report,
        );
    }
    validate_irregular_points(&compositions, mapping.coordinate_tolerance, &mut report);
    IrregularPastePreview {
        compositions,
        fields,
        pasted_rows: table.height(),
        report,
    }
}

fn bad_regular(
    subdivisions: usize,
    canonical: Vec<[f64; 3]>,
    message: String,
) -> RegularPastePreview {
    RegularPastePreview {
        subdivisions,
        canonical_compositions: canonical,
        fields: Vec::new(),
        pasted_rows: 0,
        report: report_with_error(message),
    }
}

fn report_with_error(message: String) -> EditorValidationReport {
    let mut report = EditorValidationReport::default();
    report.error(None, None, None, message);
    report
}

fn empty_fields(mappings: &[FieldColumnMapping], rows: usize) -> Vec<PendingField> {
    mappings
        .iter()
        .map(|mapping| PendingField {
            key: mapping.destination.clone(),
            column_name: mapping.label.clone(),
            values: vec![TabulatedValue::missing(); rows],
        })
        .collect()
}

fn validate_mapping(
    fields: &[FieldColumnMapping],
    width: usize,
    report: &mut EditorValidationReport,
) {
    if fields.is_empty() {
        report.error(None, None, None, "select at least one scalar field");
    }
    for (index, field) in fields.iter().enumerate() {
        if field.source_column >= width {
            report.error(
                None,
                Some(field.source_column + 1),
                None,
                "mapped scalar column is outside the table",
            );
        }
        if fields[..index]
            .iter()
            .any(|prior| prior.destination == field.destination)
        {
            report.error(
                None,
                Some(field.source_column + 1),
                None,
                format!("duplicate destination field {}", field.destination),
            );
        }
    }
}

fn validate_composition_columns(
    columns: Option<[usize; 3]>,
    width: usize,
    report: &mut EditorValidationReport,
) {
    let Some(columns) = columns else {
        report.error(None, None, None, "map all three composition columns");
        return;
    };
    if columns.into_iter().any(|column| column >= width)
        || columns[0] == columns[1]
        || columns[0] == columns[2]
        || columns[1] == columns[2]
    {
        report.error(
            None,
            None,
            None,
            "composition columns must be three distinct in-range columns",
        );
    }
}

fn regular_row_index(
    row: &ParsedRow,
    source_index: usize,
    canonical: &[[f64; 3]],
    mapping: &RegularPasteMapping,
    seen: &mut [bool],
    report: &mut EditorValidationReport,
) -> Option<usize> {
    match mapping.composition_mode {
        RegularCompositionPasteMode::ValuesOnly => Some(source_index),
        RegularCompositionPasteMode::Guidance | RegularCompositionPasteMode::Authoritative => {
            let point = parse_composition(
                row,
                mapping.composition_columns?,
                mapping.coordinate_tolerance,
                CompositionNormalization::RejectNonNormalized,
                report,
            )?;
            let target = canonical
                .iter()
                .position(|candidate| residual(point, *candidate) <= mapping.coordinate_tolerance);
            if matches!(
                mapping.composition_mode,
                RegularCompositionPasteMode::Guidance
            ) {
                let expected = canonical.get(source_index)?;
                let delta = residual(point, *expected);
                report.maximum_guidance_residual = report.maximum_guidance_residual.max(delta);
                if delta > mapping.coordinate_tolerance {
                    let issue = EditorIssue {
                        row: Some(row.source_row),
                        column: None,
                        token: None,
                        message: format!("guidance coordinate residual {delta:e}"),
                    };
                    if mapping.allow_guidance_warnings {
                        report.warnings.push(issue);
                    } else {
                        report.errors.push(issue);
                    }
                }
                Some(source_index)
            } else {
                let Some(target) = target else {
                    report.error(
                        Some(row.source_row),
                        None,
                        None,
                        "composition is not on the declared regular lattice",
                    );
                    return None;
                };
                if seen[target] {
                    report.error(
                        Some(row.source_row),
                        None,
                        None,
                        "duplicate regular-grid composition",
                    );
                    return None;
                }
                seen[target] = true;
                if target != source_index {
                    report.canonical_reorder_count += 1;
                }
                Some(target)
            }
        }
    }
}

fn read_fields(
    row: &ParsedRow,
    target: usize,
    mappings: &[FieldColumnMapping],
    missing: &[String],
    blanks: bool,
    fields: &mut [PendingField],
    report: &mut EditorValidationReport,
) {
    for (index, mapping) in mappings.iter().enumerate() {
        let Some(cell) = row.cells.get(mapping.source_column) else {
            continue;
        };
        let value = parse_tabulated_value_token(&cell.text, missing, blanks);
        match value {
            Ok(value) => {
                report.missing_values +=
                    usize::from(matches!(value.state, crate::TabulatedValueState::Missing));
                if let Some(destination) = fields
                    .get_mut(index)
                    .and_then(|field| field.values.get_mut(target))
                {
                    *destination = value;
                }
            }
            Err(message) => report.error(
                Some(cell.location.row),
                Some(cell.location.column),
                Some(cell.text.clone()),
                message,
            ),
        }
    }
}

fn parse_composition(
    row: &ParsedRow,
    columns: [usize; 3],
    tolerance: f64,
    mode: CompositionNormalization,
    report: &mut EditorValidationReport,
) -> Option<[f64; 3]> {
    let mut point = [0.0; 3];
    for (component, column) in columns.into_iter().enumerate() {
        let cell = row.cells.get(column)?;
        match parse_finite(&cell.text) {
            Ok(value) => point[component] = value,
            Err(message) => {
                report.error(
                    Some(cell.location.row),
                    Some(cell.location.column),
                    Some(cell.text.clone()),
                    message,
                );
                return None;
            }
        }
    }
    if point.iter().any(|value| *value < -tolerance) {
        report.error(
            Some(row.source_row),
            None,
            None,
            "composition components must be non-negative",
        );
        return None;
    }
    let sum = point.into_iter().sum::<f64>();
    let close = (sum - 1.0).abs() <= tolerance;
    let normalize = match mode {
        CompositionNormalization::RejectNonNormalized => close,
        CompositionNormalization::NormalizeWithinTolerance => close && sum > tolerance,
        CompositionNormalization::NormalizeAllPositiveRows => sum > tolerance,
    };
    if !normalize {
        report.error(
            Some(row.source_row),
            None,
            None,
            format!("composition sum {sum:.12} is not accepted by the normalization policy"),
        );
        return None;
    }
    if (sum - 1.0).abs() > f64::EPSILON {
        report.normalized_rows += 1;
        point = point.map(|value| value / sum);
    }
    Some(point.map(|value| if value.abs() <= tolerance { 0.0 } else { value }))
}

fn parse_finite(text: &str) -> Result<f64, &'static str> {
    let value = text.parse::<f64>().map_err(|_| "invalid numeric token")?;
    value
        .is_finite()
        .then_some(value)
        .ok_or("numeric value must be finite")
}

fn validate_irregular_points(
    points: &[[f64; 3]],
    tolerance: f64,
    report: &mut EditorValidationReport,
) {
    if points.len() < 3 {
        report.error(
            None,
            None,
            None,
            "irregular grid requires at least three distinct points",
        );
        return;
    }
    for right in 0..points.len() {
        if let Some(left) = points[..right]
            .iter()
            .position(|point| residual(*point, points[right]) <= tolerance)
        {
            report.error(
                Some(right + 1),
                None,
                None,
                format!(
                    "duplicate or near-duplicate composition matches row {}",
                    left + 1
                ),
            );
        }
    }
    let origin = logical(points[0]);
    let non_collinear = (1..points.len()).any(|middle| {
        ((middle + 1)..points.len()).any(|end| {
            let p = logical(points[middle]);
            let q = logical(points[end]);
            ((p[0] - origin[0]) * (q[1] - origin[1]) - (p[1] - origin[1]) * (q[0] - origin[0]))
                .abs()
                > tolerance
        })
    });
    if !non_collinear {
        report.error(None, None, None, "irregular-grid points are collinear");
    }
}

fn logical([_a, b, c]: [f64; 3]) -> [f64; 2] {
    [b + 0.5 * c, 0.5 * 3.0_f64.sqrt() * c]
}

fn residual(left: [f64; 3], right: [f64; 3]) -> f64 {
    left.into_iter()
        .zip(right)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max)
}

use crate::{
    CompositionColumns, IrregularTabulatedGrid, RegularTabulatedGrid, RowOrder, SourceRange,
    TabulatedField, TabulatedGrid, TabulatedTernaryDataset, TabulatedValue, TctSerializeOptions,
    parse_str, parse_tabulated_value_token, serialize_tct,
};

/// Separate applied data from a mutable draft and un-applied paste preview.
#[derive(Clone, Debug)]
pub struct DatasetEditorState {
    pub active: TabulatedTernaryDataset,
    pub draft: TabulatedTernaryDataset,
    pub paste_preview: Option<PastePreview>,
    pub validation: EditorValidationReport,
    pub dirty: bool,
    undo: Vec<TabulatedTernaryDataset>,
    redo: Vec<TabulatedTernaryDataset>,
    draft_undo: Vec<TabulatedTernaryDataset>,
    draft_redo: Vec<TabulatedTernaryDataset>,
    history_limit: usize,
}

impl DatasetEditorState {
    pub fn new(active: TabulatedTernaryDataset) -> Self {
        Self {
            draft: active.clone(),
            active,
            paste_preview: None,
            validation: EditorValidationReport::default(),
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            draft_undo: Vec::new(),
            draft_redo: Vec::new(),
            history_limit: 32,
        }
    }

    pub fn set_preview(&mut self, preview: PastePreview) {
        self.validation = preview.report().clone();
        self.paste_preview = Some(preview);
    }

    pub fn revert(&mut self) {
        self.draft = self.active.clone();
        self.paste_preview = None;
        self.validation = EditorValidationReport::default();
        self.draft_undo.clear();
        self.draft_redo.clear();
        self.dirty = false;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.active.clone());
        self.active = previous.clone();
        self.draft = previous;
        self.dirty = false;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.active.clone());
        self.active = next.clone();
        self.draft = next;
        self.dirty = false;
        true
    }

    /// Validate the draft through the real TCT parser and make it the active
    /// editor dataset. The caller decides when to initiate numerical work.
    pub fn apply_draft(&mut self) -> Result<TabulatedTernaryDataset, String> {
        self.draft.validate_structure()?;
        let text = serialize_tct(&self.draft, &TctSerializeOptions::default())
            .map_err(|error| error.to_string())?;
        let validated = parse_str(&text).map_err(|error| error.to_string())?;
        self.snapshot();
        self.active = validated.clone();
        self.draft = validated.clone();
        self.paste_preview = None;
        self.validation = EditorValidationReport::default();
        self.draft_undo.clear();
        self.draft_redo.clear();
        self.dirty = false;
        Ok(validated)
    }

    pub fn apply_regular_preview(
        &mut self,
        grid_index: usize,
        preview: &RegularPastePreview,
        replacement: FieldReplacement,
    ) -> Result<(), String> {
        if !preview.report.is_valid() {
            return Err("regular paste preview has validation errors".into());
        }
        let existing = self
            .draft
            .grids
            .get(grid_index)
            .ok_or("selected grid no longer exists")?;
        let TabulatedGrid::Regular(regular) = existing else {
            return Err("selected grid is not regular".into());
        };
        if regular.subdivisions != preview.subdivisions {
            return Err("preview subdivision count differs from selected grid".into());
        }
        let replacement_grid = TabulatedGrid::Regular(RegularTabulatedGrid {
            name: regular.name.clone(),
            source: regular.source.clone(),
            subdivisions: preview.subdivisions,
            order: RowOrder::Canonical,
            composition_columns: CompositionColumns::None,
            compositions: preview.canonical_compositions.clone(),
            fields: fields_from_pending(&preview.fields),
        });
        self.replace_grid_or_fields(grid_index, replacement_grid, replacement)?;
        self.paste_preview = None;
        self.dirty = true;
        Ok(())
    }

    pub fn apply_irregular_preview(
        &mut self,
        grid_index: Option<usize>,
        name: String,
        preview: &IrregularPastePreview,
        replacement: FieldReplacement,
    ) -> Result<(), String> {
        if !preview.report.is_valid() {
            return Err("irregular paste preview has validation errors".into());
        }
        let replacement_grid = TabulatedGrid::Irregular(IrregularTabulatedGrid {
            name,
            source: SourceRange {
                first_line: 0,
                last_line: 0,
            },
            compositions: preview.compositions.clone(),
            fields: fields_from_pending(&preview.fields),
        });
        if let Some(index) = grid_index {
            self.replace_grid_or_fields(index, replacement_grid, replacement)?;
        } else {
            if self
                .draft
                .grids
                .iter()
                .any(|grid| grid.name() == replacement_grid.name())
            {
                return Err("a grid with that name already exists".into());
            }
            if matches!(replacement, FieldReplacement::ReplaceExistingField) {
                return Err("select a grid before replacing a field".into());
            }
            self.draft.grids.push(replacement_grid);
        }
        self.paste_preview = None;
        self.dirty = true;
        Ok(())
    }

    /// Return per-state counts for a selected grid field in declaration order:
    /// calculated, non-existing, cut-off, missing.
    pub fn field_state_counts(
        &self,
        grid_index: usize,
        field_index: usize,
    ) -> Result<[usize; 4], String> {
        let field = self
            .draft
            .grids
            .get(grid_index)
            .and_then(|grid| grid.fields().get(field_index))
            .ok_or("selected grid field no longer exists")?;
        Ok(field.values.iter().fold([0; 4], |mut counts, value| {
            counts[match value.state {
                crate::TabulatedValueState::Calculated => 0,
                crate::TabulatedValueState::NonExisting => 1,
                crate::TabulatedValueState::CutOff => 2,
                crate::TabulatedValueState::Missing => 3,
            }] += 1;
            counts
        }))
    }

    /// Whether this draft field differs from the last applied dataset.
    pub fn field_is_modified(&self, grid_index: usize, field_index: usize) -> bool {
        let Some(draft_grid) = self.draft.grids.get(grid_index) else {
            return false;
        };
        let Some(draft_field) = draft_grid.fields().get(field_index) else {
            return false;
        };
        self.active
            .grids
            .iter()
            .find(|grid| grid.name() == draft_grid.name())
            .and_then(|grid| {
                grid.fields().iter().find(|field| {
                    field.phase_id == draft_field.phase_id && field.property == draft_field.property
                })
            })
            .is_none_or(|active| active != draft_field)
    }

    /// Edit one point without applying the full draft or launching numerical work.
    pub fn set_field_point(
        &mut self,
        grid_index: usize,
        field_index: usize,
        row: usize,
        value: TabulatedValue,
    ) -> Result<(), String> {
        value.validate()?;
        let exists = self
            .draft
            .grids
            .get(grid_index)
            .and_then(|grid| grid.fields().get(field_index))
            .and_then(|field| field.values.get(row))
            .is_some();
        if !exists {
            return Err("selected grid point no longer exists".into());
        }
        self.snapshot_draft();
        *grid_field_mut(&mut self.draft.grids[grid_index], field_index)
            .and_then(|field| field.values.get_mut(row))
            .ok_or("selected grid point no longer exists")? = value;
        self.dirty = true;
        Ok(())
    }

    /// Apply one non-calculated state to a group of selected point rows.
    pub fn set_field_state_batch(
        &mut self,
        grid_index: usize,
        field_index: usize,
        rows: &[usize],
        state: crate::TabulatedValueState,
        note: Option<String>,
    ) -> Result<(), String> {
        if matches!(state, crate::TabulatedValueState::Calculated) {
            return Err(
                "batch calculated values require an explicit scalar value and confirmation".into(),
            );
        }
        let count = self
            .draft
            .grids
            .get(grid_index)
            .and_then(|grid| grid.fields().get(field_index))
            .map(|field| field.values.len())
            .ok_or("selected grid field no longer exists")?;
        if rows.iter().any(|row| *row >= count) {
            return Err("a selected point no longer exists".into());
        }
        self.snapshot_draft();
        let field = grid_field_mut(&mut self.draft.grids[grid_index], field_index)
            .ok_or("selected grid field no longer exists")?;
        for row in rows {
            field.values[*row] = TabulatedValue {
                state,
                value: None,
                note: note.clone().filter(|note| !note.trim().is_empty()),
            };
        }
        self.dirty = true;
        Ok(())
    }

    /// Clear notes from selected points without changing their classified states.
    pub fn clear_field_notes(
        &mut self,
        grid_index: usize,
        field_index: usize,
        rows: &[usize],
    ) -> Result<(), String> {
        let count = self
            .draft
            .grids
            .get(grid_index)
            .and_then(|grid| grid.fields().get(field_index))
            .map(|field| field.values.len())
            .ok_or("selected grid field no longer exists")?;
        if rows.iter().any(|row| *row >= count) {
            return Err("a selected point no longer exists".into());
        }
        self.snapshot_draft();
        let field = grid_field_mut(&mut self.draft.grids[grid_index], field_index)
            .ok_or("selected grid field no longer exists")?;
        for row in rows {
            field.values[*row].note = None;
        }
        self.dirty = true;
        Ok(())
    }

    /// Restore a selected draft field from the last applied dataset.
    pub fn revert_field(&mut self, grid_index: usize, field_index: usize) -> Result<(), String> {
        let (grid_name, phase_id, property) = self
            .draft
            .grids
            .get(grid_index)
            .and_then(|grid| {
                grid.fields().get(field_index).map(|field| {
                    (
                        grid.name().to_owned(),
                        field.phase_id,
                        field.property.clone(),
                    )
                })
            })
            .ok_or("selected grid field no longer exists")?;
        let values = self
            .active
            .grids
            .iter()
            .find(|grid| grid.name() == grid_name)
            .and_then(|grid| {
                grid.fields()
                    .iter()
                    .find(|field| field.phase_id == phase_id && field.property == property)
            })
            .map(|field| (field.values.clone(), field.row_lines.clone()))
            .ok_or("selected field does not exist in the applied dataset")?;
        self.snapshot_draft();
        let field = grid_field_mut(&mut self.draft.grids[grid_index], field_index)
            .ok_or("selected grid field no longer exists")?;
        field.values = values.0;
        field.row_lines = values.1;
        self.dirty = self.draft != self.active;
        Ok(())
    }

    fn snapshot_draft(&mut self) {
        self.draft_undo.push(self.draft.clone());
        if self.draft_undo.len() > self.history_limit {
            self.draft_undo.remove(0);
        }
        self.draft_redo.clear();
    }

    pub fn phase_field_references(&self, index: usize) -> Vec<String> {
        let Some(phase) = self.draft.phases.get(index) else {
            return Vec::new();
        };
        self.draft
            .grids
            .iter()
            .flat_map(|grid| {
                grid.fields()
                    .iter()
                    .filter(|field| field.phase_id == phase.id)
                    .map(|field| format!("{}.{}", grid.name(), field.column_name))
            })
            .collect()
    }

    pub fn property_field_references(&self, index: usize) -> Vec<String> {
        let Some(property) = self.draft.properties.get(index) else {
            return Vec::new();
        };
        self.draft
            .grids
            .iter()
            .flat_map(|grid| {
                grid.fields()
                    .iter()
                    .filter(|field| field.property == property.name)
                    .map(|field| format!("{}.{}", grid.name(), field.column_name))
            })
            .collect()
    }

    pub fn reorder_phase(&mut self, index: usize, delta: isize) -> Result<(), String> {
        let target = index as isize + delta;
        if target < 0 || target >= self.draft.phases.len() as isize {
            return Err("phase is already at that boundary".into());
        }
        self.snapshot_draft();
        self.draft.phases.swap(index, target as usize);
        self.dirty = true;
        Ok(())
    }

    pub fn add_phase(&mut self) -> Result<(), String> {
        self.snapshot_draft();
        let mut id = 1;
        while self.draft.phases.iter().any(|phase| phase.id.0 == id) {
            id += 1;
        }
        let mut name = format!("phase_{id}");
        let mut suffix = 2;
        while self.draft.phases.iter().any(|phase| phase.name == name) {
            name = format!("phase_{id}_{suffix}");
            suffix += 1;
        }
        self.draft.phases.push(crate::PhaseDefinition {
            name,
            id: StablePhaseId(id),
            line: 0,
        });
        self.dirty = true;
        Ok(())
    }

    pub fn remove_phase(&mut self, index: usize) -> Result<(), String> {
        if self.phase_field_references(index).is_empty() {
            self.remove_phase_confirmed(index)
        } else {
            Err("phase is referenced by grid fields; confirm removal first".into())
        }
    }

    pub fn remove_phase_confirmed(&mut self, index: usize) -> Result<(), String> {
        if self.draft.phases.len() <= 1 {
            return Err("a dataset must retain at least one phase".into());
        }
        if index >= self.draft.phases.len() {
            return Err("selected phase no longer exists".into());
        }
        self.snapshot_draft();
        let phase_id = self.draft.phases[index].id;
        self.draft.phases.remove(index);
        for grid in &mut self.draft.grids {
            match grid {
                TabulatedGrid::Regular(value) => {
                    value.fields.retain(|field| field.phase_id != phase_id)
                }
                TabulatedGrid::Irregular(value) => {
                    value.fields.retain(|field| field.phase_id != phase_id)
                }
            }
        }
        self.dirty = true;
        Ok(())
    }

    pub fn reorder_property(&mut self, index: usize, delta: isize) -> Result<(), String> {
        let target = index as isize + delta;
        if target < 0 || target >= self.draft.properties.len() as isize {
            return Err("property is already at that boundary".into());
        }
        self.snapshot_draft();
        self.draft.properties.swap(index, target as usize);
        self.dirty = true;
        Ok(())
    }

    pub fn add_property(&mut self) -> Result<(), String> {
        self.snapshot_draft();
        let mut suffix = 1;
        let mut name = format!("property_{suffix}");
        while self
            .draft
            .properties
            .iter()
            .any(|property| property.name == name)
        {
            suffix += 1;
            name = format!("property_{suffix}");
        }
        self.draft.properties.push(crate::PropertyDefinition {
            name,
            required: false,
            unit: "1".into(),
            line: 0,
        });
        self.dirty = true;
        Ok(())
    }

    pub fn remove_property(&mut self, index: usize) -> Result<(), String> {
        let Some(property) = self.draft.properties.get(index) else {
            return Err("selected property no longer exists".into());
        };
        if property.name == "T" {
            return Err("property T cannot be removed".into());
        }
        if self.property_field_references(index).is_empty() {
            self.remove_property_confirmed(index)
        } else {
            Err("property is referenced by grid fields; confirm removal first".into())
        }
    }

    pub fn remove_property_confirmed(&mut self, index: usize) -> Result<(), String> {
        let Some(property_name) = self
            .draft
            .properties
            .get(index)
            .filter(|property| property.name != "T")
            .map(|property| property.name.clone())
        else {
            return Err("property T cannot be removed".into());
        };
        self.snapshot_draft();
        self.draft.properties.remove(index);
        for grid in &mut self.draft.grids {
            match grid {
                TabulatedGrid::Regular(value) => {
                    value.fields.retain(|field| field.property != property_name)
                }
                TabulatedGrid::Irregular(value) => {
                    value.fields.retain(|field| field.property != property_name)
                }
            }
        }
        self.dirty = true;
        Ok(())
    }
    pub fn regenerate_regular_grid(
        &mut self,
        grid_index: usize,
        subdivisions: usize,
    ) -> Result<(), String> {
        let compositions = regular_compositions(subdivisions)?;
        self.snapshot_draft();
        let Some(TabulatedGrid::Regular(grid)) = self.draft.grids.get_mut(grid_index) else {
            return Err("selected grid is not regular".into());
        };
        grid.compositions = compositions;
        grid.subdivisions = subdivisions;
        grid.order = RowOrder::Canonical;
        grid.composition_columns = CompositionColumns::None;
        for field in &mut grid.fields {
            field.values = vec![TabulatedValue::missing(); grid.compositions.len()];
            field.row_lines = vec![0; grid.compositions.len()];
        }
        self.dirty = true;
        Ok(())
    }
    pub fn can_draft_undo(&self) -> bool {
        !self.draft_undo.is_empty()
    }
    pub fn can_draft_redo(&self) -> bool {
        !self.draft_redo.is_empty()
    }
    pub fn draft_undo(&mut self) -> bool {
        let Some(previous) = self.draft_undo.pop() else {
            return false;
        };
        self.draft_redo.push(self.draft.clone());
        self.draft = previous;
        self.dirty = self.draft != self.active;
        true
    }

    pub fn draft_redo(&mut self) -> bool {
        let Some(next) = self.draft_redo.pop() else {
            return false;
        };
        self.draft_undo.push(self.draft.clone());
        self.draft = next;
        self.dirty = self.draft != self.active;
        true
    }
    pub fn add_grid(&mut self, grid: TabulatedGrid) -> Result<(), String> {
        if self
            .draft
            .grids
            .iter()
            .any(|candidate| candidate.name() == grid.name())
        {
            return Err(format!("grid '{}' already exists", grid.name()));
        }
        self.snapshot_draft();
        self.draft.grids.push(grid);
        self.dirty = true;
        Ok(())
    }

    pub fn remove_grid(&mut self, index: usize) -> Result<(), String> {
        if self.draft.grids.len() <= 1 {
            return Err("a dataset must retain at least one grid".into());
        }
        if index >= self.draft.grids.len() {
            return Err("selected grid no longer exists".into());
        }
        self.snapshot_draft();
        self.draft.grids.remove(index);
        self.dirty = true;
        Ok(())
    }

    fn replace_grid_or_fields(
        &mut self,
        index: usize,
        replacement_grid: TabulatedGrid,
        replacement: FieldReplacement,
    ) -> Result<(), String> {
        let current = self
            .draft
            .grids
            .get_mut(index)
            .ok_or("selected grid no longer exists")?;
        if matches!(replacement, FieldReplacement::ReplaceEntireGrid) {
            *current = replacement_grid;
            return Ok(());
        }
        if current.grid_type() != replacement_grid.grid_type()
            || current.compositions() != replacement_grid.compositions()
        {
            return Err(
                "field replacement requires matching compositions; choose Replace entire grid"
                    .into(),
            );
        }
        for field in replacement_grid.fields() {
            let found = current.fields().iter().position(|candidate| {
                candidate.phase_id == field.phase_id && candidate.property == field.property
            });
            match (replacement, found) {
                (FieldReplacement::AddNewField, Some(_)) => {
                    return Err(format!(
                        "field {}.{} exists; choose Replace existing field",
                        field.phase_id.0, field.property
                    ));
                }
                (FieldReplacement::ReplaceExistingField, None) => {
                    return Err(format!(
                        "field {}.{} is new; choose Add new field",
                        field.phase_id.0, field.property
                    ));
                }
                (_, Some(found)) => mutate_field(current, found, field.clone()),
                (_, None) => push_field(current, field.clone()),
            }
        }
        Ok(())
    }

    fn snapshot(&mut self) {
        self.undo.push(self.active.clone());
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}

fn fields_from_pending(fields: &[PendingField]) -> Vec<TabulatedField> {
    fields
        .iter()
        .map(|field| TabulatedField {
            phase_id: field.key.phase_id,
            property: field.key.property.clone(),
            column_name: field.column_name.clone(),
            values: field.values.clone(),
            row_lines: vec![0; field.values.len()],
        })
        .collect()
}

fn grid_field_mut(grid: &mut TabulatedGrid, index: usize) -> Option<&mut TabulatedField> {
    match grid {
        TabulatedGrid::Regular(value) => value.fields.get_mut(index),
        TabulatedGrid::Irregular(value) => value.fields.get_mut(index),
    }
}

fn mutate_field(grid: &mut TabulatedGrid, index: usize, field: TabulatedField) {
    match grid {
        TabulatedGrid::Regular(value) => value.fields[index] = field,
        TabulatedGrid::Irregular(value) => value.fields[index] = field,
    }
}

fn push_field(grid: &mut TabulatedGrid, field: TabulatedField) {
    match grid {
        TabulatedGrid::Regular(value) => value.fields.push(field),
        TabulatedGrid::Irregular(value) => value.fields.push(field),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_str;

    fn field(column: usize, property: &str) -> FieldColumnMapping {
        FieldColumnMapping {
            source_column: column,
            destination: FieldKey {
                phase_id: StablePhaseId(10),
                property: property.into(),
            },
            label: format!("alpha.{property}"),
        }
    }

    #[test]
    fn canonical_generation_and_tsv_use_the_numerical_grid() {
        for subdivisions in 1..=12 {
            assert_eq!(
                regular_compositions(subdivisions).unwrap(),
                RegularTernaryGrid::new(subdivisions)
                    .unwrap()
                    .compositions()
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(
            compositions_tsv(
                2,
                ["A", "B", "C"],
                true,
                NumericFormat { decimal_places: 1 }
            )
            .unwrap(),
            "A\tB\tC\n0.0\t0.0\t1.0\n0.0\t0.5\t0.5\n0.0\t1.0\t0.0\n0.5\t0.0\t0.5\n0.5\t0.5\t0.0\n1.0\t0.0\t0.0\n"
        );
    }

    #[test]
    fn regular_values_guidance_and_authoritative_mapping_are_checked() {
        let values = preview_regular_paste(
            "1\n2\n3\n4\n5\n6",
            2,
            &RegularPasteMapping {
                header_mode: HeaderMode::Absent,
                fields: vec![field(0, "T")],
                ..Default::default()
            },
        );
        assert!(values.report.is_valid());
        let guidance = preview_regular_paste(
            "0\t0\t1\t1\n0\t0.5\t0.5\t2\n0\t1\t0\t3\n0.5\t0\t0.5\t4\n0.5\t0.5\t0\t5\n1\t0\t0\t6",
            2,
            &RegularPasteMapping {
                header_mode: HeaderMode::Absent,
                composition_mode: RegularCompositionPasteMode::Guidance,
                composition_columns: Some([0, 1, 2]),
                fields: vec![field(3, "T")],
                ..Default::default()
            },
        );
        assert!(guidance.report.is_valid());
        let shuffled = preview_regular_paste(
            "1\t0\t0\t6\n0\t0\t1\t1\n0\t0.5\t0.5\t2\n0\t1\t0\t3\n0.5\t0\t0.5\t4\n0.5\t0.5\t0\t5",
            2,
            &RegularPasteMapping {
                header_mode: HeaderMode::Absent,
                composition_mode: RegularCompositionPasteMode::Authoritative,
                composition_columns: Some([0, 1, 2]),
                fields: vec![field(3, "T")],
                ..Default::default()
            },
        );
        assert!(shuffled.report.is_valid());
        assert_eq!(shuffled.fields[0].values, values.fields[0].values);
        assert!(shuffled.report.canonical_reorder_count > 0);
    }

    #[test]
    fn irregular_validation_and_dataset_undo_are_safe() {
        let malformed = preview_irregular_paste(
            "0\t0\t2\t1\n0\t1\t0\t2\n1\t0\t0\t3",
            &IrregularPasteMapping {
                header_mode: HeaderMode::Absent,
                fields: vec![field(3, "T")],
                ..Default::default()
            },
        );
        assert!(!malformed.report.is_valid());
        let valid = preview_irregular_paste(
            "A\tB\tC\talpha.T\talpha.activity\n1.000000001\t0\t0\t1200\t0.2\n0\t1\t0\t1100\tNA\n0\t0\t1\t1000\t0.4",
            &IrregularPasteMapping {
                header_mode: HeaderMode::Present,
                fields: vec![field(3, "T"), field(4, "activity")],
                ..Default::default()
            },
        );
        assert!(valid.report.is_valid(), "{}", valid.report.as_text());
        assert_eq!(valid.report.normalized_rows, 1);

        let original = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let mut state = DatasetEditorState::new(original.clone());
        let regular = preview_regular_paste(
            "1\n2\n3\n4\n5\n6",
            2,
            &RegularPasteMapping {
                header_mode: HeaderMode::Absent,
                fields: vec![field(0, "T")],
                ..Default::default()
            },
        );
        state.set_preview(PastePreview::Regular(regular.clone()));
        assert_eq!(state.active, original);
        state
            .apply_regular_preview(0, &regular, FieldReplacement::ReplaceExistingField)
            .unwrap();
        let applied = state.apply_draft().unwrap();
        assert_ne!(applied, original);
        assert!(state.undo());
        assert_eq!(state.active, original);
        assert!(state.redo());
        assert_eq!(state.active, applied);
    }

    #[test]
    fn declaration_controls_preserve_ids_and_protect_referenced_fields() {
        let mut state = DatasetEditorState::new(crate::default_regular_dataset());
        state.add_phase().unwrap();
        let added_id = state.draft.phases.last().unwrap().id;
        state.reorder_phase(3, -1).unwrap();
        assert_eq!(state.draft.phases[2].id, added_id);
        assert!(state.remove_phase(2).is_ok());
        assert!(state.remove_property(0).is_err());
        state.add_property().unwrap();
        state.reorder_property(1, -1).unwrap();
        assert_eq!(state.draft.properties[0].name, "property_1");
        assert!(state.draft.validate_structure().is_ok());
        assert!(state.draft_undo());
        assert!(state.draft_redo());
    }

    #[test]
    fn classified_tsv_paste_and_point_edits_preserve_states_and_undo() {
        let preview = preview_regular_paste(
            "100\nNE\nCO:3000\nNA\n101\n102",
            2,
            &RegularPasteMapping {
                header_mode: HeaderMode::Absent,
                fields: vec![field(0, "T")],
                ..Default::default()
            },
        );
        assert!(preview.report.is_valid(), "{}", preview.report.as_text());
        assert_eq!(preview.fields[0].values[0].calculated_value(), Some(100.0));
        assert_eq!(
            preview.fields[0].values[1].state,
            crate::TabulatedValueState::NonExisting
        );
        assert_eq!(
            preview.fields[0].values[2].state,
            crate::TabulatedValueState::CutOff
        );
        assert_eq!(preview.fields[0].values[2].note.as_deref(), Some("3000"));

        let mut state = DatasetEditorState::new(crate::default_regular_dataset());
        state
            .set_field_point(0, 0, 0, TabulatedValue::calculated(1200.0).unwrap())
            .unwrap();
        state
            .set_field_state_batch(
                0,
                0,
                &[1, 2],
                crate::TabulatedValueState::CutOff,
                Some("3000".into()),
            )
            .unwrap();
        assert_eq!(state.field_state_counts(0, 0).unwrap(), [1, 0, 2, 63]);
        assert_eq!(
            state.draft.grids[0].fields()[0].values[1].note.as_deref(),
            Some("3000")
        );
        assert!(state.draft_undo());
        assert_eq!(state.field_state_counts(0, 0).unwrap(), [1, 0, 0, 65]);
        state.revert_field(0, 0).unwrap();
        assert_eq!(state.field_state_counts(0, 0).unwrap(), [0, 0, 0, 66]);
    }

    #[test]
    fn regular_resolution_regeneration_is_validated_and_clears_values() {
        let mut state = DatasetEditorState::new(crate::default_regular_dataset());
        assert!(regular_row_count(0).is_err());
        assert!(regular_row_count(20).is_ok());
        let TabulatedGrid::Regular(grid) = &mut state.draft.grids[0] else {
            panic!("expected regular grid");
        };
        grid.fields[0].values[0] = TabulatedValue::calculated(1200.0).unwrap();
        state.regenerate_regular_grid(0, 20).unwrap();
        let TabulatedGrid::Regular(grid) = &state.draft.grids[0] else {
            panic!("expected regular grid");
        };
        assert_eq!(grid.compositions.len(), 231);
        assert!(grid.fields.iter().all(|field| {
            field
                .values
                .iter()
                .all(|value| value.state == crate::TabulatedValueState::Missing)
        }));
        assert!(state.draft_undo());
        let TabulatedGrid::Regular(grid) = &state.draft.grids[0] else {
            panic!("expected regular grid");
        };
        assert_eq!(grid.compositions.len(), 66);
    }
}
