//! Native viewer controls for focused tabular data entry.

use eframe::egui;

use crate::{
    CompositionNormalization, DatasetEditorState, FieldColumnMapping, FieldKey, FieldReplacement,
    HeaderMode, IrregularPasteMapping, NumericFormat, PastePreview, RegularCompositionPasteMode,
    RegularPasteMapping, TabulatedGrid, compositions_tsv, preview_irregular_paste,
    preview_regular_paste,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataEditorAction {
    None,
    Recalculate,
}

#[derive(Clone, Debug)]
pub struct DataEditorUi {
    pub selected_grid: usize,
    pub selected_field: usize,
    pub regular_paste: String,
    pub irregular_paste: String,
    pub regular_mode: RegularCompositionPasteMode,
    pub normalization: CompositionNormalization,
    pub replacement: FieldReplacement,
    pub blank_missing: bool,
    pub has_header: bool,
    pub irregular_columns: [usize; 3],
    pub irregular_name: String,
    pub confirm_remove: bool,
    pub message: Option<String>,
    pub phase_remove: Option<usize>,
    pub property_remove: Option<usize>,
    pub resolution_input: String,
    pub resolution_pending: Option<usize>,
    pub apply_declarations: bool,
}

impl Default for DataEditorUi {
    fn default() -> Self {
        Self {
            selected_grid: 0,
            selected_field: 0,
            regular_paste: String::new(),
            irregular_paste: String::new(),
            regular_mode: RegularCompositionPasteMode::ValuesOnly,
            normalization: CompositionNormalization::NormalizeWithinTolerance,
            replacement: FieldReplacement::ReplaceExistingField,
            blank_missing: false,
            has_header: true,
            irregular_columns: [0, 1, 2],
            irregular_name: "irregular_data".into(),
            confirm_remove: false,
            message: None,
            phase_remove: None,
            property_remove: None,
            resolution_input: String::new(),
            resolution_pending: None,
            apply_declarations: false,
        }
    }
}

pub fn show(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut DatasetEditorState,
    state: &mut DataEditorUi,
) -> DataEditorAction {
    state.selected_grid = state
        .selected_grid
        .min(editor.draft.grids.len().saturating_sub(1));
    declarations(ui, editor, state);
    if state.apply_declarations {
        state.apply_declarations = false;
        state.message = editor.apply_draft().err();
    }
    ui.separator();
    grid_list(ui, editor, state);
    ui.separator();
    let mut action = DataEditorAction::None;
    if let Some(grid) = editor.draft.grids.get(state.selected_grid) {
        match grid {
            TabulatedGrid::Regular(_) => {
                action = regular_editor(ctx, ui, editor, state);
            }
            TabulatedGrid::Irregular(_) => {
                action = irregular_editor(ctx, ui, editor, state);
            }
        }
    }
    ui.separator();
    preview_panel(ctx, ui, editor);
    ui.separator();
    action
}

fn declarations(ui: &mut egui::Ui, editor: &mut DatasetEditorState, state: &mut DataEditorUi) {
    ui.collapsing("Dataset declarations", |ui| {
        ui.horizontal(|ui| {
            ui.label("Title");
            let title = editor.draft.title.get_or_insert_with(String::new);
            if ui.text_edit_singleline(title).changed() {
                editor.dirty = true;
            }
        });
        ui.horizontal(|ui| {
            for component in &mut editor.draft.components {
                if ui.text_edit_singleline(&mut component.name).changed() {
                    editor.dirty = true;
                }
            }
        });
        ui.label("Phase order controls display order. Stable phase IDs are not renumbered.");
        for index in 0..editor.draft.phases.len() {
            let mut remove = false;
            let mut up = false;
            let mut down = false;
            ui.horizontal(|ui| {
                let phase = &mut editor.draft.phases[index];
                ui.label(format!("position {}", index + 1));
                ui.label(format!("ID {}", phase.id.0));
                if ui.text_edit_singleline(&mut phase.name).changed() {
                    editor.dirty = true;
                }
                up = ui.add_enabled(index > 0, egui::Button::new("Up")).clicked();
                down = ui
                    .add_enabled(
                        index + 1 < editor.draft.phases.len(),
                        egui::Button::new("Down"),
                    )
                    .clicked();
                remove = ui.button("Remove").clicked();
            });
            if up {
                state.message = editor.reorder_phase(index, -1).err();
            } else if down {
                state.message = editor.reorder_phase(index, 1).err();
            } else if remove {
                if editor.phase_field_references(index).is_empty() {
                    state.message = editor.remove_phase(index).err();
                } else {
                    state.phase_remove = Some(index);
                }
            }
        }
        if let Some(index) = state.phase_remove {
            let references = editor.phase_field_references(index);
            ui.group(|ui| {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Removing this phase will remove referenced grid fields:",
                );
                for reference in &references {
                    ui.label(format!("- {reference}"));
                }
                ui.horizontal(|ui| {
                    if ui.button("Confirm phase removal").clicked() {
                        state.message = editor.remove_phase_confirmed(index).err();
                        state.phase_remove = None;
                    }
                    if ui.button("Cancel").clicked() {
                        state.phase_remove = None;
                    }
                });
            });
        }
        ui.separator();
        ui.label("Property order controls display order. T is always required.");
        for index in 0..editor.draft.properties.len() {
            let mut remove = false;
            let mut up = false;
            let mut down = false;
            ui.horizontal(|ui| {
                let property = &mut editor.draft.properties[index];
                let is_temperature = property.name == "T";
                if is_temperature {
                    ui.label("T");
                } else if ui.text_edit_singleline(&mut property.name).changed() {
                    editor.dirty = true;
                }
                if ui
                    .add_enabled(
                        !is_temperature,
                        egui::Checkbox::new(&mut property.required, "required"),
                    )
                    .changed()
                {
                    editor.dirty = true;
                }
                if is_temperature {
                    property.required = true;
                }
                if ui.text_edit_singleline(&mut property.unit).changed() {
                    editor.dirty = true;
                }
                up = ui.add_enabled(index > 0, egui::Button::new("Up")).clicked();
                down = ui
                    .add_enabled(
                        index + 1 < editor.draft.properties.len(),
                        egui::Button::new("Down"),
                    )
                    .clicked();
                remove = ui
                    .add_enabled(!is_temperature, egui::Button::new("Remove"))
                    .clicked();
            });
            if up {
                state.message = editor.reorder_property(index, -1).err();
            } else if down {
                state.message = editor.reorder_property(index, 1).err();
            } else if remove {
                if editor.property_field_references(index).is_empty() {
                    state.message = editor.remove_property(index).err();
                } else {
                    state.property_remove = Some(index);
                }
            }
        }
        if let Some(index) = state.property_remove {
            let references = editor.property_field_references(index);
            ui.group(|ui| {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Removing this property will remove referenced grid fields:",
                );
                for reference in &references {
                    ui.label(format!("- {reference}"));
                }
                ui.horizontal(|ui| {
                    if ui.button("Confirm property removal").clicked() {
                        state.message = editor.remove_property_confirmed(index).err();
                        state.property_remove = None;
                    }
                    if ui.button("Cancel").clicked() {
                        state.property_remove = None;
                    }
                });
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Add phase declaration").clicked() {
                state.message = editor.add_phase().err();
            }
            if ui.button("Add optional property").clicked() {
                state.message = editor.add_property().err();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Apply declarations").clicked() {
                state.apply_declarations = true;
            }
        });
        ui.small("T cannot be renamed, removed, or made optional.");
    });
}
fn grid_list(ui: &mut egui::Ui, editor: &mut DatasetEditorState, state: &mut DataEditorUi) {
    ui.heading("Grids");
    for (index, grid) in editor.draft.grids.iter().enumerate() {
        let (defined, missing) = grid
            .fields()
            .iter()
            .fold((0, 0), |(defined, missing), field| {
                let known = field.values.iter().filter(|value| value.is_some()).count();
                (defined + known, missing + field.values.len() - known)
            });
        let detail = match grid {
            TabulatedGrid::Regular(value) => format!(
                "regular n={}, rows={}",
                value.subdivisions,
                value.compositions.len()
            ),
            TabulatedGrid::Irregular(value) => {
                format!("irregular points={}", value.compositions.len())
            }
        };
        if ui
            .selectable_label(
                state.selected_grid == index,
                format!(
                    "{} - {detail}; fields={}, defined={defined}, undefined={missing}",
                    grid.name(),
                    grid.fields().len()
                ),
            )
            .clicked()
        {
            state.selected_grid = index;
            state.selected_field = 0;
        }
    }
    if let Some(grid) = editor.draft.grids.get_mut(state.selected_grid) {
        ui.horizontal(|ui| {
            ui.label("Selected name");
            let changed = match grid {
                TabulatedGrid::Regular(value) => ui.text_edit_singleline(&mut value.name).changed(),
                TabulatedGrid::Irregular(value) => {
                    ui.text_edit_singleline(&mut value.name).changed()
                }
            };
            if changed {
                editor.dirty = true;
            }
        });
    }
    ui.horizontal(|ui| {
        if ui.button("Add regular draft grid").clicked() {
            match crate::regular_compositions(4) {
                Ok(compositions) => {
                    let grid = TabulatedGrid::Regular(crate::RegularTabulatedGrid {
                        name: format!("regular_grid_{}", editor.draft.grids.len() + 1),
                        source: crate::SourceRange {
                            first_line: 0,
                            last_line: 0,
                        },
                        subdivisions: 4,
                        order: crate::RowOrder::Canonical,
                        composition_columns: crate::CompositionColumns::None,
                        compositions,
                        fields: Vec::new(),
                    });
                    state.message = editor.add_grid(grid).err();
                }
                Err(error) => state.message = Some(error),
            }
        }
        if ui.button("Add irregular draft grid").clicked() {
            let grid = TabulatedGrid::Irregular(crate::IrregularTabulatedGrid {
                name: format!("irregular_grid_{}", editor.draft.grids.len() + 1),
                source: crate::SourceRange {
                    first_line: 0,
                    last_line: 0,
                },
                compositions: Vec::new(),
                fields: Vec::new(),
            });
            state.message = editor.add_grid(grid).err();
        }
        if ui.button("Duplicate grid").clicked()
            && let Some(mut grid) = editor.draft.grids.get(state.selected_grid).cloned()
        {
            match &mut grid {
                TabulatedGrid::Regular(value) => value.name.push_str("_copy"),
                TabulatedGrid::Irregular(value) => value.name.push_str("_copy"),
            }
            state.message = editor.add_grid(grid).err();
        }
        if ui.button("Remove grid").clicked() {
            state.confirm_remove = true;
        }
        if ui.button("Revert unapplied edits").clicked() {
            editor.revert();
        }
        if ui
            .add_enabled(
                editor.can_undo() || editor.can_draft_undo(),
                egui::Button::new("Undo"),
            )
            .clicked()
            && !editor.draft_undo()
        {
            editor.undo();
        }
        if ui
            .add_enabled(
                editor.can_redo() || editor.can_draft_redo(),
                egui::Button::new("Redo"),
            )
            .clicked()
            && !editor.draft_redo()
        {
            editor.redo();
        }
    });
    if state.confirm_remove {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::YELLOW, "Remove selected grid?");
            if ui.button("Confirm remove").clicked() {
                state.message = editor.remove_grid(state.selected_grid).err();
                state.selected_grid = state.selected_grid.saturating_sub(1);
                state.confirm_remove = false;
            }
            if ui.button("Cancel").clicked() {
                state.confirm_remove = false;
            }
        });
    }
}

fn regular_editor(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut DatasetEditorState,
    state: &mut DataEditorUi,
) -> DataEditorAction {
    let grid_index = state.selected_grid;
    let (current_subdivisions, current_rows, has_values) = match editor.draft.grids.get(grid_index)
    {
        Some(TabulatedGrid::Regular(grid)) => (
            grid.subdivisions,
            grid.compositions.len(),
            grid.fields
                .iter()
                .any(|field| field.values.iter().any(Option::is_some)),
        ),
        _ => return DataEditorAction::None,
    };
    if state.resolution_input.is_empty() {
        state.resolution_input = current_subdivisions.to_string();
    }
    ui.heading("Regular-grid editor");
    ui.horizontal(|ui| {
        ui.label("Resolution");
        ui.text_edit_singleline(&mut state.resolution_input);
        if let Ok(subdivisions) = state.resolution_input.trim().parse::<usize>() {
            match crate::regular_row_count(subdivisions) {
                Ok(points) if (1..=200).contains(&subdivisions) => {
                    ui.label(format!("Expected points: {points}"));
                    if ui.button("Apply resolution").clicked()
                        && subdivisions != current_subdivisions
                    {
                        if has_values {
                            state.resolution_pending = Some(subdivisions);
                        } else {
                            state.message = editor
                                .regenerate_regular_grid(grid_index, subdivisions)
                                .err();
                            state.resolution_input = subdivisions.to_string();
                        }
                    }
                }
                _ => {
                    ui.colored_label(
                        egui::Color32::RED,
                        "Enter an integer subdivision count from 1 to 200.",
                    );
                }
            }
        } else {
            ui.colored_label(
                egui::Color32::RED,
                "Subdivision count must be a whole number.",
            );
        }
    });
    ui.small(format!(
        "Current grid: {current_rows} points; expected formula (n + 1)(n + 2) / 2. Safety limit: 200."
    ));
    if let Some(subdivisions) = state.resolution_pending {
        let new_points = crate::regular_row_count(subdivisions).unwrap_or_default();
        ui.group(|ui| {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "Changing subdivisions from {current_subdivisions} to {subdivisions} changes the grid from {current_rows} to {new_points} points. Existing scalar values cannot be retained automatically."
                ),
            );
            ui.horizontal(|ui| {
                if ui.button("Regenerate and clear values").clicked() {
                    state.message = editor.regenerate_regular_grid(grid_index, subdivisions).err();
                    if state.message.is_none() {
                        state.resolution_input = subdivisions.to_string();
                    }
                    state.resolution_pending = None;
                }
                if ui.button("Cancel").clicked() {
                    state.resolution_pending = None;
                }
            });
        });
    }
    let TabulatedGrid::Regular(grid) = &editor.draft.grids[grid_index] else {
        return DataEditorAction::None;
    };
    field_choice(ui, editor, state);
    ui.horizontal(|ui| {
        if ui.button("Copy compositions").clicked() {
            copy_regular(ctx, editor, grid.subdivisions, false, state);
        }
        if ui.button("Copy compositions with header").clicked() {
            copy_regular(ctx, editor, grid.subdivisions, true, state);
        }
        if ui.button("Copy complete table").clicked() {
            ctx.copy_text(grid_tsv(&editor.draft, grid, true));
            state.message = Some("Copied complete regular table as TSV.".into());
        }
    });
    canonical_table(ui, editor, grid, state.selected_field);
    ui.separator();
    ui.label("Paste values, multiple scalar columns, or composition plus values");
    ui.add(
        egui::TextEdit::multiline(&mut state.regular_paste)
            .desired_rows(8)
            .hint_text("Paste TSV copied from Excel"),
    );
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.has_header, "header present");
        ui.checkbox(&mut state.blank_missing, "Treat blank cells as missing");
    });
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut state.regular_mode,
            RegularCompositionPasteMode::ValuesOnly,
            "Values only",
        );
        ui.selectable_value(
            &mut state.regular_mode,
            RegularCompositionPasteMode::Guidance,
            "Coordinates guidance",
        );
        ui.selectable_value(
            &mut state.regular_mode,
            RegularCompositionPasteMode::Authoritative,
            "Coordinates authoritative",
        );
    });
    replacement_choice(ui, state);
    if ui.button("Preview regular paste").clicked() {
        let mapping = regular_mapping(editor, grid, state);
        let preview = preview_regular_paste(&state.regular_paste, grid.subdivisions, &mapping);
        editor.set_preview(PastePreview::Regular(preview));
    }
    apply_buttons(ui, editor, state, true)
}

fn irregular_editor(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut DatasetEditorState,
    state: &mut DataEditorUi,
) -> DataEditorAction {
    let TabulatedGrid::Irregular(grid) = &editor.draft.grids[state.selected_grid] else {
        return DataEditorAction::None;
    };
    ui.heading("Irregular-grid editor");
    ui.label(format!(
        "{} source points, {} fields",
        grid.compositions.len(),
        grid.fields.len()
    ));
    field_choice(ui, editor, state);
    ui.horizontal(|ui| {
        if ui.button("Copy selected grid TSV").clicked() {
            ctx.copy_text(grid_tsv(&editor.draft, grid, true));
            state.message = Some("Copied irregular grid as TSV.".into());
        }
        ui.label("New grid name");
        ui.text_edit_singleline(&mut state.irregular_name);
    });
    ui.add(
        egui::TextEdit::multiline(&mut state.irregular_paste)
            .desired_rows(10)
            .hint_text("A B C scalar columns as TSV"),
    );
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.has_header, "header present");
        ui.checkbox(&mut state.blank_missing, "Treat blank cells as missing");
        ui.label("component columns");
        for column in &mut state.irregular_columns {
            ui.add(egui::DragValue::new(column).range(0..=100));
        }
    });
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut state.normalization,
            CompositionNormalization::RejectNonNormalized,
            "Reject non-normalized",
        );
        ui.selectable_value(
            &mut state.normalization,
            CompositionNormalization::NormalizeWithinTolerance,
            "Normalize within tolerance",
        );
        ui.selectable_value(
            &mut state.normalization,
            CompositionNormalization::NormalizeAllPositiveRows,
            "Normalize positive rows",
        );
    });
    replacement_choice(ui, state);
    if ui.button("Preview irregular paste").clicked() {
        let mapping = irregular_mapping(editor, state);
        editor.set_preview(PastePreview::Irregular(preview_irregular_paste(
            &state.irregular_paste,
            &mapping,
        )));
    }
    apply_buttons(ui, editor, state, false)
}

fn field_choice(ui: &mut egui::Ui, editor: &DatasetEditorState, state: &mut DataEditorUi) {
    let fields = editor.draft.grids[state.selected_grid].fields();
    state.selected_field = state.selected_field.min(fields.len().saturating_sub(1));
    if fields.is_empty() {
        ui.colored_label(egui::Color32::YELLOW, "No field exists in this grid. Add a field through a pasted table and choose Add new field.");
        return;
    }
    egui::ComboBox::from_label("Default paste destination")
        .selected_text(&fields[state.selected_field].column_name)
        .show_ui(ui, |ui| {
            for (index, field) in fields.iter().enumerate() {
                ui.selectable_value(&mut state.selected_field, index, &field.column_name);
            }
        });
}

fn replacement_choice(ui: &mut egui::Ui, state: &mut DataEditorUi) {
    egui::ComboBox::from_label("Apply semantics")
        .selected_text(match state.replacement {
            FieldReplacement::AddNewField => "Add new field",
            FieldReplacement::ReplaceExistingField => "Replace existing field",
            FieldReplacement::ReplaceEntireGrid => "Replace entire grid",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut state.replacement,
                FieldReplacement::AddNewField,
                "Add new field",
            );
            ui.selectable_value(
                &mut state.replacement,
                FieldReplacement::ReplaceExistingField,
                "Replace existing field",
            );
            ui.selectable_value(
                &mut state.replacement,
                FieldReplacement::ReplaceEntireGrid,
                "Replace entire grid",
            );
        });
}

fn apply_buttons(
    ui: &mut egui::Ui,
    editor: &mut DatasetEditorState,
    state: &mut DataEditorUi,
    regular: bool,
) -> DataEditorAction {
    let has_preview = editor.paste_preview.is_some();
    let mut action = DataEditorAction::None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(has_preview, egui::Button::new("Apply"))
            .clicked()
        {
            state.message = apply_preview(editor, state, regular).err();
        }
        if ui
            .add_enabled(has_preview, egui::Button::new("Apply and recalculate"))
            .clicked()
        {
            match apply_preview(editor, state, regular) {
                Ok(()) => action = DataEditorAction::Recalculate,
                Err(error) => state.message = Some(error),
            }
        }
    });
    action
}

fn apply_preview(
    editor: &mut DatasetEditorState,
    state: &DataEditorUi,
    regular: bool,
) -> Result<(), String> {
    let preview = editor
        .paste_preview
        .clone()
        .ok_or("create a paste preview first")?;
    if regular {
        let PastePreview::Regular(preview) = preview else {
            return Err("current preview is irregular".into());
        };
        editor.apply_regular_preview(state.selected_grid, &preview, state.replacement)?;
    } else {
        let PastePreview::Irregular(preview) = preview else {
            return Err("current preview is regular".into());
        };
        let selected = matches!(
            editor.draft.grids.get(state.selected_grid),
            Some(TabulatedGrid::Irregular(_))
        )
        .then_some(state.selected_grid);
        editor.apply_irregular_preview(
            selected,
            state.irregular_name.clone(),
            &preview,
            state.replacement,
        )?;
    }
    editor.apply_draft().map(|_| ())
}

fn regular_mapping(
    editor: &DatasetEditorState,
    grid: &crate::RegularTabulatedGrid,
    state: &DataEditorUi,
) -> RegularPasteMapping {
    let fields = automatic_fields(
        editor,
        &state.regular_paste,
        state.has_header,
        state.selected_grid,
        state.selected_field,
    );
    let composition_columns = match state.regular_mode {
        RegularCompositionPasteMode::ValuesOnly => None,
        _ => Some(
            component_columns(editor, &state.regular_paste, state.has_header).unwrap_or([0, 1, 2]),
        ),
    };
    let _ = grid;
    RegularPasteMapping {
        header_mode: if state.has_header {
            HeaderMode::Present
        } else {
            HeaderMode::Absent
        },
        composition_mode: state.regular_mode,
        composition_columns,
        fields,
        missing_tokens: editor.draft.missing_tokens.clone(),
        blank_cells_are_missing: state.blank_missing,
        coordinate_tolerance: 1.0e-8,
        allow_guidance_warnings: false,
    }
}

fn irregular_mapping(editor: &DatasetEditorState, state: &DataEditorUi) -> IrregularPasteMapping {
    IrregularPasteMapping {
        header_mode: if state.has_header {
            HeaderMode::Present
        } else {
            HeaderMode::Absent
        },
        composition_columns: component_columns(editor, &state.irregular_paste, state.has_header)
            .unwrap_or(state.irregular_columns),
        fields: automatic_fields(
            editor,
            &state.irregular_paste,
            state.has_header,
            state.selected_grid,
            state.selected_field,
        ),
        missing_tokens: editor.draft.missing_tokens.clone(),
        blank_cells_are_missing: state.blank_missing,
        coordinate_tolerance: 1.0e-8,
        normalization: state.normalization,
    }
}

fn automatic_fields(
    editor: &DatasetEditorState,
    text: &str,
    header: bool,
    grid_index: usize,
    selected_field: usize,
) -> Vec<FieldColumnMapping> {
    let grid = &editor.draft.grids[grid_index];
    let headers = header
        .then(|| {
            crate::ParsedTable::parse_tsv(text, HeaderMode::Present)
                .ok()
                .and_then(|table| table.headers)
        })
        .flatten();
    if let Some(headers) = headers {
        let mapped = headers
            .iter()
            .enumerate()
            .filter_map(|(column, header)| {
                if let Some((phase_name, property)) = header.split_once('.') {
                    let phase = editor
                        .draft
                        .phases
                        .iter()
                        .find(|phase| phase.name == phase_name)?;
                    editor
                        .draft
                        .properties
                        .iter()
                        .any(|candidate| candidate.name == property)
                        .then(|| FieldColumnMapping {
                            source_column: column,
                            destination: FieldKey {
                                phase_id: phase.id,
                                property: property.to_owned(),
                            },
                            label: header.clone(),
                        })
                } else {
                    grid.fields()
                        .iter()
                        .find(|field| header == &field.column_name)
                        .map(|field| FieldColumnMapping {
                            source_column: column,
                            destination: FieldKey {
                                phase_id: field.phase_id,
                                property: field.property.clone(),
                            },
                            label: field.column_name.clone(),
                        })
                }
            })
            .collect::<Vec<_>>();
        if !mapped.is_empty() {
            return mapped;
        }
    }
    grid.fields()
        .get(selected_field)
        .map(|field| FieldColumnMapping {
            source_column: 0,
            destination: FieldKey {
                phase_id: field.phase_id,
                property: field.property.clone(),
            },
            label: field.column_name.clone(),
        })
        .into_iter()
        .collect()
}
fn component_columns(editor: &DatasetEditorState, text: &str, header: bool) -> Option<[usize; 3]> {
    let headers = header.then(|| {
        crate::ParsedTable::parse_tsv(text, HeaderMode::Present)
            .ok()
            .and_then(|table| table.headers)
    })??;
    let indexes = editor
        .draft
        .components
        .each_ref()
        .map(|component| headers.iter().position(|header| header == &component.name));
    Some([indexes[0]?, indexes[1]?, indexes[2]?])
}

fn canonical_table(
    ui: &mut egui::Ui,
    editor: &DatasetEditorState,
    grid: &crate::RegularTabulatedGrid,
    selected_field: usize,
) {
    ui.label("Canonical row order (virtualized)");
    let field = grid.fields.get(selected_field);
    egui::ScrollArea::vertical().max_height(180.0).show_rows(
        ui,
        18.0,
        grid.compositions.len(),
        |ui, range| {
            for index in range {
                let point = grid.compositions[index];
                let value = field
                    .and_then(|field| field.values.get(index))
                    .copied()
                    .flatten();
                ui.horizontal(|ui| {
                    ui.monospace(format!("{:>5}", index + 1));
                    ui.monospace(format!("{:.6}\t{:.6}\t{:.6}", point[0], point[1], point[2]));
                    ui.monospace(value.map(|value| format!("{value:.6}")).unwrap_or_else(|| {
                        editor
                            .draft
                            .missing_tokens
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "NA".into())
                    }));
                });
            }
        },
    );
}

fn copy_regular(
    ctx: &egui::Context,
    editor: &DatasetEditorState,
    subdivisions: usize,
    header: bool,
    state: &mut DataEditorUi,
) {
    match compositions_tsv(
        subdivisions,
        [
            &editor.draft.components[0].name,
            &editor.draft.components[1].name,
            &editor.draft.components[2].name,
        ],
        header,
        NumericFormat::default(),
    ) {
        Ok(text) => {
            ctx.copy_text(text);
            state.message = Some("Copied canonical compositions as TSV.".into());
        }
        Err(error) => state.message = Some(format!("clipboard copy failed: {error}")),
    }
}

fn grid_tsv(
    dataset: &crate::TabulatedTernaryDataset,
    grid: &impl GridTable,
    header: bool,
) -> String {
    let fields = grid.fields();
    let mut lines = Vec::new();
    if header {
        let mut columns = dataset
            .components
            .iter()
            .map(|component| component.name.clone())
            .collect::<Vec<_>>();
        columns.extend(fields.iter().map(|field| {
            let phase = dataset
                .phases
                .iter()
                .find(|phase| phase.id == field.phase_id)
                .map(|phase| phase.name.as_str())
                .unwrap_or("unknown");
            format!("{phase}.{}", field.property)
        }));
        lines.push(columns.join("\t"));
    }
    for (row, point) in grid.compositions().iter().enumerate() {
        let mut cells = point.map(|value| format!("{value:.6}")).to_vec();
        cells.extend(fields.iter().map(|field| {
            field.values[row]
                .map(|value| format!("{value:.6}"))
                .unwrap_or_else(|| {
                    dataset
                        .missing_tokens
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "NA".into())
                })
        }));
        lines.push(cells.join("\t"));
    }
    lines.join("\n") + "\n"
}

trait GridTable {
    fn fields(&self) -> &[crate::TabulatedField];
    fn compositions(&self) -> &[[f64; 3]];
}

impl GridTable for crate::RegularTabulatedGrid {
    fn fields(&self) -> &[crate::TabulatedField] {
        &self.fields
    }
    fn compositions(&self) -> &[[f64; 3]] {
        &self.compositions
    }
}

impl GridTable for crate::IrregularTabulatedGrid {
    fn fields(&self) -> &[crate::TabulatedField] {
        &self.fields
    }
    fn compositions(&self) -> &[[f64; 3]] {
        &self.compositions
    }
}

fn preview_panel(ctx: &egui::Context, ui: &mut egui::Ui, editor: &DatasetEditorState) {
    ui.collapsing("Paste preview and validation", |ui| {
        let Some(preview) = &editor.paste_preview else {
            ui.small(
                "Paste is parsed into a preview; the active dataset is unchanged until Apply.",
            );
            return;
        };
        match preview {
            PastePreview::Regular(value) => ui.label(format!(
                "Regular: pasted {} rows, expected {}; {} scalar fields",
                value.pasted_rows,
                value.canonical_compositions.len(),
                value.fields.len()
            )),
            PastePreview::Irregular(value) => ui.label(format!(
                "Irregular: pasted {} rows, {} accepted points; {} scalar fields",
                value.pasted_rows,
                value.compositions.len(),
                value.fields.len()
            )),
        };
        ui.label(editor.validation.as_text());
        if ui.button("Copy validation report").clicked() {
            ctx.copy_text(editor.validation.as_text());
        }
        for issue in editor.validation.errors.iter().take(8) {
            ui.colored_label(egui::Color32::RED, issue.to_string());
        }
        let omitted = editor.validation.errors.len().saturating_sub(8);
        if omitted > 0 {
            ui.small(format!("{omitted} additional errors omitted"));
        }
    });
}
