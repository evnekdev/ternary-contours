//! Rust-owned document state for the Qt Widgets application.
//!
//! Qt receives copies of typed snapshots and submits small mutations. TCT
//! parsing, serialization, validation, classified values, and calculations stay
//! in Rust; no second document parser or NaN-based missing representation exists
//! in the C++ layer.

use std::{
    ffi::CStr,
    os::raw::c_char,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use ternary_contours::{RegularTernaryGrid, StablePhaseId};
use ternary_contours_cli::{
    CompositionColumns, GridType, IrregularTabulatedGrid, LiquidusProjection, OutputFormat,
    PhaseDefinition, ProjectionCsvOptions, ProjectionCsvRecord, ProjectionOptions,
    PropertyDefinition, RegularTabulatedGrid, RenderOptions, RowOrder, SourceRange, TabulatedField,
    TabulatedGrid, TabulatedTernaryDataset, TabulatedValue, TabulatedValueState,
    TctSerializeOptions, calculate_projection, empty_project_dataset, parse_path,
    parse_tabulated_value_token, projection_csv_records, render_to_path, save_tct_atomic,
    serialize_projection_csv, serialize_tct, validate_new_regular_grid_subdivisions,
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

#[derive(Clone)]
struct ProjectDocument {
    dataset: TabulatedTernaryDataset,
    path: Option<PathBuf>,
    dirty: bool,
    revision: u64,
    undo: Vec<TabulatedTernaryDataset>,
    redo: Vec<TabulatedTernaryDataset>,
    contract: GuiContractState,
    projection: Option<LiquidusProjection>,
    projection_records: Vec<ProjectionCsvRecord>,
}
impl ProjectDocument {
    fn new() -> Self {
        Self {
            dataset: empty_project_dataset(),
            path: None,
            dirty: false,
            revision: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            contract: GuiContractState::default(),
            projection: None,
            projection_records: Vec::new(),
        }
    }
    fn mutate(
        &mut self,
        edit: impl FnOnce(&mut TabulatedTernaryDataset) -> Result<(), String>,
    ) -> Result<(), String> {
        let prior = self.dataset.clone();
        edit(&mut self.dataset)?;
        // A new document is intentionally allowed to be incomplete while the
        // user adds its first phase and grid. Once both collections exist, use
        // the normal dataset validator so loaded/populated projects retain all
        // declaration and field invariants.
        if !self.dataset.phases.is_empty()
            && !self.dataset.grids.is_empty()
            && let Err(error) = self.dataset.validate_structure()
        {
            self.dataset = prior;
            return Err(error);
        }
        self.undo.push(prior);
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.revision = self.revision.saturating_add(1);
        self.contract.revisions.dataset = Revision(self.revision);
        let _ = update(&mut self.contract, UiAction::DatasetEdited);
        Ok(())
    }
    fn replace_loaded(
        &mut self,
        mut dataset: TabulatedTernaryDataset,
        path: PathBuf,
    ) -> Result<(), String> {
        dataset.validate_structure()?;
        dataset.source_path = Some(path.clone());
        self.dataset = dataset;
        self.path = Some(path);
        self.dirty = false;
        self.revision = self.revision.saturating_add(1);
        self.undo.clear();
        self.redo.clear();
        self.contract = GuiContractState::default();
        self.projection = None;
        self.projection_records.clear();
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
pub unsafe extern "C" fn tcqt_save_document(path: *const c_char) -> TcqtStatus {
    status((|| {
        // SAFETY: Qt passes a NUL-terminated UTF-8 filesystem path.
        let path = PathBuf::from(unsafe { input(path, "document path") }?);
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let text = serialize_tct(&state.dataset, &TctSerializeOptions::default())
            .map_err(|error| error.to_string())?;
        save_tct_atomic(&path, &text).map_err(|error| error.to_string())?;
        state.dataset.source_path = Some(path.clone());
        state.path = Some(path);
        state.dirty = false;
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_project_summary(output_value: *mut TcqtProjectSummary) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable summary storage.
        let output_value = unsafe { out(output_value, "project summary") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
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
        state.dirty = true;
        state.revision = state.revision.saturating_add(1);
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
        state.dirty = true;
        state.revision = state.revision.saturating_add(1);
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_calculate_current() -> TcqtCalculationResult {
    let request = (|| {
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
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
    let projection = match calculate_projection(&dataset, &ProjectionOptions::default()) {
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
                            .into_iter()
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
