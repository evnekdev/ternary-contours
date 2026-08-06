//! Ternary grid inspection and classified point editing controls.

use std::collections::BTreeSet;

use eframe::egui;
use ternary_contours::StablePhaseId;

use crate::{
    DatasetEditorState, NumericFormat, PLOT_BACKGROUND_RGB, ProjectionOptions, TabulatedGrid,
    TabulatedValue, TabulatedValueState, TernaryRenderTransform,
    interpolation_inspection::{
        FieldInspectionCache, InspectionFieldIdentity, InterpolatedResult, InterpolatedResultState,
    },
};

use super::{controls, hit_test::ViewerTransform};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridInspectionAction {
    None,
    /// A point or field changed only in the draft dataset.
    DraftEdited,
    /// The draft was committed to the active dataset.
    Applied,
    Recalculate,
    /// A local inspection setting changed; no document or liquidus recalculation is needed.
    InspectionChanged,
}
/// The two focused workflows available in Grid inspection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GridInspectionMode {
    #[default]
    VertexSelection,
    Interpolation,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GridLabelMode {
    #[default]
    None,
    Value,
    State,
    RowIndex,
    ValueAndState,
}

pub struct GridInspectionUi {
    pub selected_grid: usize,
    pub selected_phase: Option<StablePhaseId>,
    pub selected_property: String,
    pub selected_rows: BTreeSet<usize>,
    pub label_mode: GridLabelMode,
    pub label_precision: usize,
    pub labels_selected_only: bool,
    pub marker_size: f32,
    pub show_regular_edges: bool,
    pub show_calculated: bool,
    pub show_non_existing: bool,
    pub show_cut_off: bool,
    pub show_missing: bool,
    pub mode: GridInspectionMode,
    pub show_containing_triangle: bool,
    pub results: Vec<InterpolatedResult>,
    pub selected_result: Option<usize>,
    next_query_id: u64,
    pub show_local_barycentric: bool,
    pub show_contributions: bool,
    pub show_triangle_index: bool,
    pub show_triangle_vertices: bool,
    results_table_has_focus: bool,
    inspection_cache: FieldInspectionCache,
    edit_state: TabulatedValueState,
    edit_value: String,
    edit_note: String,
    edit_row: Option<usize>,
    pub message: Option<String>,
}

impl Default for GridInspectionUi {
    fn default() -> Self {
        Self {
            selected_grid: 0,
            selected_phase: None,
            selected_property: String::new(),
            selected_rows: BTreeSet::new(),
            label_mode: GridLabelMode::None,
            label_precision: 3,
            labels_selected_only: true,
            marker_size: 6.0,
            show_regular_edges: false,
            show_calculated: true,
            show_non_existing: true,
            show_cut_off: true,
            show_missing: true,
            mode: GridInspectionMode::VertexSelection,
            show_containing_triangle: true,
            results: Vec::new(),
            selected_result: None,
            next_query_id: 1,
            show_local_barycentric: false,
            show_contributions: false,
            show_triangle_index: false,
            show_triangle_vertices: false,
            results_table_has_focus: false,
            inspection_cache: FieldInspectionCache::default(),
            edit_state: TabulatedValueState::Missing,
            edit_value: String::new(),
            edit_note: String::new(),
            edit_row: None,
            message: None,
        }
    }
}

impl GridInspectionUi {
    /// Select the first grid field, preferring `T`, after a document opens.
    pub fn initialise(&mut self, editor: &DatasetEditorState) {
        self.selected_rows.clear();
        self.edit_row = None;
        self.message = None;
        self.reset_interpolation_state();
        let selected = editor
            .draft
            .grids
            .iter()
            .enumerate()
            .find_map(|(grid_index, grid)| {
                grid.fields()
                    .iter()
                    .position(|field| field.property == "T")
                    .or_else(|| (!grid.fields().is_empty()).then_some(0))
                    .map(|field_index| (grid_index, field_index))
            });
        if let Some((grid_index, field_index)) = selected {
            self.selected_grid = grid_index;
            let field = &editor.draft.grids[grid_index].fields()[field_index];
            self.selected_phase = Some(field.phase_id);
            self.selected_property = field.property.clone();
        } else {
            self.selected_grid = 0;
            self.selected_phase = None;
            self.selected_property.clear();
            self.message = Some(
                "The dataset was loaded successfully, but it contains no scalar fields available for Grid inspection.".into(),
            );
        }
    }

    fn ensure_selection(&mut self, editor: &DatasetEditorState) {
        if editor.draft.grids.is_empty() {
            self.initialise(editor);
            return;
        }
        self.selected_grid = self.selected_grid.min(editor.draft.grids.len() - 1);
        if self.selected_field_index(editor).is_none() {
            self.initialise(editor);
        }
    }

    pub fn selected_field_index(&self, editor: &DatasetEditorState) -> Option<usize> {
        let grid = editor.draft.grids.get(self.selected_grid)?;
        let phase = self.selected_phase?;
        grid.fields()
            .iter()
            .position(|field| field.phase_id == phase && field.property == self.selected_property)
    }

    fn select_field(&mut self, editor: &DatasetEditorState, field_index: usize) {
        let Some(field) = editor
            .draft
            .grids
            .get(self.selected_grid)
            .and_then(|grid| grid.fields().get(field_index))
        else {
            return;
        };
        self.selected_phase = Some(field.phase_id);
        self.selected_property = field.property.clone();
        self.selected_rows.clear();
        self.edit_row = None;
    }

    fn sync_point_editor(&mut self, editor: &DatasetEditorState) {
        let Some(row) = self.selected_rows.iter().next().copied() else {
            self.edit_row = None;
            return;
        };
        if self.edit_row == Some(row) {
            return;
        }
        let Some(value) = self.selected_field_index(editor).and_then(|field| {
            editor.draft.grids[self.selected_grid].fields()[field]
                .values
                .get(row)
        }) else {
            self.edit_row = None;
            return;
        };
        self.edit_row = Some(row);
        self.edit_state = value.state;
        self.edit_value = value
            .defined_value()
            .map(|number| number.to_string())
            .unwrap_or_default();
        self.edit_note = value.note.clone().unwrap_or_default();
    }
    fn field_identity(&self) -> Option<InspectionFieldIdentity> {
        Some(InspectionFieldIdentity {
            grid_index: self.selected_grid,
            phase_id: self.selected_phase?,
            property: self.selected_property.clone(),
        })
    }

    fn register_interpolation_query(
        &mut self,
        editor: &DatasetEditorState,
        options: &ProjectionOptions,
        composition: [f64; 3],
    ) {
        let Some(identity) = self.field_identity() else {
            return;
        };
        let index = self
            .results
            .last()
            .map_or(1, |result| result.index.saturating_add(1));
        let id = self.next_query_id;
        self.next_query_id = self.next_query_id.saturating_add(1);
        let result = self.inspection_cache.evaluate(
            &editor.draft,
            &identity,
            options,
            composition,
            index,
            id,
        );
        self.results.push(result);
        self.selected_result = Some(self.results.len() - 1);
    }

    pub(crate) fn recalculate_interpolation_results(
        &mut self,
        editor: &DatasetEditorState,
        options: &ProjectionOptions,
    ) {
        let Some(identity) = self.field_identity() else {
            return;
        };
        let Some(first) = self
            .results
            .first()
            .map(|result| (result.composition, result.index, result.id))
        else {
            return;
        };
        let first_updated = self.inspection_cache.evaluate(
            &editor.draft,
            &identity,
            options,
            first.0,
            first.1,
            first.2,
        );
        if let Some(error) = self.inspection_cache.preparation_error() {
            for result in &mut self.results {
                result.stale = true;
                result.stale_error = Some(error.to_owned());
            }
            self.message = Some(format!(
                "Interpolator preparation failed. {} previous result(s) are still displayed and marked stale: {error}",
                self.results.len()
            ));
            return;
        }
        self.results[0] = first_updated;
        for result in self.results.iter_mut().skip(1) {
            let composition = result.composition;
            let index = result.index;
            let id = result.id;
            *result = self.inspection_cache.evaluate(
                &editor.draft,
                &identity,
                options,
                composition,
                index,
                id,
            );
        }
        self.message = None;
        self.selected_result = self
            .selected_result
            .filter(|index| *index < self.results.len());
    }
    fn reset_interpolation_state(&mut self) {
        self.results.clear();
        self.selected_result = None;
        self.inspection_cache.invalidate();
    }
}

pub fn show_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut DatasetEditorState,
    state: &mut GridInspectionUi,
    options: &mut ProjectionOptions,
) -> GridInspectionAction {
    state.ensure_selection(editor);
    let Some(_) = state.selected_field_index(editor) else {
        ui.heading("Grid inspection");
        ui.colored_label(
            egui::Color32::YELLOW,
            state.message.as_deref().unwrap_or(
                "No scalar field is available. Add a phase/property field in the Data tab.",
            ),
        );
        return GridInspectionAction::None;
    };

    ui.heading("Grid inspection");
    let mut inspection_changed = selectors(ui, editor, state);
    ui.horizontal(|ui| {
        ui.label("Mode:");
        inspection_changed |= ui
            .selectable_value(
                &mut state.mode,
                GridInspectionMode::VertexSelection,
                "Vertex selection",
            )
            .changed();
        inspection_changed |= ui
            .selectable_value(
                &mut state.mode,
                GridInspectionMode::Interpolation,
                "Interpolation",
            )
            .changed();
    });
    if state.mode == GridInspectionMode::Interpolation {
        ui.separator();
        ui.label("Interpolation settings");
        let cubic_supported = matches!(
            editor.draft.grids.get(state.selected_grid),
            Some(TabulatedGrid::Regular(_))
        );
        inspection_changed |= controls::show_source_interpolation_controls(
            ui,
            options,
            cubic_supported,
            "grid_inspection",
        );
        ui.checkbox(
            &mut state.show_containing_triangle,
            "Highlight containing triangle",
        );
    }
    if inspection_changed {
        state.inspection_cache.invalidate();
        state.recalculate_interpolation_results(editor, options);
    }
    let Some(field_index) = state.selected_field_index(editor) else {
        return GridInspectionAction::None;
    };
    let grid = &editor.draft.grids[state.selected_grid];
    let field = &grid.fields()[field_index];
    let phase_name = editor
        .draft
        .phases
        .iter()
        .find(|phase| phase.id == field.phase_id)
        .map(|phase| phase.name.as_str())
        .unwrap_or("unknown");
    ui.small(format!(
        "{} / {} / {} ? {}",
        grid.name(),
        phase_name,
        field.property,
        if editor.field_is_modified(state.selected_grid, field_index) {
            "modified draft"
        } else {
            "applied"
        }
    ));

    let counts = editor
        .field_state_counts(state.selected_grid, field_index)
        .unwrap_or([0; 4]);
    ui.separator();
    ui.label("Visibility and state counts");
    ui.checkbox(
        &mut state.show_calculated,
        format!("Calculated: {}", counts[0]),
    );
    ui.checkbox(
        &mut state.show_non_existing,
        format!("Extrapolated: {}", counts[1]),
    );
    ui.checkbox(&mut state.show_cut_off, format!("Cut-off: {}", counts[2]));
    ui.checkbox(&mut state.show_missing, format!("Missing: {}", counts[3]));
    ui.add(egui::Slider::new(&mut state.marker_size, 3.0..=16.0).text("Marker size"));
    ui.checkbox(&mut state.show_regular_edges, "Regular-grid edges");
    egui::ComboBox::from_label("Data labels")
        .selected_text(label_mode_name(state.label_mode))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut state.label_mode, GridLabelMode::None, "None");
            ui.selectable_value(&mut state.label_mode, GridLabelMode::Value, "Value");
            ui.selectable_value(&mut state.label_mode, GridLabelMode::State, "State");
            ui.selectable_value(&mut state.label_mode, GridLabelMode::RowIndex, "Row index");
            ui.selectable_value(
                &mut state.label_mode,
                GridLabelMode::ValueAndState,
                "Value + state",
            );
        });
    ui.add(egui::Slider::new(&mut state.label_precision, 0..=8).text("Label decimals"));
    ui.checkbox(
        &mut state.labels_selected_only,
        "Labels only for selected points",
    );

    ui.horizontal(|ui| {
        if ui.button("Copy selected field TSV").clicked() {
            ctx.copy_text(selected_field_tsv(
                &editor.draft,
                state.selected_grid,
                field_index,
            ));
            state.message = Some("Copied selected field as TSV.".into());
        }
        if ui.button("Copy complete grid TSV").clicked() {
            ctx.copy_text(complete_grid_tsv(&editor.draft, state.selected_grid));
            state.message = Some("Copied complete grid as TSV.".into());
        }
        if ui.button("Previous field").clicked() {
            let fields = editor.draft.grids[state.selected_grid].fields();
            state.select_field(editor, field_index.saturating_sub(1));
            inspection_changed = true;
            if fields.is_empty() {
                state.message = Some("No scalar fields in this grid.".into());
            }
        }
        if ui.button("Next field").clicked() {
            let next = (field_index + 1) % editor.draft.grids[state.selected_grid].fields().len();
            state.select_field(editor, next);
            inspection_changed = true;
        }
    });

    ui.separator();
    let vertex_mode = state.mode == GridInspectionMode::VertexSelection;
    let mut action = if vertex_mode {
        state.sync_point_editor(editor);
        point_editor(ui, editor, state, field_index)
    } else {
        ui.small("Click inside a source triangle to register an interpolation result.");
        GridInspectionAction::None
    };
    ui.separator();
    ui.label(format!("Selected points: {}", state.selected_rows.len()));
    let selected = state.selected_rows.iter().copied().collect::<Vec<_>>();
    ui.horizontal_wrapped(|ui| {
        for (label, value) in [
            ("Set selected to Cut-off", TabulatedValueState::CutOff),
            ("Set selected to Missing", TabulatedValueState::Missing),
        ] {
            if ui
                .add_enabled(
                    vertex_mode && !selected.is_empty(),
                    egui::Button::new(label),
                )
                .clicked()
            {
                state.message = editor
                    .set_field_state_batch(state.selected_grid, field_index, &selected, value, None)
                    .err();
                if state.message.is_none() {
                    action = GridInspectionAction::DraftEdited;
                }
            }
        }
        if ui
            .add_enabled(
                vertex_mode && !selected.is_empty(),
                egui::Button::new("Clear notes"),
            )
            .clicked()
        {
            state.message = editor
                .clear_field_notes(state.selected_grid, field_index, &selected)
                .err();
            if state.message.is_none() {
                action = GridInspectionAction::DraftEdited;
            }
        }
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(vertex_mode, egui::Button::new("Revert selected field"))
            .clicked()
        {
            state.message = editor.revert_field(state.selected_grid, field_index).err();
            if state.message.is_none() {
                state.selected_rows.clear();
                action = GridInspectionAction::DraftEdited;
            }
        }
        if ui
            .add_enabled(editor.dirty, egui::Button::new("Apply edits"))
            .clicked()
        {
            state.message = editor.apply_draft().err();
            if state.message.is_none() {
                action = GridInspectionAction::Applied;
            }
        }
        if ui
            .add_enabled(editor.dirty, egui::Button::new("Apply and recalculate"))
            .clicked()
        {
            state.message = editor.apply_draft().err();
            if state.message.is_none() {
                action = GridInspectionAction::Recalculate;
            }
        }
    });
    if inspection_changed {
        state.inspection_cache.invalidate();
        state.recalculate_interpolation_results(editor, options);
        if matches!(action, GridInspectionAction::None) {
            action = GridInspectionAction::InspectionChanged;
        }
    }
    if matches!(action, GridInspectionAction::DraftEdited) {
        state.inspection_cache.invalidate();
        state.recalculate_interpolation_results(editor, options);
    }
    if let Some(message) = &state.message {
        ui.colored_label(egui::Color32::YELLOW, message);
    }
    action
}

fn selectors(ui: &mut egui::Ui, editor: &DatasetEditorState, state: &mut GridInspectionUi) -> bool {
    let mut changed = false;
    let grids = editor
        .draft
        .grids
        .iter()
        .map(|grid| grid.name().to_owned())
        .collect::<Vec<_>>();
    egui::ComboBox::from_label("Grid")
        .selected_text(grids.get(state.selected_grid).cloned().unwrap_or_default())
        .show_ui(ui, |ui| {
            for (index, name) in grids.iter().enumerate() {
                if ui
                    .selectable_value(&mut state.selected_grid, index, name)
                    .clicked()
                {
                    if let Some(field_index) = editor.draft.grids[index]
                        .fields()
                        .iter()
                        .position(|field| field.property == "T")
                        .or_else(|| (!editor.draft.grids[index].fields().is_empty()).then_some(0))
                    {
                        state.select_field(editor, field_index);
                    }
                    changed = true;
                }
            }
        });
    let phases = editor.draft.grids[state.selected_grid]
        .fields()
        .iter()
        .filter_map(|field| {
            editor
                .draft
                .phases
                .iter()
                .find(|phase| phase.id == field.phase_id)
                .map(|phase| (phase.id, phase.name.clone()))
        })
        .fold(
            Vec::<(StablePhaseId, String)>::new(),
            |mut phases, phase| {
                if !phases.iter().any(|candidate| candidate.0 == phase.0) {
                    phases.push(phase);
                }
                phases
            },
        );
    let phase_name = state
        .selected_phase
        .and_then(|id| phases.iter().find(|phase| phase.0 == id))
        .map(|phase| phase.1.clone())
        .unwrap_or_else(|| "Select phase".into());
    egui::ComboBox::from_label("Phase")
        .selected_text(phase_name)
        .show_ui(ui, |ui| {
            for (phase_id, name) in &phases {
                if ui
                    .selectable_value(&mut state.selected_phase, Some(*phase_id), name)
                    .clicked()
                {
                    if let Some(field_index) = editor.draft.grids[state.selected_grid]
                        .fields()
                        .iter()
                        .position(|field| field.phase_id == *phase_id && field.property == "T")
                        .or_else(|| {
                            editor.draft.grids[state.selected_grid]
                                .fields()
                                .iter()
                                .position(|field| field.phase_id == *phase_id)
                        })
                    {
                        state.select_field(editor, field_index);
                    }
                    changed = true;
                }
            }
        });
    let properties = state
        .selected_phase
        .map(|phase_id| {
            editor.draft.grids[state.selected_grid]
                .fields()
                .iter()
                .filter(|field| field.phase_id == phase_id)
                .map(|field| field.property.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    egui::ComboBox::from_label("Property")
        .selected_text(&state.selected_property)
        .show_ui(ui, |ui| {
            for property in &properties {
                if ui
                    .selectable_value(&mut state.selected_property, property.clone(), property)
                    .clicked()
                {
                    state.selected_rows.clear();
                    state.edit_row = None;
                    changed = true;
                }
            }
        });
    changed
}
fn point_editor(
    ui: &mut egui::Ui,
    editor: &mut DatasetEditorState,
    state: &mut GridInspectionUi,
    field_index: usize,
) -> GridInspectionAction {
    let Some(row) = state.selected_rows.iter().next().copied() else {
        ui.small("Click a point to inspect its composition and edit its classified state.");
        return GridInspectionAction::None;
    };
    let grid = &editor.draft.grids[state.selected_grid];
    let Some(composition) = grid.compositions().get(row) else {
        return GridInspectionAction::None;
    };
    ui.heading(format!("Point {}", row + 1));
    ui.label(format!(
        "composition: ({:.6}, {:.6}, {:.6})",
        composition[0], composition[1], composition[2]
    ));
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(
            &mut state.edit_state,
            TabulatedValueState::Calculated,
            "Calculated",
        );
        ui.selectable_value(
            &mut state.edit_state,
            TabulatedValueState::CutOff,
            "Cut-off",
        );
        ui.selectable_value(
            &mut state.edit_state,
            TabulatedValueState::Missing,
            "Missing",
        );
    });
    ui.add_enabled_ui(
        matches!(state.edit_state, TabulatedValueState::Calculated),
        |ui| {
            ui.horizontal(|ui| {
                ui.label("Scalar value");
                ui.text_edit_singleline(&mut state.edit_value);
            });
        },
    );
    ui.add_enabled_ui(
        !matches!(state.edit_state, TabulatedValueState::Calculated),
        |ui| {
            ui.horizontal(|ui| {
                ui.label(if matches!(state.edit_state, TabulatedValueState::CutOff) {
                    "Cut-off limit / note"
                } else {
                    "Note"
                });
                ui.text_edit_singleline(&mut state.edit_note);
            });
        },
    );
    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
    if ui.button("Apply point").clicked() || enter {
        let value = match state.edit_state {
            TabulatedValueState::Calculated => state
                .edit_value
                .trim()
                .parse::<f64>()
                .map_err(|_| "calculated value must be a finite number".to_owned())
                .and_then(TabulatedValue::calculated),
            classified => Ok(TabulatedValue {
                state: classified,
                value: None,
                extrapolation: None,
                note: (!state.edit_note.trim().is_empty())
                    .then(|| state.edit_note.trim().to_owned()),
            }),
        };
        state.message = value
            .and_then(|value| editor.set_field_point(state.selected_grid, field_index, row, value))
            .err();
        if state.message.is_none() {
            state.edit_row = None;
            return GridInspectionAction::DraftEdited;
        }
    }
    GridInspectionAction::None
}

pub fn show_canvas(
    ui: &mut egui::Ui,
    editor: &DatasetEditorState,
    state: &mut GridInspectionUi,
    options: &ProjectionOptions,
) {
    state.ensure_selection(editor);
    let Some(field_index) = state.selected_field_index(editor) else {
        ui.centered_and_justified(|ui| ui.label("No inspectable scalar field."));
        return;
    };
    let grid = &editor.draft.grids[state.selected_grid];
    let compositions = grid.compositions();
    let values = &grid.fields()[field_index].values;
    let rect = ui.available_rect_before_wrap();
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return;
    }
    let response = ui.allocate_rect(rect, egui::Sense::click());
    let bitmap_width = rect.width().max(1.0) as u32;
    let bitmap_height = rect.height().max(1.0) as u32;
    let transform = ViewerTransform::new(
        TernaryRenderTransform::fit_triangle(bitmap_width, bitmap_height),
        bitmap_width,
        bitmap_height,
        [f64::from(rect.min.x), f64::from(rect.min.y)],
        [f64::from(rect.width()), f64::from(rect.height())],
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, grid_inspection_background());
    let corners = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .map(|point| screen_pos(&transform, point));
    painter.line_segment(
        [corners[0], corners[1]],
        egui::Stroke::new(1.5_f32, egui::Color32::DARK_GRAY),
    );
    painter.line_segment(
        [corners[1], corners[2]],
        egui::Stroke::new(1.5_f32, egui::Color32::DARK_GRAY),
    );
    painter.line_segment(
        [corners[2], corners[0]],
        egui::Stroke::new(1.5_f32, egui::Color32::DARK_GRAY),
    );
    for (corner, component) in corners.into_iter().zip(&editor.draft.components) {
        painter.text(
            corner,
            egui::Align2::CENTER_CENTER,
            &component.name,
            egui::FontId::proportional(13.0),
            egui::Color32::BLACK,
        );
    }
    if state.show_regular_edges
        && matches!(grid, TabulatedGrid::Regular(_))
        && compositions.len() <= 1_500
    {
        for (index, left) in compositions.iter().enumerate() {
            for right in compositions.iter().skip(index + 1) {
                if regular_neighbours(*left, *right) {
                    painter.line_segment(
                        [
                            screen_pos(&transform, *left),
                            screen_pos(&transform, *right),
                        ],
                        egui::Stroke::new(0.5_f32, egui::Color32::from_gray(170)),
                    );
                }
            }
        }
    }
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        match state.mode {
            GridInspectionMode::VertexSelection => {
                let nearest = compositions
                    .iter()
                    .enumerate()
                    .filter(|(row, _)| {
                        values
                            .get(*row)
                            .is_some_and(|value| visible(value.state, state))
                    })
                    .map(|(row, point)| ((pointer - screen_pos(&transform, *point)).length(), row))
                    .filter(|(distance, _)| *distance <= state.marker_size + 5.0)
                    .min_by(|left, right| left.0.total_cmp(&right.0))
                    .map(|(_, row)| row);
                if let Some(row) = nearest {
                    let extend = ui.input(|input| input.modifiers.shift);
                    if extend {
                        if !state.selected_rows.insert(row) {
                            state.selected_rows.remove(&row);
                        }
                    } else {
                        state.selected_rows.clear();
                        state.selected_rows.insert(row);
                    }
                    state.edit_row = None;
                }
            }
            GridInspectionMode::Interpolation => {
                let composition = transform
                    .screen_to_logical([f64::from(pointer.x), f64::from(pointer.y)])
                    .and_then(|logical| transform.logical_to_composition(logical));
                match composition {
                    Some(composition)
                        if composition.into_iter().all(|value| value >= -1.0e-10)
                            && (composition.into_iter().sum::<f64>() - 1.0).abs() <= 1.0e-8 =>
                    {
                        state.register_interpolation_query(editor, options, composition);
                    }
                    _ => {
                        state.message =
                            Some("Interpolation points must lie inside the ternary simplex.".into())
                    }
                }
            }
        }
    }
    for (row, (point, value)) in compositions.iter().zip(values).enumerate() {
        if !visible(value.state, state) {
            continue;
        }
        let position = screen_pos(&transform, *point);
        draw_marker(&painter, position, value.state, state.marker_size);
        if state.selected_rows.contains(&row) {
            painter.circle_stroke(
                position,
                state.marker_size + 3.0,
                egui::Stroke::new(1.5_f32, egui::Color32::YELLOW),
            );
        }
        if state.label_mode != GridLabelMode::None
            && (!state.labels_selected_only || state.selected_rows.contains(&row))
        {
            painter.text(
                position + egui::vec2(state.marker_size + 2.0, -state.marker_size - 2.0),
                egui::Align2::LEFT_BOTTOM,
                label(value, row, state.label_mode, state.label_precision),
                egui::FontId::monospace(10.0),
                egui::Color32::BLACK,
            );
        }
    }
    if state.mode == GridInspectionMode::Interpolation {
        for (result_row, result) in state.results.iter().enumerate() {
            let selected = state.selected_result == Some(result_row);
            let latest = result_row + 1 == state.results.len();
            if state.show_containing_triangle
                && (selected || latest)
                && let Some(vertices) = result.triangle_vertex_indices
            {
                let triangle = vertices
                    .into_iter()
                    .filter_map(|index| compositions.get(index).copied())
                    .map(|point| screen_pos(&transform, point))
                    .collect::<Vec<_>>();
                if triangle.len() == 3 {
                    painter.add(egui::Shape::closed_line(
                        vec![triangle[0], triangle[1], triangle[2], triangle[0]],
                        egui::Stroke::new(
                            if selected { 2.0_f32 } else { 1.0_f32 },
                            egui::Color32::from_rgb(80, 110, 220),
                        ),
                    ));
                }
            }
            let point = screen_pos(&transform, result.composition);
            let colour = match result.state {
                InterpolatedResultState::Defined => egui::Color32::from_rgb(41, 128, 185),
                _ => egui::Color32::from_rgb(192, 57, 43),
            };
            painter.circle_filled(point, if latest { 7.0 } else { 4.0 }, colour);
            if selected {
                painter.circle_stroke(
                    point,
                    10.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                );
            }
            painter.text(
                point + egui::vec2(7.0, -7.0),
                egui::Align2::LEFT_BOTTOM,
                result.index.to_string(),
                egui::FontId::monospace(10.0),
                egui::Color32::BLACK,
            );
        }
    }
}

/// Render the independently scrollable right-side interpolation-results pane.
pub fn show_results(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut GridInspectionUi) {
    ui.heading("Interpolated Results");
    ui.small("Source rows are displayed 1-based; Rust/grid indices remain 0-based internally.");
    let copy_allowed = !state.results.iter().any(|result| result.stale);
    if !copy_allowed {
        ui.colored_label(
            egui::Color32::YELLOW,
            "Recalculate successfully before copying stale interpolation results.",
        );
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("Clear").clicked() {
            state.results.clear();
            state.selected_result = None;
        }
        if ui
            .add_enabled(
                state.selected_result.is_some(),
                egui::Button::new("Delete selected"),
            )
            .clicked()
            && let Some(index) = state.selected_result.take()
            && index < state.results.len()
        {
            state.results.remove(index);
            state.selected_result = index
                .checked_sub(1)
                .filter(|index| *index < state.results.len());
        }
        if ui
            .add_enabled(
                state.selected_result.is_some() && copy_allowed,
                egui::Button::new("Copy selected"),
            )
            .clicked()
            && let Some(index) = state.selected_result
            && let Some(result) = state.results.get(index)
        {
            ctx.copy_text(result_tsv(std::slice::from_ref(result), state, false));
        }
        if ui
            .add_enabled(copy_allowed, egui::Button::new("Copy all"))
            .clicked()
        {
            ctx.copy_text(result_tsv(&state.results, state, true));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.show_local_barycentric, "Local lambda");
        ui.checkbox(&mut state.show_contributions, "Linear + excess");
        ui.checkbox(&mut state.show_triangle_index, "Triangle index");
        ui.checkbox(&mut state.show_triangle_vertices, "Source rows");
    });
    if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::C))
        && copy_allowed
        && state.results_table_has_focus
        && let Some(index) = state.selected_result
        && let Some(result) = state.results.get(index)
    {
        ctx.copy_text(result_tsv(std::slice::from_ref(result), state, false));
    }

    let available = ui.available_size();
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .max_height(available.y)
        .show(ui, |ui| {
            egui::Grid::new("interpolation_results_table")
                .striped(true)
                .min_col_width(72.0)
                .show(ui, |ui| {
                    ui.strong("Index");
                    ui.strong("Grid");
                    ui.strong("Phase");
                    ui.strong("Property");
                    let names = state
                        .results
                        .first()
                        .map(|result| result.component_names.clone())
                        .unwrap_or_else(|| ["A".into(), "B".into(), "C".into()]);
                    ui.strong(format!("A ({})", names[0]));
                    ui.strong(format!("B ({})", names[1]));
                    ui.strong(format!("C ({})", names[2]));
                    ui.strong("Method");
                    ui.strong("State");
                    ui.strong("Interpolated value");
                    ui.strong("Unit");
                    if state.show_local_barycentric {
                        ui.strong("Local lambda0");
                        ui.strong("Local lambda1");
                        ui.strong("Local lambda2");
                    }
                    if state.show_contributions {
                        ui.strong("Linear part");
                        ui.strong("Excess part");
                    }
                    if state.show_triangle_index {
                        ui.strong("Triangle index0");
                    }
                    if state.show_triangle_vertices {
                        ui.strong("Vertex 0 row");
                        ui.strong("Vertex 1 row");
                        ui.strong("Vertex 2 row");
                    }
                    ui.end_row();

                    for (row_index, result) in state.results.iter().enumerate() {
                        let selected = state.selected_result == Some(row_index);
                        let response = ui.selectable_label(selected, result.index.to_string());
                        if response.clicked() {
                            state.selected_result = Some(row_index);
                            state.results_table_has_focus = true;
                        }
                        ui.label(&result.grid_name);
                        ui.label(&result.phase_name);
                        ui.label(&result.field.property);
                        for value in result.composition {
                            ui.label(format!("{value:.6}"));
                        }
                        ui.label(result.method_label());
                        ui.label(if result.stale {
                            format!("{} (stale)", result.state.label())
                        } else {
                            result.state.label().to_owned()
                        })
                        .on_hover_text(
                            result
                                .stale_error
                                .as_deref()
                                .unwrap_or(match &result.state {
                                    InterpolatedResultState::Error(message) => message,
                                    _ => "",
                                }),
                        );
                        ui.label(
                            result
                                .value
                                .map(|value| format!("{value:.8}"))
                                .unwrap_or_else(|| result.state.value_token().into()),
                        );
                        ui.label(&result.unit);
                        if state.show_local_barycentric {
                            if let Some(values) = result.local_barycentric {
                                for value in values {
                                    ui.label(format!("{value:.6}"));
                                }
                            } else {
                                for _ in 0..3 {
                                    ui.label("N/A");
                                }
                            }
                        }
                        if state.show_contributions {
                            ui.label(format_optional(result.linear_part));
                            ui.label(format_optional(result.excess_part));
                        }
                        if state.show_triangle_index {
                            ui.label(
                                result
                                    .triangle_index
                                    .map(|index| index.to_string())
                                    .unwrap_or_else(|| "N/A".into()),
                            );
                        }
                        if state.show_triangle_vertices {
                            for index in result.triangle_vertex_indices.unwrap_or([usize::MAX; 3]) {
                                ui.label(if index == usize::MAX {
                                    "N/A".into()
                                } else {
                                    displayed_source_row(index).to_string()
                                });
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

fn format_optional(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.8}"))
        .unwrap_or_else(|| "N/A".into())
}

/// Convert a zero-based Rust/grid index to the one-based source row shown in
/// Grid inspection and emitted by its TSV export.
pub(crate) const fn displayed_source_row(index: usize) -> usize {
    index + 1
}
fn result_tsv(results: &[InterpolatedResult], state: &GridInspectionUi, full: bool) -> String {
    let mut header = vec!["A", "B", "C", "InterpolatedValue", "State"];
    if full {
        header.splice(
            0..0,
            [
                "Index",
                "Grid",
                "Phase",
                "Property",
                "InterpolationMethod",
                "Unit",
            ],
        );
        if state.show_local_barycentric {
            header.extend(["local_lambda0", "local_lambda1", "local_lambda2"]);
        }
        if state.show_contributions {
            header.extend(["linear_part", "excess_part"]);
        }
        if state.show_triangle_index {
            header.push("triangle_index0");
        }
        if state.show_triangle_vertices {
            header.extend(["vertex_0_row", "vertex_1_row", "vertex_2_row"]);
        }
    }
    let mut rows = vec![header.join("\t")];
    for result in results {
        let mut cells = Vec::new();
        if full {
            cells.extend([
                result.index.to_string(),
                result.grid_name.clone(),
                result.phase_name.clone(),
                result.field.property.clone(),
                result.method_label(),
                result.unit.clone(),
            ]);
        }
        cells.extend(result.composition.map(|value| format!("{value:.10}")));
        cells.push(
            result
                .value
                .map(|value| format!("{value:.10}"))
                .unwrap_or_else(|| result.state.value_token().into()),
        );
        cells.push(result.state.label().into());
        if full && state.show_local_barycentric {
            if let Some(values) = result.local_barycentric {
                cells.extend(values.map(|value| format!("{value:.10}")));
            } else {
                cells.extend(["N/A".into(), "N/A".into(), "N/A".into()]);
            }
        }
        if full && state.show_contributions {
            cells.extend([
                format_optional(result.linear_part),
                format_optional(result.excess_part),
            ]);
        }
        if full && state.show_triangle_index {
            cells.push(
                result
                    .triangle_index
                    .map(|index| index.to_string())
                    .unwrap_or_else(|| "N/A".into()),
            );
        }
        if full && state.show_triangle_vertices {
            cells.extend(
                result
                    .triangle_vertex_indices
                    .unwrap_or([usize::MAX; 3])
                    .map(|index| {
                        if index == usize::MAX {
                            "N/A".into()
                        } else {
                            displayed_source_row(index).to_string()
                        }
                    }),
            );
        }
        rows.push(cells.join("\t"));
    }
    rows.join("\n") + "\n"
}
fn selected_field_tsv(
    dataset: &crate::TabulatedTernaryDataset,
    grid_index: usize,
    field_index: usize,
) -> String {
    let grid = &dataset.grids[grid_index];
    let field = &grid.fields()[field_index];
    let phase = dataset
        .phases
        .iter()
        .find(|phase| phase.id == field.phase_id)
        .map(|phase| phase.name.as_str())
        .unwrap_or("unknown");
    let mut rows = vec![format!(
        "{}\\t{}\\t{}\\t{}.{}",
        dataset.components[0].name,
        dataset.components[1].name,
        dataset.components[2].name,
        phase,
        field.property
    )];
    for (point, value) in grid.compositions().iter().zip(&field.values) {
        rows.push(format!(
            "{:.6}\\t{:.6}\\t{:.6}\\t{}",
            point[0],
            point[1],
            point[2],
            value.token_with_format(
                |number| NumericFormat::default().format(number),
                dataset
                    .missing_tokens
                    .first()
                    .map(String::as_str)
                    .unwrap_or("NA")
            )
        ));
    }
    rows.join("\\n") + "\\n"
}

fn complete_grid_tsv(dataset: &crate::TabulatedTernaryDataset, grid_index: usize) -> String {
    let grid = &dataset.grids[grid_index];
    let mut rows = Vec::new();
    let mut header = dataset
        .components
        .iter()
        .map(|component| component.name.clone())
        .collect::<Vec<_>>();
    header.extend(grid.fields().iter().map(|field| {
        let phase = dataset
            .phases
            .iter()
            .find(|phase| phase.id == field.phase_id)
            .map(|phase| phase.name.as_str())
            .unwrap_or("unknown");
        format!("{phase}.{}", field.property)
    }));
    rows.push(header.join("\t"));
    for (row, point) in grid.compositions().iter().enumerate() {
        let mut cells = point.map(|value| format!("{value:.6}")).to_vec();
        cells.extend(grid.fields().iter().map(|field| {
            field.values[row].token_with_format(
                |number| NumericFormat::default().format(number),
                dataset
                    .missing_tokens
                    .first()
                    .map(String::as_str)
                    .unwrap_or("NA"),
            )
        }));
        rows.push(cells.join("\t"));
    }
    rows.join("\n") + "\n"
}

fn label(value: &TabulatedValue, row: usize, mode: GridLabelMode, precision: usize) -> String {
    let scalar = value
        .defined_value()
        .map(|number| format!("{number:.precision$}"))
        .unwrap_or_else(|| value.state.token().into());
    match mode {
        GridLabelMode::None => String::new(),
        GridLabelMode::Value => value
            .defined_value()
            .map(|number| format!("{number:.precision$}"))
            .unwrap_or_else(|| value.state.token().into()),
        GridLabelMode::State => value.state.token().into(),
        GridLabelMode::RowIndex => (row + 1).to_string(),
        GridLabelMode::ValueAndState => format!("{scalar} ({})", value.state.token()),
    }
}

fn label_mode_name(mode: GridLabelMode) -> &'static str {
    match mode {
        GridLabelMode::None => "None",
        GridLabelMode::Value => "Value",
        GridLabelMode::State => "State",
        GridLabelMode::RowIndex => "Row index",
        GridLabelMode::ValueAndState => "Value + state",
    }
}

fn visible(value: TabulatedValueState, state: &GridInspectionUi) -> bool {
    match value {
        TabulatedValueState::Calculated | TabulatedValueState::Extrapolated => {
            state.show_calculated
        }
        TabulatedValueState::CutOff => state.show_cut_off,
        TabulatedValueState::Missing => state.show_missing,
    }
}

fn grid_inspection_background() -> egui::Color32 {
    egui::Color32::from_rgb(
        PLOT_BACKGROUND_RGB[0],
        PLOT_BACKGROUND_RGB[1],
        PLOT_BACKGROUND_RGB[2],
    )
}

fn screen_pos(transform: &ViewerTransform, composition: [f64; 3]) -> egui::Pos2 {
    let logical = transform
        .composition_to_logical(composition)
        .expect("validated grid composition");
    let screen = transform.logical_to_screen(logical);
    egui::pos2(screen[0] as f32, screen[1] as f32)
}

fn regular_neighbours(left: [f64; 3], right: [f64; 3]) -> bool {
    let changes = left
        .into_iter()
        .zip(right)
        .filter(|(left, right)| (*left - *right).abs() > 1.0e-10)
        .collect::<Vec<_>>();
    changes.len() == 2 && (changes[0].0 - changes[0].1).abs() == (changes[1].0 - changes[1].1).abs()
}

fn draw_marker(painter: &egui::Painter, point: egui::Pos2, state: TabulatedValueState, size: f32) {
    match state {
        TabulatedValueState::Calculated => {
            painter.circle_filled(point, size, egui::Color32::from_rgb(46, 204, 113));
        }
        TabulatedValueState::Extrapolated => {
            painter.circle_filled(point, size, egui::Color32::from_rgb(74, 144, 226));
            painter.text(
                point,
                egui::Align2::CENTER_CENTER,
                "EX",
                egui::FontId::proportional((size * 0.85).max(7.0)),
                egui::Color32::WHITE,
            );
        }
        TabulatedValueState::CutOff => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    point + egui::vec2(0.0, -size),
                    point + egui::vec2(size, size),
                    point + egui::vec2(-size, size),
                ],
                egui::Color32::from_rgb(230, 126, 34),
                egui::Stroke::new(1.0_f32, egui::Color32::DARK_RED),
            ));
        }
        TabulatedValueState::Missing => {
            painter.circle_stroke(
                point,
                size,
                egui::Stroke::new(1.5_f32, egui::Color32::LIGHT_GRAY),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialise_prefers_the_first_temperature_field() {
        let editor = DatasetEditorState::new(crate::default_regular_dataset());
        let mut state = GridInspectionUi::default();
        state.initialise(&editor);
        assert_eq!(state.selected_grid, 0);
        assert_eq!(state.selected_phase, Some(StablePhaseId(1)));
        assert_eq!(state.selected_property, "T");
    }

    #[test]
    fn labels_and_marker_visibility_keep_states_distinct() {
        let state = GridInspectionUi {
            show_missing: false,
            ..GridInspectionUi::default()
        };
        assert!(!visible(TabulatedValueState::Missing, &state));
        assert!(visible(TabulatedValueState::CutOff, &state));
        assert_eq!(
            label(&TabulatedValue::missing(), 3, GridLabelMode::State, 3),
            "NA"
        );
        assert_eq!(
            label(
                &TabulatedValue::cut_off(),
                3,
                GridLabelMode::ValueAndState,
                3
            ),
            "CO (CO)"
        );
    }

    #[test]
    fn interpolation_tsv_uses_unambiguous_one_based_source_rows() {
        let result = InterpolatedResult {
            id: 1,
            index: 1,
            field: InspectionFieldIdentity {
                grid_index: 0,
                phase_id: StablePhaseId(1),
                property: "T".into(),
            },
            grid_name: "regular".into(),
            phase_name: "Phase1".into(),
            component_names: ["A".into(), "B".into(), "C".into()],
            unit: "K".into(),
            composition: [0.5, 0.25, 0.25],
            source_interpolation: crate::SourceInterpolation::Linear,
            partial_domain_policy: ternary_contours::CubicPartialDomainPolicy::OneSidedThenLinear,
            state: InterpolatedResultState::Defined,
            value: Some(1_234.5),
            triangle_index: Some(7),
            local_barycentric: Some([0.2, 0.3, 0.5]),
            triangle_vertex_indices: Some([0, 11, 22]),
            linear_part: Some(1_234.5),
            excess_part: Some(0.0),
            local_mode: Some(ternary_contours::LocalInterpolationMode::Linear),
            source_provenance: Default::default(),
            stale_error: None,
            stale: false,
        };
        let state = GridInspectionUi {
            show_triangle_vertices: true,
            ..GridInspectionUi::default()
        };
        let tsv = result_tsv(&[result], &state, true);
        let mut lines = tsv.lines();
        let header = lines.next().unwrap();
        assert!(header.contains("vertex_0_row\tvertex_1_row\tvertex_2_row"));
        let row = lines.next().unwrap().split('\t').collect::<Vec<_>>();
        assert_eq!(&row[row.len() - 3..], ["1", "12", "23"]);
        assert_eq!(displayed_source_row(0), 1);
        assert_eq!(displayed_source_row(22), 23);
    }

    #[test]
    fn interpolation_mode_uses_the_plot_background_and_keeps_results_across_modes() {
        assert_eq!(
            grid_inspection_background(),
            egui::Color32::from_rgb(
                PLOT_BACKGROUND_RGB[0],
                PLOT_BACKGROUND_RGB[1],
                PLOT_BACKGROUND_RGB[2],
            )
        );
        let mut editor = DatasetEditorState::new(crate::default_regular_dataset());
        let TabulatedGrid::Regular(grid) = &mut editor.draft.grids[0] else {
            unreachable!("default dataset uses a regular grid");
        };
        for field in &mut grid.fields {
            for (index, value) in field.values.iter_mut().enumerate() {
                *value = TabulatedValue::calculated(1_000.0 + index as f64).unwrap();
            }
        }
        let mut state = GridInspectionUi::default();
        state.initialise(&editor);
        assert_eq!(state.mode, GridInspectionMode::VertexSelection);
        state.mode = GridInspectionMode::Interpolation;
        let mut options = ProjectionOptions::default();
        state.register_interpolation_query(&editor, &options, [0.4, 0.3, 0.3]);
        assert_eq!(state.results.len(), 1);
        assert_eq!(
            state.results[0].source_interpolation,
            crate::SourceInterpolation::Linear
        );
        state.mode = GridInspectionMode::VertexSelection;
        assert_eq!(state.results.len(), 1);
        options.source_interpolation = crate::SourceInterpolation::CubicAlpha {
            method: ternary_contours::CubicAlphaMethod::Akima,
            continuation: ternary_contours::BinaryExtrapolation::RawBarycentric,
        };
        state.recalculate_interpolation_results(&editor, &options);
        assert_eq!(
            state.results[0].source_interpolation,
            options.source_interpolation
        );
        state.select_field(&editor, 1);
        state.recalculate_interpolation_results(&editor, &options);
        assert_eq!(state.results[0].field.phase_id, StablePhaseId(2));
    }
    #[test]
    fn registered_queries_keep_ids_coordinates_order_and_marker_positions_on_recalculation() {
        let mut editor = DatasetEditorState::new(crate::default_regular_dataset());
        let TabulatedGrid::Regular(grid) = &mut editor.draft.grids[0] else {
            unreachable!("default dataset uses a regular grid");
        };
        for field in &mut grid.fields {
            for (row, value) in field.values.iter_mut().enumerate() {
                *value = TabulatedValue::calculated(1_000.0 + row as f64).unwrap();
            }
        }
        let mut state = GridInspectionUi::default();
        state.initialise(&editor);
        let options = ProjectionOptions::default();
        state.register_interpolation_query(&editor, &options, [0.5, 0.3, 0.2]);
        state.register_interpolation_query(&editor, &options, [0.3, 0.2, 0.5]);
        let queries = state
            .results
            .iter()
            .map(|result| (result.id, result.composition))
            .collect::<Vec<_>>();
        let mut changed = options;
        changed.source_interpolation = crate::SourceInterpolation::CubicAlpha {
            method: ternary_contours::CubicAlphaMethod::Makima,
            continuation: ternary_contours::BinaryExtrapolation::Kohler,
        };
        state.recalculate_interpolation_results(&editor, &changed);
        assert_eq!(state.results.len(), 2);
        assert_eq!(
            state
                .results
                .iter()
                .map(|result| (result.id, result.composition))
                .collect::<Vec<_>>(),
            queries
        );
        assert!(
            state
                .results
                .iter()
                .all(|result| result.source_interpolation == changed.source_interpolation)
        );
        assert!(state.results.iter().all(|result| !result.stale));
    }
    #[test]
    fn global_interpolator_failure_marks_previous_queries_stale_without_erasing_them() {
        let dataset =
            crate::parse_str(include_str!("../../fixtures/irregular-phase-grids.tct")).unwrap();
        let editor = DatasetEditorState::new(dataset);
        let mut state = GridInspectionUi::default();
        state.initialise(&editor);
        let linear = ProjectionOptions::default();
        state.register_interpolation_query(&editor, &linear, [0.2, 0.3, 0.5]);
        let before = state.results[0].clone();
        let mut unavailable = linear;
        unavailable.source_interpolation = crate::SourceInterpolation::CubicAlpha {
            method: ternary_contours::CubicAlphaMethod::Akima,
            continuation: ternary_contours::BinaryExtrapolation::Muggianu,
        };
        state.recalculate_interpolation_results(&editor, &unavailable);
        assert_eq!(state.results.len(), 1);
        assert_eq!(state.results[0].id, before.id);
        assert_eq!(state.results[0].composition, before.composition);
        assert_eq!(state.results[0].value, before.value);
        assert!(state.results[0].stale);
        assert!(state.results[0].stale_error.is_some());
    }
}
