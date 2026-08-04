//! Ternary grid inspection and classified point editing controls.

use std::collections::BTreeSet;

use eframe::egui;
use ternary_contours::StablePhaseId;

use crate::{
    DatasetEditorState, NumericFormat, TabulatedGrid, TabulatedValue, TabulatedValueState,
};

use super::hit_test::ViewerTransform;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridInspectionAction {
    None,
    Applied,
    Recalculate,
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

#[derive(Clone, Debug)]
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
            .calculated_value()
            .map(|number| number.to_string())
            .unwrap_or_default();
        self.edit_note = value.note.clone().unwrap_or_default();
    }
}

pub fn show_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut DatasetEditorState,
    state: &mut GridInspectionUi,
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
    selectors(ui, editor, state);
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
        format!("Non-existing: {}", counts[1]),
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
            if fields.is_empty() {
                state.message = Some("No scalar fields in this grid.".into());
            }
        }
        if ui.button("Next field").clicked() {
            let next = (field_index + 1) % editor.draft.grids[state.selected_grid].fields().len();
            state.select_field(editor, next);
        }
    });

    ui.separator();
    state.sync_point_editor(editor);
    let mut action = point_editor(ui, editor, state, field_index);
    ui.separator();
    ui.label(format!("Selected points: {}", state.selected_rows.len()));
    let selected = state.selected_rows.iter().copied().collect::<Vec<_>>();
    ui.horizontal_wrapped(|ui| {
        for (label, value) in [
            (
                "Set selected to Non-existing",
                TabulatedValueState::NonExisting,
            ),
            ("Set selected to Cut-off", TabulatedValueState::CutOff),
            ("Set selected to Missing", TabulatedValueState::Missing),
        ] {
            if ui
                .add_enabled(!selected.is_empty(), egui::Button::new(label))
                .clicked()
            {
                state.message = editor
                    .set_field_state_batch(state.selected_grid, field_index, &selected, value, None)
                    .err();
                if state.message.is_none() {
                    action = GridInspectionAction::Applied;
                }
            }
        }
        if ui
            .add_enabled(!selected.is_empty(), egui::Button::new("Clear notes"))
            .clicked()
        {
            state.message = editor
                .clear_field_notes(state.selected_grid, field_index, &selected)
                .err();
            if state.message.is_none() {
                action = GridInspectionAction::Applied;
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button("Revert selected field").clicked() {
            state.message = editor.revert_field(state.selected_grid, field_index).err();
            if state.message.is_none() {
                state.selected_rows.clear();
                action = GridInspectionAction::Applied;
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
    if let Some(message) = &state.message {
        ui.colored_label(egui::Color32::YELLOW, message);
    }
    action
}

fn selectors(ui: &mut egui::Ui, editor: &DatasetEditorState, state: &mut GridInspectionUi) {
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
                    state.initialise(editor);
                    state.selected_grid = index;
                    if let Some(field_index) = editor.draft.grids[index]
                        .fields()
                        .iter()
                        .position(|field| field.property == "T")
                        .or_else(|| (!editor.draft.grids[index].fields().is_empty()).then_some(0))
                    {
                        state.select_field(editor, field_index);
                    }
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
                    && let Some(field) = editor.draft.grids[state.selected_grid]
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
                    state.select_field(editor, field);
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
                }
            }
        });
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
            TabulatedValueState::NonExisting,
            "Non-existing",
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
                note: (!state.edit_note.trim().is_empty())
                    .then(|| state.edit_note.trim().to_owned()),
            }),
        };
        state.message = value
            .and_then(|value| editor.set_field_point(state.selected_grid, field_index, row, value))
            .err();
        if state.message.is_none() {
            state.edit_row = None;
            return GridInspectionAction::Applied;
        }
    }
    GridInspectionAction::None
}

pub fn show_canvas(ui: &mut egui::Ui, editor: &DatasetEditorState, state: &mut GridInspectionUi) {
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
    let transform = ViewerTransform::new(
        rect.width().max(1.0) as u32,
        rect.height().max(1.0) as u32,
        [f64::from(rect.min.x), f64::from(rect.min.y)],
        [f64::from(rect.width()), f64::from(rect.height())],
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(20));
    let corners = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .map(|point| screen_pos(&transform, point));
    painter.line_segment(
        [corners[0], corners[1]],
        egui::Stroke::new(1.5_f32, egui::Color32::LIGHT_GRAY),
    );
    painter.line_segment(
        [corners[1], corners[2]],
        egui::Stroke::new(1.5_f32, egui::Color32::LIGHT_GRAY),
    );
    painter.line_segment(
        [corners[2], corners[0]],
        egui::Stroke::new(1.5_f32, egui::Color32::LIGHT_GRAY),
    );
    for (corner, component) in corners.into_iter().zip(&editor.draft.components) {
        painter.text(
            corner,
            egui::Align2::CENTER_CENTER,
            &component.name,
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
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
                        egui::Stroke::new(0.5_f32, egui::Color32::from_gray(70)),
                    );
                }
            }
        }
    }
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
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
                egui::Color32::WHITE,
            );
        }
    }
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
        .calculated_value()
        .map(|number| format!("{number:.precision$}"))
        .unwrap_or_else(|| value.state.token().into());
    match mode {
        GridLabelMode::None => String::new(),
        GridLabelMode::Value => value
            .calculated_value()
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
        TabulatedValueState::Calculated => state.show_calculated,
        TabulatedValueState::NonExisting => state.show_non_existing,
        TabulatedValueState::CutOff => state.show_cut_off,
        TabulatedValueState::Missing => state.show_missing,
    }
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
        TabulatedValueState::NonExisting => {
            let stroke = egui::Stroke::new(2.0_f32, egui::Color32::GRAY);
            painter.line_segment(
                [
                    point - egui::vec2(size, size),
                    point + egui::vec2(size, size),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    point + egui::vec2(size, -size),
                    point - egui::vec2(size, -size),
                ],
                stroke,
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
            label(&TabulatedValue::non_existing(), 3, GridLabelMode::State, 3),
            "NE"
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
}
