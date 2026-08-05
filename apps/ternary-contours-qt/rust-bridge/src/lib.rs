//! Rust-owned document state for the Qt Widgets application.
//!
//! Qt receives copies of typed snapshots and submits small mutations. TCT
//! parsing, serialization, validation, classified values, and calculations stay
//! in Rust; no second document parser or NaN-based missing representation exists
//! in the C++ layer.

#![allow(clippy::missing_safety_doc)]

use std::{
    ffi::CStr,
    os::raw::c_char,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use ternary_contours::{
    BinaryExtrapolation, CubicAlphaMethod, CubicPartialDomainPolicy, RegularTernaryGrid,
    StablePhaseId,
};
use ternary_contours_cli::{
    CompositionColumns, GridType, HeaderMode, IrregularTabulatedGrid, LiquidusProjection,
    OutputFormat, ParsedTable, PhaseDefinition, ProjectionCsvOptions, ProjectionCsvRecord,
    ProjectionOptions, PropertyDefinition, RegularTabulatedGrid, RenderOptions, RowOrder,
    SourceRange, TabulatedField, TabulatedGrid, TabulatedTernaryDataset, TabulatedValue,
    TabulatedValueState, TctSerializeOptions, automatic_iso_levels, calculate_projection,
    empty_project_dataset,
    interpolation_inspection::{
        FieldInspectionCache, InspectionFieldIdentity, InterpolatedResultState,
    },
    parse_path, parse_tabulated_value_token, projection_csv_records, render_to_path,
    save_tct_atomic, serialize_projection_csv, serialize_tct,
    validate_new_regular_grid_subdivisions,
};
use ternary_contours_gui_core::{GuiContractState, Revision, UiAction, UiEffect, update};

const NAME: usize = 128;
const PATH: usize = 512;
const MESSAGE: usize = 512;

#[repr(C)]
pub struct TcqtStatus {
    pub success: bool,
    pub message: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtCalculationResult {
    pub success: bool,
    pub request_id: u64,
    pub vertex_count: u32,
    pub message: [u8; 128],
}
/// Numerical configuration shared by the Qt Viewer and the Rust projection.
/// Enum values are deliberately explicit across the C ABI; invalid values are
/// rejected by the bridge instead of falling back silently.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TcqtViewerCalculationOptions {
    pub automatic_range: bool,
    pub minimum: f64,
    pub maximum: f64,
    pub level_step: f64,
    pub sampling_subdivisions: u32,
    pub regularize: bool,
    pub regularization_spacing: f64,
    /// 0 = linear, 1 = cubic alpha.
    pub source_interpolation: u32,
    /// 0 = Akima, 1 = Makima, 2 = PCHIP, 3 = Steffen.
    pub cubic_method: u32,
    /// 0 = strict, 1 = one-sided, 2 = one-sided then linear,
    /// 3 = linear near boundaries.
    pub partial_domain_policy: u32,
    /// 0 = raw barycentric, 1 = Muggianu, 2 = Kohler.
    pub continuation: u32,
}

#[repr(C)]
pub struct TcqtProjectionSummary {
    pub available: bool,
    pub source_minimum: f64,
    pub source_maximum: f64,
    pub automatic_minimum: f64,
    pub automatic_used_invariant: bool,
    pub level_count: u32,
    pub invariant_count: u32,
    pub univariant_count: u32,
    pub contour_path_count: u32,
    pub message: [u8; MESSAGE],
}

/// Result of a single authoritative source-field interpolation query. Source
/// rows are zero-based at the ABI and are presented as one-based rows by Qt.
#[repr(C)]
pub struct TcqtInspectionResult {
    pub success: bool,
    /// 0 defined, 1 missing, 2 non-existing, 3 cut-off, 4 triangle
    /// unavailable, 5 outside domain, 6 evaluation error.
    pub state: u32,
    pub has_value: bool,
    pub value: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub triangle_index: u32,
    pub has_local_barycentric: bool,
    pub lambda0: f64,
    pub lambda1: f64,
    pub lambda2: f64,
    pub has_contributions: bool,
    pub linear_part: f64,
    pub excess_part: f64,
    pub has_source_rows: bool,
    pub source_row0: u32,
    pub source_row1: u32,
    pub source_row2: u32,
    /// 0 cubic, 1 one-sided cubic, 2 linear fallback, 3 undefined.
    pub local_mode: u32,
    pub unit: [u8; NAME],
    pub message: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtProjectSummary {
    pub title: [u8; NAME],
    pub path: [u8; PATH],
    pub component_a: [u8; NAME],
    pub component_b: [u8; NAME],
    pub component_c: [u8; NAME],
    pub phase_count: u32,
    pub property_count: u32,
    pub grid_count: u32,
    pub dirty: bool,
    pub revision: u64,
    pub saved_revision: u64,
    /// 0 invalid, 1 draft-valid, 2 calculation-ready.
    pub validity: u32,
    pub saveable: bool,
    pub calculation_available: bool,
    pub blocking_reason: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtSaveResult {
    /// 0 saved, 1 invalid document, 2 serialization failure, 3 write failure.
    pub outcome: u32,
    pub message: [u8; MESSAGE],
    pub path: [u8; PATH],
}
#[repr(C)]
pub struct TcqtPasteResult {
    pub success: bool,
    pub rows_pasted: u32,
    pub columns_pasted: u32,
    pub rows_appended: u32,
    pub header_skipped: bool,
    /// 1-based clipboard source row/column for an error, or zero on success.
    pub clipboard_row: u32,
    pub clipboard_column: u32,
    /// 1-based destination table row/column for an error, or zero on success.
    pub target_row: u32,
    pub target_column: u32,
    pub message: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtPhase {
    pub id: u32,
    pub name: [u8; NAME],
}
#[repr(C)]
pub struct TcqtProperty {
    pub ordinal: u32,
    pub required: bool,
    pub name: [u8; NAME],
    pub unit: [u8; NAME],
}
#[repr(C)]
pub struct TcqtGrid {
    pub index: u32,
    pub kind: u32,
    pub subdivisions: u32,
    pub row_count: u32,
    pub field_count: u32,
    pub name: [u8; NAME],
}
#[repr(C)]
pub struct TcqtField {
    pub index: u32,
    pub phase_id: u32,
    pub property: [u8; NAME],
    pub column_name: [u8; NAME],
}
#[repr(C)]
pub struct TcqtRow {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}
#[repr(C)]
pub struct TcqtCell {
    /// 0 calculated, 1 non-existing, 2 cut-off, 3 missing. Undefined cells
    /// use has_value=false and value=0.0, never NaN.
    pub state: u32,
    pub has_value: bool,
    pub value: f64,
    pub note: [u8; NAME],
}

/// A projected polyline point exported by the Rust calculation pipeline.
///
/// `point_index` is the position within `line_id`; it lets the Qt canvas retain
/// the exact path boundaries rather than inferring them from coordinates.
#[repr(C)]
pub struct TcqtProjectionRecord {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub point_index: u32,
    pub line_type: u32,
    pub line_id: [u8; NAME],
}

struct ProjectDocument {
    dataset: TabulatedTernaryDataset,
    saved_dataset: TabulatedTernaryDataset,
    path: Option<PathBuf>,
    dirty: bool,
    revision: u64,
    saved_revision: u64,
    undo: Vec<TabulatedTernaryDataset>,
    redo: Vec<TabulatedTernaryDataset>,
    contract: GuiContractState,
    projection: Option<LiquidusProjection>,
    projection_records: Vec<ProjectionCsvRecord>,
    inspection_cache: FieldInspectionCache,
    calculation_generation: u64,
}
impl ProjectDocument {
    fn new() -> Self {
        Self {
            dataset: empty_project_dataset(),
            saved_dataset: empty_project_dataset(),
            path: None,
            dirty: false,
            revision: 1,
            saved_revision: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            contract: GuiContractState::default(),
            projection: None,
            projection_records: Vec::new(),
            inspection_cache: FieldInspectionCache::default(),
            calculation_generation: 0,
        }
    }
    fn invalidate_calculation(&mut self) {
        self.projection = None;
        self.projection_records.clear();
        self.inspection_cache.invalidate();
        self.calculation_generation = self.calculation_generation.saturating_add(1);
    }
    fn mark_revision_changed(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.dirty = self.dataset != self.saved_dataset;
        self.contract.revisions.dataset = Revision(self.revision);
    }

    fn mutate(
        &mut self,
        edit: impl FnOnce(&mut TabulatedTernaryDataset) -> Result<(), String>,
    ) -> Result<(), String> {
        let prior = self.dataset.clone();
        if let Err(error) = edit(&mut self.dataset) {
            self.dataset = prior;
            return Err(error);
        }
        if let Err(error) = self.dataset.validate_document_structure() {
            self.dataset = prior;
            return Err(error);
        }
        self.undo.push(prior);
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.invalidate_calculation();
        self.mark_revision_changed();
        let _ = update(&mut self.contract, UiAction::DatasetEdited);
        Ok(())
    }
    fn replace_loaded(
        &mut self,
        mut dataset: TabulatedTernaryDataset,
        path: PathBuf,
    ) -> Result<(), String> {
        dataset.validate_saveable_document()?;
        dataset.source_path = Some(path.clone());
        self.dataset = dataset;
        self.saved_dataset = self.dataset.clone();
        self.path = Some(path);
        self.revision = self.revision.saturating_add(1);
        self.saved_revision = self.revision;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.contract = GuiContractState::default();
        self.invalidate_calculation();
        self.contract.revisions.dataset = Revision(self.revision);
        Ok(())
    }
}
fn document() -> &'static Mutex<ProjectDocument> {
    static INSTANCE: OnceLock<Mutex<ProjectDocument>> = OnceLock::new();
    INSTANCE.get_or_init(|| Mutex::new(ProjectDocument::new()))
}
fn bytes<const N: usize>(value: &str) -> [u8; N] {
    let mut output = [0; N];
    let length = value.len().min(N.saturating_sub(1));
    output[..length].copy_from_slice(&value.as_bytes()[..length]);
    output
}
fn status(result: Result<(), String>) -> TcqtStatus {
    match result {
        Ok(()) => TcqtStatus {
            success: true,
            message: bytes("OK"),
        },
        Err(error) => TcqtStatus {
            success: false,
            message: bytes(&error),
        },
    }
}
fn viewer_options(raw: &TcqtViewerCalculationOptions) -> Result<ProjectionOptions, String> {
    let source_interpolation = match raw.source_interpolation {
        0 => ternary_contours_cli::SourceInterpolation::Linear,
        1 => ternary_contours_cli::SourceInterpolation::CubicAlpha {
            method: match raw.cubic_method {
                0 => CubicAlphaMethod::Akima,
                1 => CubicAlphaMethod::Makima,
                2 => CubicAlphaMethod::Pchip,
                3 => CubicAlphaMethod::Steffen,
                _ => return Err("unsupported cubic slope method".into()),
            },
            continuation: match raw.continuation {
                0 => BinaryExtrapolation::RawBarycentric,
                1 => BinaryExtrapolation::Muggianu,
                2 => BinaryExtrapolation::Kohler,
                _ => return Err("unsupported ternary continuation".into()),
            },
        },
        _ => return Err("unsupported source interpolation".into()),
    };
    let partial_domain_policy = match raw.partial_domain_policy {
        0 => CubicPartialDomainPolicy::Strict,
        1 => CubicPartialDomainPolicy::OneSided,
        2 => CubicPartialDomainPolicy::OneSidedThenLinear,
        3 => CubicPartialDomainPolicy::LinearNearDomain,
        _ => return Err("unsupported partial-domain policy".into()),
    };
    if !raw.level_step.is_finite() || raw.level_step <= 0.0 {
        return Err("isotherm step must be finite and positive".into());
    }
    let levels = if raw.automatic_range {
        Vec::new()
    } else {
        automatic_iso_levels(raw.minimum, raw.maximum, raw.level_step)
            .map_err(|error| format!("invalid manual isotherm range: {error}"))?
    };
    if raw.regularization_spacing.is_finite() && raw.regularization_spacing <= 0.0 {
        return Err("regularization spacing must be positive when supplied".into());
    }
    Ok(ProjectionOptions {
        levels,
        automatic_level_step: raw.automatic_range.then_some(raw.level_step),
        sampling_subdivisions: (raw.sampling_subdivisions != 0)
            .then_some(raw.sampling_subdivisions as usize),
        regularize: raw.regularize,
        regularization_spacing: (raw.regularization_spacing > 0.0)
            .then_some(raw.regularization_spacing),
        source_interpolation,
        partial_domain_policy,
    })
}

fn default_viewer_options() -> TcqtViewerCalculationOptions {
    TcqtViewerCalculationOptions {
        automatic_range: true,
        minimum: 0.0,
        maximum: 0.0,
        level_step: 100.0,
        sampling_subdivisions: 0,
        regularize: true,
        regularization_spacing: 0.0,
        source_interpolation: 0,
        cubic_method: 3,
        partial_domain_policy: 2,
        continuation: 1,
    }
}

fn inspection_state(state: &InterpolatedResultState) -> u32 {
    match state {
        InterpolatedResultState::Defined => 0,
        InterpolatedResultState::UndefinedMissing => 1,
        InterpolatedResultState::UndefinedNonExisting => 2,
        InterpolatedResultState::UndefinedCutOff => 3,
        InterpolatedResultState::TriangleUnavailable => 4,
        InterpolatedResultState::OutsideDomain => 5,
        InterpolatedResultState::Error(_) => 6,
    }
}

fn local_mode(mode: Option<ternary_contours::LocalInterpolationMode>) -> u32 {
    use ternary_contours::LocalInterpolationMode;
    match mode {
        Some(LocalInterpolationMode::Linear) => 2,
        Some(LocalInterpolationMode::Cubic) => 0,
        Some(LocalInterpolationMode::OneSidedCubic) => 1,
        Some(LocalInterpolationMode::LinearFallback) => 2,
        Some(LocalInterpolationMode::Undefined) | None => 3,
    }
}
fn save_result(outcome: u32, message: impl AsRef<str>, path: impl AsRef<str>) -> TcqtSaveResult {
    TcqtSaveResult {
        outcome,
        message: bytes(message.as_ref()),
        path: bytes(path.as_ref()),
    }
}
#[derive(Debug)]
struct PasteFailure {
    message: String,
    clipboard_row: usize,
    clipboard_column: usize,
    target_row: usize,
    target_column: usize,
}

impl PasteFailure {
    fn new(
        message: impl Into<String>,
        clipboard_row: usize,
        clipboard_column: usize,
        target_row: usize,
        target_column: usize,
    ) -> Self {
        Self {
            message: message.into(),
            clipboard_row,
            clipboard_column,
            target_row,
            target_column,
        }
    }

    fn result(self) -> TcqtPasteResult {
        TcqtPasteResult {
            success: false,
            rows_pasted: 0,
            columns_pasted: 0,
            rows_appended: 0,
            header_skipped: false,
            clipboard_row: self.clipboard_row as u32,
            clipboard_column: self.clipboard_column as u32,
            target_row: self.target_row as u32,
            target_column: self.target_column as u32,
            message: bytes(&self.message),
        }
    }
}

fn paste_success(
    rows_pasted: usize,
    columns_pasted: usize,
    rows_appended: usize,
    header_skipped: bool,
    grid_name: &str,
) -> TcqtPasteResult {
    let message = if rows_appended > 0 {
        format!(
            "Pasted {} rows x {} columns and added {} irregular-grid rows to grid \"{}\".",
            rows_pasted, columns_pasted, rows_appended, grid_name
        )
    } else {
        format!(
            "Pasted {} rows x {} columns into grid \"{}\".",
            rows_pasted, columns_pasted, grid_name
        )
    };
    TcqtPasteResult {
        success: true,
        rows_pasted: rows_pasted as u32,
        columns_pasted: columns_pasted as u32,
        rows_appended: rows_appended as u32,
        header_skipped,
        clipboard_row: 0,
        clipboard_column: 0,
        target_row: 0,
        target_column: 0,
        message: bytes(&message),
    }
}

fn paste_headers(
    dataset: &TabulatedTernaryDataset,
    grid_index: usize,
) -> Result<Vec<String>, PasteFailure> {
    let grid = dataset
        .grids
        .get(grid_index)
        .ok_or_else(|| PasteFailure::new("selected grid is unavailable", 0, 0, 0, 0))?;
    let mut headers = dataset
        .components
        .iter()
        .map(|component| component.name.clone())
        .collect::<Vec<_>>();
    headers.extend(grid.fields().iter().map(|field| field.column_name.clone()));
    Ok(headers)
}

fn parse_pasted_value(token: &str, missing_tokens: &[String]) -> Result<TabulatedValue, String> {
    if token.trim().is_empty() {
        return Err("Blank cells are not accepted. Use NA to represent a missing value.".into());
    }
    parse_tabulated_value_token(token, missing_tokens, false)
        .map_err(|_| "Unsupported value. Enter a finite number, NA, NE, or CO.".to_owned())
}

fn parse_pasted_composition(token: &str) -> Result<f64, String> {
    let value = token
        .trim()
        .parse::<f64>()
        .map_err(|_| "Composition must be a finite non-negative number.".to_owned())?;
    if !value.is_finite() || value < 0.0 {
        return Err("Composition must be a finite non-negative number.".into());
    }
    Ok(value)
}

fn validate_pasted_irregular_compositions(
    compositions: &[[f64; 3]],
) -> Result<(), (usize, String)> {
    for (row, point) in compositions.iter().enumerate() {
        if point.iter().any(|value| !value.is_finite() || *value < 0.0)
            || (point.iter().sum::<f64>() - 1.0).abs() > 1.0e-8
        {
            return Err((
                row + 1,
                "irregular compositions must be finite, non-negative, and sum to one".into(),
            ));
        }
        for (previous, other) in compositions[..row].iter().enumerate() {
            if point
                .iter()
                .zip(other)
                .map(|(left, right)| (left - right).abs())
                .fold(0.0, f64::max)
                <= 1.0e-10
            {
                return Err((
                    row + 1,
                    format!(
                        "duplicate or near-duplicate composition matches row {}",
                        previous + 1
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn prepare_grid_paste(
    dataset: &TabulatedTernaryDataset,
    grid_index: usize,
    start_row: usize,
    start_column: usize,
    clipboard: &str,
) -> Result<(TabulatedTernaryDataset, usize, usize, usize, bool, String), PasteFailure> {
    let headers = paste_headers(dataset, grid_index)?;
    if start_column >= headers.len() {
        return Err(PasteFailure::new(
            "destination column is outside the selected grid",
            0,
            0,
            start_row + 1,
            start_column + 1,
        ));
    }
    let table = ParsedTable::parse_tsv(clipboard, HeaderMode::Absent).map_err(|error| {
        let (row, column) = error
            .location
            .map(|location| (location.row, location.column))
            .unwrap_or((0, 0));
        PasteFailure::new(
            error.message,
            row,
            column,
            start_row + row,
            start_column + column,
        )
    })?;
    let width = table.width();
    if width == 0 || start_column + width > headers.len() {
        return Err(PasteFailure::new(
            format!(
                "The clipboard contains {} columns, but only {} columns are available from the selected destination.",
                width,
                headers.len().saturating_sub(start_column)
            ),
            0,
            0,
            start_row + 1,
            start_column + 1,
        ));
    }
    let mut rows = table.rows;
    let header_skipped = rows.first().is_some_and(|row| {
        row.cells.len() == width
            && row
                .cells
                .iter()
                .enumerate()
                .all(|(offset, cell)| cell.text == headers[start_column + offset])
    });
    if header_skipped {
        rows.remove(0);
    }
    if rows.is_empty() {
        return Err(PasteFailure::new(
            "Clipboard contains a header but no data rows.",
            1,
            1,
            start_row + 1,
            start_column + 1,
        ));
    }
    let grid = dataset
        .grids
        .get(grid_index)
        .ok_or_else(|| PasteFailure::new("selected grid is unavailable", 0, 0, 0, 0))?;
    let existing_rows = grid.compositions().len();
    let mut candidate = dataset.clone();
    let mut rows_appended = 0;
    if matches!(grid, TabulatedGrid::Regular(_)) {
        if start_column < 3 {
            return Err(PasteFailure::new(
                "Paste cannot modify the composition columns of a regular grid. Select the first phase/property column and try again.",
                0,
                0,
                start_row + 1,
                start_column + 1,
            ));
        }
        if start_row + rows.len() > existing_rows {
            return Err(PasteFailure::new(
                format!(
                    "The clipboard contains {} rows, but only {} rows are available from the selected destination.",
                    rows.len(),
                    existing_rows.saturating_sub(start_row)
                ),
                0,
                0,
                start_row + 1,
                start_column + 1,
            ));
        }
        let mut assignments = Vec::with_capacity(rows.len() * width);
        for (offset, row) in rows.iter().enumerate() {
            for (column, cell) in row.cells.iter().enumerate() {
                let target_column = start_column + column;
                let value =
                    parse_pasted_value(&cell.text, &dataset.missing_tokens).map_err(|message| {
                        PasteFailure::new(
                            message,
                            row.source_row,
                            cell.location.column,
                            start_row + offset + 1,
                            target_column + 1,
                        )
                    })?;
                assignments.push((start_row + offset, target_column - 3, value));
            }
        }
        let grid = candidate
            .grids
            .get_mut(grid_index)
            .expect("checked grid index");
        for (row, field_index, value) in assignments {
            fields_mut(grid)
                .get_mut(field_index)
                .expect("checked field index")
                .values[row] = value;
        }
    } else {
        if start_row > existing_rows {
            return Err(PasteFailure::new(
                "Paste cannot leave a gap before appended irregular-grid rows.",
                0,
                0,
                start_row + 1,
                start_column + 1,
            ));
        }
        rows_appended = (start_row + rows.len()).saturating_sub(existing_rows);
        if rows_appended > 0 && (start_column != 0 || width < 3) {
            return Err(PasteFailure::new(
                "New irregular-grid rows require A, B, and C values. No cells were changed.",
                0,
                0,
                existing_rows + 1,
                start_column + 1,
            ));
        }
        let grid = candidate
            .grids
            .get_mut(grid_index)
            .expect("checked grid index");
        let TabulatedGrid::Irregular(grid) = grid else {
            unreachable!()
        };
        for _ in 0..rows_appended {
            grid.compositions.push([0.0; 3]);
            for field in &mut grid.fields {
                field.values.push(TabulatedValue::missing());
                field.row_lines.push(0);
            }
        }
        let mut compositions = Vec::new();
        let mut assignments = Vec::new();
        for (offset, row) in rows.iter().enumerate() {
            let target_row = start_row + offset;
            for (column, cell) in row.cells.iter().enumerate() {
                let target_column = start_column + column;
                if target_column < 3 {
                    let value = parse_pasted_composition(&cell.text).map_err(|message| {
                        PasteFailure::new(
                            message,
                            row.source_row,
                            cell.location.column,
                            target_row + 1,
                            target_column + 1,
                        )
                    })?;
                    compositions.push((target_row, target_column, value));
                } else {
                    let value = parse_pasted_value(&cell.text, &dataset.missing_tokens).map_err(
                        |message| {
                            PasteFailure::new(
                                message,
                                row.source_row,
                                cell.location.column,
                                target_row + 1,
                                target_column + 1,
                            )
                        },
                    )?;
                    assignments.push((target_row, target_column - 3, value));
                }
            }
        }
        for (row, component, value) in compositions {
            grid.compositions[row][component] = value;
        }
        if let Err((row, message)) = validate_pasted_irregular_compositions(&grid.compositions) {
            return Err(PasteFailure::new(message, row, 0, row, 1));
        }
        for (row, field_index, value) in assignments {
            grid.fields
                .get_mut(field_index)
                .ok_or_else(|| {
                    PasteFailure::new(
                        "destination field is unavailable",
                        0,
                        0,
                        row + 1,
                        field_index + 4,
                    )
                })?
                .values[row] = value;
        }
    }
    candidate
        .validate_document_structure()
        .map_err(|message| PasteFailure::new(message, 0, 0, 0, 0))?;
    Ok((
        candidate,
        rows.len(),
        width,
        rows_appended,
        header_skipped,
        grid.name().to_owned(),
    ))
}

unsafe fn input(value: *const c_char, label: &str) -> Result<String, String> {
    if value.is_null() {
        return Err(format!("{label} is required"));
    }
    // SAFETY: a non-null Qt UTF-8 string is NUL terminated.
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| format!("{label} must be valid UTF-8"))
}
unsafe fn out<'a, T>(value: *mut T, label: &str) -> Result<&'a mut T, String> {
    if value.is_null() {
        return Err(format!("{label} output is required"));
    }
    // SAFETY: the ABI requires caller-provided writable output storage.
    Ok(unsafe { &mut *value })
}
fn fields_mut(grid: &mut TabulatedGrid) -> &mut Vec<TabulatedField> {
    match grid {
        TabulatedGrid::Regular(grid) => &mut grid.fields,
        TabulatedGrid::Irregular(grid) => &mut grid.fields,
    }
}
fn add_field(
    fields: &mut Vec<TabulatedField>,
    row_count: usize,
    phase: &PhaseDefinition,
    property: &PropertyDefinition,
) {
    fields.push(TabulatedField {
        phase_id: phase.id,
        property: property.name.clone(),
        column_name: format!("{}.{}", phase.name, property.name),
        values: vec![TabulatedValue::missing(); row_count],
        row_lines: vec![0; row_count],
    });
}
fn initialise_fields(dataset: &TabulatedTernaryDataset, grid: &mut TabulatedGrid) {
    let row_count = grid.compositions().len();
    let fields = fields_mut(grid);
    for phase in &dataset.phases {
        for property in &dataset.properties {
            add_field(fields, row_count, phase, property);
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_new_document() -> TcqtStatus {
    status((|| {
        *document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())? = ProjectDocument::new();
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_open_document(path: *const c_char) -> TcqtStatus {
    status((|| {
        // SAFETY: Qt passes a NUL-terminated UTF-8 filesystem path.
        let path = PathBuf::from(unsafe { input(path, "document path") }?);
        let dataset = parse_path(&path).map_err(|error| error.to_string())?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .replace_loaded(dataset, path)
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_save_document(path: *const c_char) -> TcqtSaveResult {
    let path = match unsafe { input(path, "document path") } {
        Ok(path) => PathBuf::from(path),
        Err(error) => return save_result(3, error, ""),
    };
    let attempted = path.to_string_lossy().into_owned();
    let mut state = match document().lock() {
        Ok(state) => state,
        Err(_) => return save_result(3, "project lock is unavailable", &attempted),
    };
    if let Err(error) = state.dataset.validate_saveable_document() {
        return save_result(1, format!("Cannot save project: {error}"), &attempted);
    }
    let text = match serialize_tct(&state.dataset, &TctSerializeOptions::default()) {
        Ok(text) => text,
        Err(error) => {
            return save_result(
                2,
                format!("Could not serialize project: {error}"),
                &attempted,
            );
        }
    };
    if let Err(error) = save_tct_atomic(&path, &text) {
        return save_result(3, format!("Could not save project: {error}"), &attempted);
    }
    state.dataset.source_path = Some(path.clone());
    state.path = Some(path);
    state.saved_revision = state.revision;
    state.saved_dataset = state.dataset.clone();
    state.dirty = false;
    let (validity, _, _, _) = document_status(&state.dataset);
    let message = if validity == 1 {
        format!("Saved {} - draft document", attempted)
    } else {
        format!("Saved {}", attempted)
    };
    save_result(0, message, &attempted)
}
fn document_status(dataset: &TabulatedTernaryDataset) -> (u32, bool, bool, String) {
    match dataset.validate_saveable_document() {
        Err(error) => (0, false, false, error),
        Ok(()) => match dataset.validate_calculation_readiness() {
            Ok(()) => (2, true, true, String::new()),
            Err(error) => (1, true, false, error),
        },
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_project_summary(output_value: *mut TcqtProjectSummary) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable summary storage.
        let output_value = unsafe { out(output_value, "project summary") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let (validity, saveable, calculation_available, blocking_reason) =
            document_status(&state.dataset);
        *output_value = TcqtProjectSummary {
            title: bytes(state.dataset.title.as_deref().unwrap_or("Untitled")),
            path: bytes(
                state
                    .path
                    .as_ref()
                    .and_then(|path| path.to_str())
                    .unwrap_or(""),
            ),
            component_a: bytes(&state.dataset.components[0].name),
            component_b: bytes(&state.dataset.components[1].name),
            component_c: bytes(&state.dataset.components[2].name),
            phase_count: state.dataset.phases.len() as u32,
            property_count: state.dataset.properties.len() as u32,
            grid_count: state.dataset.grids.len() as u32,
            dirty: state.dirty,
            revision: state.revision,
            saved_revision: state.saved_revision,
            validity,
            saveable,
            calculation_available,
            blocking_reason: bytes(&blocking_reason),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_phase_at(index: u32, output_value: *mut TcqtPhase) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable phase storage.
        let output_value = unsafe { out(output_value, "phase") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let phase = state
            .dataset
            .phases
            .get(index as usize)
            .ok_or("phase index is out of range")?;
        *output_value = TcqtPhase {
            id: phase.id.0,
            name: bytes(&phase.name),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_property_at(
    index: u32,
    output_value: *mut TcqtProperty,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable property storage.
        let output_value = unsafe { out(output_value, "property") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let property = state
            .dataset
            .properties
            .get(index as usize)
            .ok_or("property index is out of range")?;
        *output_value = TcqtProperty {
            ordinal: index,
            required: property.required,
            name: bytes(&property.name),
            unit: bytes(&property.unit),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_grid_at(index: u32, output_value: *mut TcqtGrid) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable grid storage.
        let output_value = unsafe { out(output_value, "grid") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let grid = state
            .dataset
            .grids
            .get(index as usize)
            .ok_or("grid index is out of range")?;
        *output_value = TcqtGrid {
            index,
            kind: match grid.grid_type() {
                GridType::Regular => 0,
                GridType::Irregular => 1,
            },
            subdivisions: match grid {
                TabulatedGrid::Regular(grid) => grid.subdivisions as u32,
                TabulatedGrid::Irregular(_) => 0,
            },
            row_count: grid.compositions().len() as u32,
            field_count: grid.fields().len() as u32,
            name: bytes(grid.name()),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_grid_field_at(
    grid_index: u32,
    field_index: u32,
    output_value: *mut TcqtField,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable field storage.
        let output_value = unsafe { out(output_value, "field") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let grid = state
            .dataset
            .grids
            .get(grid_index as usize)
            .ok_or("grid index is out of range")?;
        let field = grid
            .fields()
            .get(field_index as usize)
            .ok_or("field index is out of range")?;
        *output_value = TcqtField {
            index: field_index,
            phase_id: field.phase_id.0,
            property: bytes(&field.property),
            column_name: bytes(&field.column_name),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_grid_row_at(
    grid_index: u32,
    row_index: u32,
    output_value: *mut TcqtRow,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable row storage.
        let output_value = unsafe { out(output_value, "row") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let row = state
            .dataset
            .grids
            .get(grid_index as usize)
            .ok_or("grid index is out of range")?
            .compositions()
            .get(row_index as usize)
            .ok_or("row index is out of range")?;
        *output_value = TcqtRow {
            a: row[0],
            b: row[1],
            c: row[2],
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_grid_cell_at(
    grid_index: u32,
    field_index: u32,
    row_index: u32,
    output_value: *mut TcqtCell,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable cell storage.
        let output_value = unsafe { out(output_value, "cell") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let value = state
            .dataset
            .grids
            .get(grid_index as usize)
            .ok_or("grid index is out of range")?
            .fields()
            .get(field_index as usize)
            .ok_or("field index is out of range")?
            .values
            .get(row_index as usize)
            .ok_or("row index is out of range")?;
        *output_value = TcqtCell {
            state: match value.state {
                TabulatedValueState::Calculated => 0,
                TabulatedValueState::NonExisting => 1,
                TabulatedValueState::CutOff => 2,
                TabulatedValueState::Missing => 3,
            },
            has_value: value.value.is_some(),
            value: value.value.unwrap_or_default(),
            note: bytes(value.note.as_deref().unwrap_or("")),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_paste_grid_tsv(
    grid_index: u32,
    start_row: u32,
    start_column: u32,
    clipboard: *const c_char,
) -> TcqtPasteResult {
    let clipboard = match unsafe { input(clipboard, "clipboard text") } {
        Ok(value) => value,
        Err(error) => return PasteFailure::new(error, 0, 0, 0, 0).result(),
    };
    let mut state = match document().lock() {
        Ok(state) => state,
        Err(_) => return PasteFailure::new("project lock is unavailable", 0, 0, 0, 0).result(),
    };
    let prior = state.dataset.clone();
    let (candidate, rows, columns, appended, header_skipped, grid_name) = match prepare_grid_paste(
        &state.dataset,
        grid_index as usize,
        start_row as usize,
        start_column as usize,
        &clipboard,
    ) {
        Ok(plan) => plan,
        Err(error) => return error.result(),
    };
    state.dataset = candidate;
    state.undo.push(prior);
    if state.undo.len() > 50 {
        state.undo.remove(0);
    }
    state.redo.clear();
    state.invalidate_calculation();
    state.mark_revision_changed();
    let _ = update(&mut state.contract, UiAction::DatasetEdited);
    paste_success(rows, columns, appended, header_skipped, &grid_name)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_title(value: *const c_char) -> TcqtStatus {
    status((|| {
        let value = unsafe { input(value, "title") }?;
        if value.trim().is_empty() {
            return Err("title cannot be empty".into());
        }
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                dataset.title = Some(value);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_component(index: u32, value: *const c_char) -> TcqtStatus {
    status((|| {
        let value = unsafe { input(value, "component name") }?;
        if value.trim().is_empty() {
            return Err("component names cannot be empty".into());
        }
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                dataset
                    .components
                    .get_mut(index as usize)
                    .ok_or("component index is out of range")?
                    .name = value;
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_add_phase(name: *const c_char) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "phase name") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if name.trim().is_empty() || dataset.phases.iter().any(|phase| phase.name == name) {
                    return Err("phase names must be non-empty and unique".into());
                }
                let mut id = 1;
                while dataset.phases.iter().any(|phase| phase.id.0 == id) {
                    id += 1;
                }
                let phase = PhaseDefinition {
                    name,
                    id: StablePhaseId(id),
                    line: 0,
                };
                for grid in &mut dataset.grids {
                    let count = grid.compositions().len();
                    let fields = fields_mut(grid);
                    for property in &dataset.properties {
                        add_field(fields, count, &phase, property);
                    }
                }
                dataset.phases.push(phase);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_remove_phase(id: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if !dataset.phases.iter().any(|phase| phase.id.0 == id) {
                    return Err("phase ID is out of range".into());
                }
                dataset.phases.retain(|phase| phase.id.0 != id);
                for grid in &mut dataset.grids {
                    fields_mut(grid).retain(|field| field.phase_id.0 != id);
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_add_property(
    name: *const c_char,
    unit: *const c_char,
    required: bool,
) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "property name") }?;
        let unit = unsafe { input(unit, "property unit") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if name.trim().is_empty()
                    || dataset
                        .properties
                        .iter()
                        .any(|property| property.name == name)
                {
                    return Err("property names must be non-empty and unique".into());
                }
                let property = PropertyDefinition {
                    name,
                    required,
                    unit,
                    line: 0,
                };
                for grid in &mut dataset.grids {
                    let count = grid.compositions().len();
                    let fields = fields_mut(grid);
                    for phase in &dataset.phases {
                        add_field(fields, count, phase, &property);
                    }
                }
                dataset.properties.push(property);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_remove_property(ordinal: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let property = dataset
                    .properties
                    .get(ordinal as usize)
                    .cloned()
                    .ok_or("property index is out of range")?;
                if property.name == "T" {
                    return Err("required property T cannot be removed".into());
                }
                dataset.properties.remove(ordinal as usize);
                for grid in &mut dataset.grids {
                    fields_mut(grid).retain(|field| field.property != property.name);
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_add_regular_grid(
    name: *const c_char,
    subdivisions: u32,
) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "grid name") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                validate_new_regular_grid_subdivisions(subdivisions as usize)?;
                if name.trim().is_empty() || dataset.grids.iter().any(|grid| grid.name() == name) {
                    return Err("grid names must be non-empty and unique".into());
                }
                let compositions = RegularTernaryGrid::new(subdivisions as usize)
                    .map_err(|error| error.to_string())?
                    .compositions()
                    .collect();
                let mut grid = TabulatedGrid::Regular(RegularTabulatedGrid {
                    name,
                    source: SourceRange {
                        first_line: 0,
                        last_line: 0,
                    },
                    subdivisions: subdivisions as usize,
                    order: RowOrder::Canonical,
                    composition_columns: CompositionColumns::None,
                    compositions,
                    fields: Vec::new(),
                });
                initialise_fields(dataset, &mut grid);
                dataset.grids.push(grid);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_add_irregular_grid(name: *const c_char) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "grid name") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if name.trim().is_empty() || dataset.grids.iter().any(|grid| grid.name() == name) {
                    return Err("grid names must be non-empty and unique".into());
                }
                let mut grid = TabulatedGrid::Irregular(IrregularTabulatedGrid {
                    name,
                    source: SourceRange {
                        first_line: 0,
                        last_line: 0,
                    },
                    compositions: Vec::new(),
                    fields: Vec::new(),
                });
                initialise_fields(dataset, &mut grid);
                dataset.grids.push(grid);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_remove_grid(index: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if index as usize >= dataset.grids.len() {
                    return Err("grid index is out of range".into());
                }
                dataset.grids.remove(index as usize);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_grid_cell(
    grid_index: u32,
    field_index: u32,
    row_index: u32,
    token: *const c_char,
) -> TcqtStatus {
    status((|| {
        let token = unsafe { input(token, "cell value") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let value = parse_tabulated_value_token(&token, &dataset.missing_tokens, true)?;
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let field = fields_mut(grid)
                    .get_mut(field_index as usize)
                    .ok_or("field index is out of range")?;
                *field
                    .values
                    .get_mut(row_index as usize)
                    .ok_or("row index is out of range")? = value;
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_add_irregular_row(grid_index: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let TabulatedGrid::Irregular(grid) = grid else {
                    return Err(
                        "regular-grid compositions are canonical and cannot be edited".into(),
                    );
                };
                grid.compositions.push([1.0, 0.0, 0.0]);
                for field in &mut grid.fields {
                    field.values.push(TabulatedValue::missing());
                    field.row_lines.push(0);
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_undo() -> TcqtStatus {
    status((|| {
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let prior = state.undo.pop().ok_or("nothing to undo")?;
        let current = std::mem::replace(&mut state.dataset, prior);
        state.redo.push(current);
        state.invalidate_calculation();
        state.mark_revision_changed();
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_redo() -> TcqtStatus {
    status((|| {
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let next = state.redo.pop().ok_or("nothing to redo")?;
        let current = std::mem::replace(&mut state.dataset, next);
        state.undo.push(current);
        state.invalidate_calculation();
        state.mark_revision_changed();
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_calculate_current() -> TcqtCalculationResult {
    let request = (|| {
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        state
            .dataset
            .validate_calculation_readiness()
            .map_err(|error| format!("Calculation unavailable: {error}"))?;
        let mut contract = state.contract.clone();
        let effects = update(&mut contract, UiAction::RecalculateRequested);
        let request_id = effects
            .iter()
            .find_map(|effect| match effect {
                UiEffect::RecalculateProjection { request, .. } => Some(request.0),
                _ => None,
            })
            .unwrap_or_default();
        Ok::<_, String>((state.dataset.clone(), state.revision, request_id))
    })();
    let (dataset, revision, request_id) = match request {
        Ok(request) => request,
        Err(error) => {
            return TcqtCalculationResult {
                success: false,
                request_id: 0,
                vertex_count: 0,
                message: bytes(&error),
            };
        }
    };
    let options =
        viewer_options(&default_viewer_options()).expect("default viewer options are valid");
    let projection = match calculate_projection(&dataset, &options) {
        Ok(projection) => projection,
        Err(error) => {
            return TcqtCalculationResult {
                success: false,
                request_id,
                vertex_count: 0,
                message: bytes(&format!("Calculation failed: {error}")),
            };
        }
    };
    let projection_records = projection_csv_records(
        &dataset,
        Some(&projection),
        None,
        &RenderOptions::default(),
        ProjectionCsvOptions::default(),
    )
    .unwrap_or_default();
    let summary = format!(
        "Calculated {} phases, {} contour paths",
        projection.input_summary.phase_count, projection.diagnostics.contour_path_count
    );
    let vertex_count = dataset
        .grids
        .iter()
        .map(|grid| grid.compositions().len())
        .sum::<usize>() as u32;
    let saved = (|| {
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        if state.revision != revision {
            return Err("project changed while the calculation was running".to_owned());
        }
        state.projection = Some(projection);
        state.projection_records = projection_records;
        Ok::<_, String>(())
    })();
    match saved {
        Ok(()) => TcqtCalculationResult {
            success: true,
            request_id,
            vertex_count,
            message: bytes(&summary),
        },
        Err(error) => TcqtCalculationResult {
            success: false,
            request_id,
            vertex_count: 0,
            message: bytes(&error),
        },
    }
}

/// Runs the exact existing CLI projection pipeline with Viewer-owned options.
/// The calculation is accepted only when both the document revision and caller
/// request generation still match, so a stale worker cannot replace a newer
/// canvas.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_calculate_viewer(
    raw_options: *const TcqtViewerCalculationOptions,
    expected_revision: u64,
    request_id: u64,
) -> TcqtCalculationResult {
    let raw_options = match unsafe { out(raw_options.cast_mut(), "viewer calculation options") } {
        Ok(value) => *value,
        Err(error) => {
            return TcqtCalculationResult {
                success: false,
                request_id,
                vertex_count: 0,
                message: bytes(&error),
            };
        }
    };
    let options = match viewer_options(&raw_options) {
        Ok(options) => options,
        Err(error) => {
            return TcqtCalculationResult {
                success: false,
                request_id,
                vertex_count: 0,
                message: bytes(&error),
            };
        }
    };
    let dataset = match document().lock() {
        Ok(mut state) => {
            if state.revision != expected_revision {
                return TcqtCalculationResult {
                    success: false,
                    request_id,
                    vertex_count: 0,
                    message: bytes("project changed before calculation started"),
                };
            }
            if let Err(error) = state.dataset.validate_calculation_readiness() {
                return TcqtCalculationResult {
                    success: false,
                    request_id,
                    vertex_count: 0,
                    message: bytes(&format!("Calculation unavailable: {error}")),
                };
            }
            state.calculation_generation = request_id;
            state.dataset.clone()
        }
        Err(_) => {
            return TcqtCalculationResult {
                success: false,
                request_id,
                vertex_count: 0,
                message: bytes("project lock is unavailable"),
            };
        }
    };
    let projection = match calculate_projection(&dataset, &options) {
        Ok(projection) => projection,
        Err(error) => {
            return TcqtCalculationResult {
                success: false,
                request_id,
                vertex_count: 0,
                message: bytes(&format!("Calculation failed: {error}")),
            };
        }
    };
    let records = projection_csv_records(
        &dataset,
        Some(&projection),
        None,
        &RenderOptions::default(),
        ProjectionCsvOptions::default(),
    )
    .unwrap_or_default();
    let vertex_count = dataset
        .grids
        .iter()
        .map(|grid| grid.compositions().len())
        .sum::<usize>() as u32;
    let summary = format!(
        "Calculated {} invariants, {} univariant paths, and {} isotherm paths",
        projection.diagnostics.invariant_count,
        projection.diagnostics.univariant_count,
        projection.diagnostics.contour_path_count,
    );
    match document().lock() {
        Ok(mut state)
            if state.revision == expected_revision
                && state.calculation_generation == request_id =>
        {
            state.projection = Some(projection);
            state.projection_records = records;
            TcqtCalculationResult {
                success: true,
                request_id,
                vertex_count,
                message: bytes(&summary),
            }
        }
        Ok(_) => TcqtCalculationResult {
            success: false,
            request_id,
            vertex_count: 0,
            message: bytes("calculation result became stale and was discarded"),
        },
        Err(_) => TcqtCalculationResult {
            success: false,
            request_id,
            vertex_count: 0,
            message: bytes("project lock is unavailable"),
        },
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_projection_summary(
    output_value: *mut TcqtProjectionSummary,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "projection summary") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let Some(projection) = state.projection.as_ref() else {
            *output_value = TcqtProjectionSummary {
                available: false,
                source_minimum: 0.0,
                source_maximum: 0.0,
                automatic_minimum: 0.0,
                automatic_used_invariant: false,
                level_count: 0,
                invariant_count: 0,
                univariant_count: 0,
                contour_path_count: 0,
                message: bytes("No projection has been calculated."),
            };
            return Ok(());
        };
        let automatic = projection.automatic_iso_range;
        *output_value = TcqtProjectionSummary {
            available: true,
            source_minimum: projection.input_summary.temperature_range[0],
            source_maximum: projection.input_summary.temperature_range[1],
            automatic_minimum: automatic
                .map_or(projection.input_summary.temperature_range[0], |range| {
                    range.minimum
                }),
            automatic_used_invariant: automatic.is_some_and(|range| range.used_invariant_minimum),
            level_count: projection.levels.len() as u32,
            invariant_count: projection.diagnostics.invariant_count as u32,
            univariant_count: projection.diagnostics.univariant_count as u32,
            contour_path_count: projection.diagnostics.contour_path_count as u32,
            message: bytes("Projection is current."),
        };
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_evaluate_field(
    grid_index: u32,
    phase_id: u32,
    property: *const c_char,
    raw_options: *const TcqtViewerCalculationOptions,
    a: f64,
    b: f64,
    c: f64,
    query_index: u64,
    output_value: *mut TcqtInspectionResult,
) -> TcqtStatus {
    status((|| {
        let property = unsafe { input(property, "field property") }?;
        let raw_options = *unsafe { out(raw_options.cast_mut(), "viewer calculation options") }?;
        let options = viewer_options(&raw_options)?;
        if [a, b, c]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || (a + b + c - 1.0).abs() > 1.0e-8
        {
            return Err("query composition must be finite, non-negative, and sum to one".into());
        }
        let output_value = unsafe { out(output_value, "inspection result") }?;
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let dataset = state.dataset.clone();
        let identity = InspectionFieldIdentity {
            grid_index: grid_index as usize,
            phase_id: StablePhaseId(phase_id),
            property,
        };
        let result = state.inspection_cache.evaluate(
            &dataset,
            &identity,
            &options,
            [a, b, c],
            query_index as usize,
            query_index,
        );
        let lambdas = result.local_barycentric.unwrap_or([0.0; 3]);
        let rows = result.triangle_vertex_indices.unwrap_or([0; 3]);
        *output_value = TcqtInspectionResult {
            success: !matches!(result.state, InterpolatedResultState::Error(_)),
            state: inspection_state(&result.state),
            has_value: result.value.is_some(),
            value: result.value.unwrap_or(0.0),
            a,
            b,
            c,
            triangle_index: result.triangle_index.map_or(u32::MAX, |index| index as u32),
            has_local_barycentric: result.local_barycentric.is_some(),
            lambda0: lambdas[0],
            lambda1: lambdas[1],
            lambda2: lambdas[2],
            has_contributions: result.linear_part.is_some() && result.excess_part.is_some(),
            linear_part: result.linear_part.unwrap_or(0.0),
            excess_part: result.excess_part.unwrap_or(0.0),
            has_source_rows: result.triangle_vertex_indices.is_some(),
            source_row0: rows[0] as u32,
            source_row1: rows[1] as u32,
            source_row2: rows[2] as u32,
            local_mode: local_mode(result.local_mode),
            unit: bytes(&result.unit),
            message: bytes(match &result.state {
                InterpolatedResultState::Error(error) => error,
                state => state.label(),
            }),
        };
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_field_vertex(
    grid_index: u32,
    phase_id: u32,
    property: *const c_char,
    row_index: u32,
    token: *const c_char,
) -> TcqtStatus {
    status((|| {
        let property = unsafe { input(property, "field property") }?;
        let token = unsafe { input(token, "vertex value") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let value = parse_tabulated_value_token(&token, &dataset.missing_tokens, false)?;
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let field = fields_mut(grid)
                    .iter_mut()
                    .find(|field| field.phase_id.0 == phase_id && field.property == property)
                    .ok_or("selected phase/property field no longer exists")?;
                *field
                    .values
                    .get_mut(row_index as usize)
                    .ok_or("vertex row is out of range")? = value;
                Ok(())
            })
    })())
}

fn state_value(code: u32) -> Result<TabulatedValueState, String> {
    match code {
        1 => Ok(TabulatedValueState::NonExisting),
        2 => Ok(TabulatedValueState::CutOff),
        3 => Ok(TabulatedValueState::Missing),
        _ => Err("bulk edits may set only Missing, Non-existing, or Cut-off".into()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_bulk_set_field_state(
    grid_index: u32,
    phase_id: u32,
    property: *const c_char,
    rows: *const u32,
    row_count: u32,
    state_code: u32,
) -> TcqtStatus {
    status((|| {
        let property = unsafe { input(property, "field property") }?;
        let state_value = state_value(state_code)?;
        if rows.is_null() && row_count != 0 {
            return Err("selected vertex rows are unavailable".into());
        }
        // SAFETY: the C++ caller supplies row_count contiguous u32 entries.
        let rows = unsafe { std::slice::from_raw_parts(rows, row_count as usize) };
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let field = fields_mut(grid)
                    .iter_mut()
                    .find(|field| field.phase_id.0 == phase_id && field.property == property)
                    .ok_or("selected phase/property field no longer exists")?;
                if rows.iter().any(|row| *row as usize >= field.values.len()) {
                    return Err("a selected vertex row is out of range".into());
                }
                for row in rows {
                    field.values[*row as usize] = TabulatedValue {
                        state: state_value,
                        value: None,
                        note: None,
                    };
                }
                Ok(())
            })
    })())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_clear_field_notes(
    grid_index: u32,
    phase_id: u32,
    property: *const c_char,
    rows: *const u32,
    row_count: u32,
) -> TcqtStatus {
    status((|| {
        let property = unsafe { input(property, "field property") }?;
        if rows.is_null() && row_count != 0 {
            return Err("selected vertex rows are unavailable".into());
        }
        // SAFETY: the C++ caller supplies row_count contiguous u32 entries.
        let rows = unsafe { std::slice::from_raw_parts(rows, row_count as usize) };
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let field = fields_mut(grid)
                    .iter_mut()
                    .find(|field| field.phase_id.0 == phase_id && field.property == property)
                    .ok_or("selected phase/property field no longer exists")?;
                if rows.iter().any(|row| *row as usize >= field.values.len()) {
                    return Err("a selected vertex row is out of range".into());
                }
                for row in rows {
                    field.values[*row as usize].note = None;
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_projection_record_count(output_value: *mut u32) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "projection record count") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        *output_value = state.projection_records.len() as u32;
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_projection_record_at(
    index: u32,
    output_value: *mut TcqtProjectionRecord,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "projection record") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let record = state
            .projection_records
            .get(index as usize)
            .ok_or("projection record index is out of range")?;
        *output_value = TcqtProjectionRecord {
            a: record.composition[0],
            b: record.composition[1],
            c: record.composition[2],
            point_index: record.point_index as u32,
            line_type: match record.line_type {
                ternary_contours_cli::ProjectionLineType::StableIsotherm => 0,
                ternary_contours_cli::ProjectionLineType::StableUnivariant => 1,
                ternary_contours_cli::ProjectionLineType::BinaryInvariant => 2,
                ternary_contours_cli::ProjectionLineType::InteriorInvariant => 3,
                ternary_contours_cli::ProjectionLineType::StableBoundaryContact => 4,
            },
            line_id: bytes(&record.line_id),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_export_plot(path: *const c_char, format: u32) -> TcqtStatus {
    status((|| {
        let path = PathBuf::from(unsafe { input(path, "export path") }?);
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let projection = state
            .projection
            .as_ref()
            .ok_or("calculate a projection before exporting it")?;
        let format = match format {
            0 => OutputFormat::Png,
            1 => OutputFormat::Svg,
            _ => return Err("unsupported plot export format".into()),
        };
        render_to_path(
            &path,
            &state.dataset,
            projection,
            &RenderOptions::default(),
            Some(format),
        )
        .map_err(|error| error.to_string())
    })())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_export_lines_csv(path: *const c_char) -> TcqtStatus {
    status((|| {
        let path = PathBuf::from(unsafe { input(path, "export path") }?);
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let records = projection_csv_records(
            &state.dataset,
            state.projection.as_ref(),
            None,
            &RenderOptions::default(),
            ProjectionCsvOptions::default(),
        )
        .map_err(|error| error.to_string())?;
        let contents = serialize_projection_csv(&records).map_err(|error| error.to_string())?;
        std::fs::write(path, contents).map_err(|error| error.to_string())
    })())
}
/// Original feasibility entrypoint retained for existing callers. The application
/// now uses `tcqt_calculate_current` against the active dataset.
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_run_feasibility_calculation(
    subdivisions: u32,
    dataset_revision: u64,
) -> TcqtCalculationResult {
    let mut state = GuiContractState::default();
    state.revisions.dataset = Revision(dataset_revision);
    let request_id = update(&mut state, UiAction::RecalculateRequested)
        .iter()
        .find_map(|effect| match effect {
            UiEffect::RecalculateProjection { request, .. } => Some(request.0),
            _ => None,
        })
        .unwrap_or_default();
    match RegularTernaryGrid::new(subdivisions as usize) {
        Ok(grid) => TcqtCalculationResult {
            success: true,
            request_id,
            vertex_count: grid.vertex_count() as u32,
            message: bytes(&format!(
                "Rust grid ready: {} canonical vertices",
                grid.vertex_count()
            )),
        },
        Err(error) => TcqtCalculationResult {
            success: false,
            request_id,
            vertex_count: 0,
            message: bytes(&format!("Rust grid rejected: {error}")),
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn tcqt_set_irregular_composition(
    grid_index: u32,
    row_index: u32,
    a: f64,
    b: f64,
    c: f64,
) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if ![a, b, c].into_iter().all(f64::is_finite)
                    || [a, b, c].into_iter().any(|value| value < 0.0)
                    || (a + b + c - 1.0).abs() > 1e-8
                {
                    return Err(
                        "irregular compositions must be finite, non-negative, and sum to one"
                            .into(),
                    );
                }
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let TabulatedGrid::Irregular(grid) = grid else {
                    return Err(
                        "regular-grid compositions are canonical and cannot be edited".into(),
                    );
                };
                if row_index as usize >= grid.compositions.len() {
                    return Err("row index is out of range".into());
                }
                if grid.compositions.iter().enumerate().any(|(index, point)| {
                    index != row_index as usize
                        && point
                            .iter()
                            .zip([a, b, c])
                            .map(|(left, right)| (left - right).abs())
                            .fold(0.0, f64::max)
                            <= 1e-10
                }) {
                    return Err("irregular grid has a duplicate composition".into());
                }
                grid.compositions[row_index as usize] = [a, b, c];
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_rename_grid(index: u32, name: *const c_char) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "grid name") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if name.trim().is_empty()
                    || dataset
                        .grids
                        .iter()
                        .enumerate()
                        .any(|(other, grid)| other != index as usize && grid.name() == name)
                {
                    return Err("grid names must be non-empty and unique".into());
                }
                match dataset
                    .grids
                    .get_mut(index as usize)
                    .ok_or("grid index is out of range")?
                {
                    TabulatedGrid::Regular(grid) => grid.name = name,
                    TabulatedGrid::Irregular(grid) => grid.name = name,
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_duplicate_grid(index: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let mut duplicate = dataset
                    .grids
                    .get(index as usize)
                    .cloned()
                    .ok_or("grid index is out of range")?;
                let base = duplicate.name().to_owned();
                let mut number = 2;
                let mut name = format!("{base} {number}");
                while dataset.grids.iter().any(|grid| grid.name() == name) {
                    number += 1;
                    name = format!("{base} {number}");
                }
                match &mut duplicate {
                    TabulatedGrid::Regular(grid) => grid.name = name,
                    TabulatedGrid::Irregular(grid) => grid.name = name,
                }
                dataset.grids.push(duplicate);
                Ok(())
            })
    })())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_has_empty_phase_and_grid_collections() {
        let state = ProjectDocument::new();
        assert_eq!(
            state.dataset.components.map(|component| component.name),
            ["A", "B", "C"]
        );
        assert!(state.dataset.phases.is_empty());
        assert!(state.dataset.grids.is_empty());
        assert_eq!(state.dataset.properties.len(), 1);
        assert_eq!(state.dataset.properties[0].name, "T");
        assert!(state.dataset.properties[0].required);
        assert_eq!(state.dataset.properties[0].unit, "C");
    }

    #[test]
    fn draft_save_and_reopen_round_trips_transactionally() {
        let path =
            std::env::temp_dir().join(format!("ternary-contours-draft-{}.tct", std::process::id()));
        let encoded = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
        unsafe {
            assert!(tcqt_new_document().success);
            let saved = tcqt_save_document(encoded.as_ptr());
            assert_eq!(saved.outcome, 0);
            assert!(path.exists());
            let mut summary = std::mem::zeroed();
            assert!(tcqt_project_summary(&mut summary).success);
            assert_eq!(summary.phase_count, 0);
            assert_eq!(summary.grid_count, 0);
            assert!(!summary.dirty);
            assert!(tcqt_open_document(encoded.as_ptr()).success);
            let mut reopened = std::mem::zeroed();
            assert!(tcqt_project_summary(&mut reopened).success);
            assert_eq!(reopened.phase_count, 0);
            assert_eq!(reopened.grid_count, 0);
            assert!(!reopened.dirty);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bulk_paste_handles_headers_states_and_is_transactional() {
        let dataset = ternary_contours_cli::default_regular_dataset();
        let pasted = prepare_grid_paste(
            &dataset,
            0,
            0,
            3,
            "Phase1.T\tPhase2.T\r\n1200\t1180\r\n1210\tNE\r\n",
        )
        .unwrap();
        assert_eq!(pasted.1, 2);
        assert_eq!(pasted.2, 2);
        assert!(pasted.4);
        let fields = pasted.0.grids[0].fields();
        assert_eq!(fields[0].values[0].state, TabulatedValueState::Calculated);
        assert_eq!(fields[1].values[1].state, TabulatedValueState::NonExisting);

        let failure = prepare_grid_paste(&dataset, 0, 0, 3, "1200\tNO").unwrap_err();
        assert!(failure.message.contains("finite number, NA, NE, or CO"));
        assert_eq!(
            dataset.grids[0].fields()[0].values[0].state,
            TabulatedValueState::Missing
        );
        assert!(
            prepare_grid_paste(&dataset, 0, 0, 3, "1200\n1210\t1220")
                .unwrap_err()
                .message
                .contains("wrong row width")
        );
    }

    #[test]
    fn bulk_paste_rejects_regular_compositions_and_appends_irregular_rows() {
        let dataset = ternary_contours_cli::default_regular_dataset();
        let regular_error = prepare_grid_paste(&dataset, 0, 0, 0, "A\tB\n0\t1").unwrap_err();
        assert!(regular_error.message.contains("composition columns"));

        let mut irregular = empty_project_dataset();
        irregular.phases.push(PhaseDefinition {
            name: "Phase1".into(),
            id: StablePhaseId(1),
            line: 0,
        });
        irregular
            .grids
            .push(TabulatedGrid::Irregular(IrregularTabulatedGrid {
                name: "irregular".into(),
                source: SourceRange {
                    first_line: 0,
                    last_line: 0,
                },
                compositions: vec![[1.0, 0.0, 0.0]],
                fields: vec![TabulatedField {
                    phase_id: StablePhaseId(1),
                    property: "T".into(),
                    column_name: "Phase1.T".into(),
                    values: vec![TabulatedValue::missing()],
                    row_lines: vec![0],
                }],
            }));
        let appended = prepare_grid_paste(
            &irregular,
            0,
            1,
            0,
            "A\tB\tC\tPhase1.T\n0\t0.5\t0.5\t1250\n0.2\t0.3\t0.5\tCO:3000",
        )
        .unwrap();
        assert_eq!(appended.3, 2);
        assert_eq!(appended.0.grids[0].compositions().len(), 3);
        assert_eq!(
            appended.0.grids[0].fields()[0].values[2].state,
            TabulatedValueState::CutOff
        );

        let missing_compositions = prepare_grid_paste(&irregular, 0, 1, 3, "1250").unwrap_err();
        assert!(missing_compositions.message.contains("A, B, and C"));
    }

    #[test]
    fn starter_cells_are_typed_missing() {
        let state = ProjectDocument {
            dataset: ternary_contours_cli::default_regular_dataset(),
            ..ProjectDocument::new()
        };
        assert_eq!(state.dataset.grids[0].compositions().len(), 66);
        assert!(
            state.dataset.grids[0]
                .fields()
                .iter()
                .flat_map(|field| &field.values)
                .all(|value| value.state == TabulatedValueState::Missing && value.value.is_none())
        );
    }

    #[test]
    fn viewer_vertex_mutation_is_one_revision_one_undo_and_invalidates_projection() {
        let mut state = ProjectDocument {
            dataset: ternary_contours_cli::default_regular_dataset(),
            ..ProjectDocument::new()
        };
        state.projection_records.push(ProjectionCsvRecord {
            line_id: "previous".into(),
            point_index: 0,
            composition: [1.0, 0.0, 0.0],
            temperature: None,
            line_type: ternary_contours_cli::ProjectionLineType::StableIsotherm,
            phase: None,
            phase_1: None,
            phase_2: None,
            level: None,
            path_source: ternary_contours_cli::ProjectionPathSource::Raw,
            closed: false,
        });
        let revision = state.revision;
        state
            .mutate(|dataset| {
                let field = &mut fields_mut(&mut dataset.grids[0])[0];
                field.values[0] =
                    TabulatedValue::calculated(1_250.0).map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert_eq!(state.revision, revision + 1);
        assert_eq!(state.undo.len(), 1);
        assert!(state.dirty);
        assert!(state.projection_records.is_empty());
        assert_eq!(
            state.dataset.grids[0].fields()[0].values[0].value,
            Some(1_250.0)
        );
    }
    #[test]
    fn a_regular_grid_has_canonical_rows_and_fields() {
        let state = ProjectDocument {
            dataset: ternary_contours_cli::default_regular_dataset(),
            ..ProjectDocument::new()
        };
        let mut grid = TabulatedGrid::Regular(RegularTabulatedGrid {
            name: "next".into(),
            source: SourceRange {
                first_line: 0,
                last_line: 0,
            },
            subdivisions: 4,
            order: RowOrder::Canonical,
            composition_columns: CompositionColumns::None,
            compositions: RegularTernaryGrid::new(4).unwrap().compositions().collect(),
            fields: Vec::new(),
        });
        initialise_fields(&state.dataset, &mut grid);
        assert_eq!(grid.compositions().len(), 15);
        assert_eq!(grid.fields().len(), 3);
    }
}
