use eframe::egui;

use crate::{RenderPathMode, parse_level_spec};

use super::state::{PathDisplayMode, ViewerState};

pub fn show(ui: &mut egui::Ui, state: &mut ViewerState) -> bool {
    let mut apply = false;
    ui.heading("Input");
    ui.label(state.input_path.display().to_string());
    if let Some(dataset) = &state.dataset {
        ui.small(format!(
            "{} phases, {} grids, {:.3} to {:.3}",
            dataset.phases.len(),
            dataset.grids.len(),
            state
                .active_projection()
                .map(|projection| projection.input_summary.temperature_range[0])
                .unwrap_or_default(),
            state
                .active_projection()
                .map(|projection| projection.input_summary.temperature_range[1])
                .unwrap_or_default()
        ));
    }
    if let Some(loaded) = state.last_successful_reload
        && let Ok(elapsed) = loaded.elapsed()
    {
        ui.small(format!(
            "last successful reload: {} s ago",
            elapsed.as_secs()
        ));
    }

    ui.separator();
    ui.heading("Levels");
    ui.label("Comma list or start:stop:step");
    ui.text_edit_singleline(&mut state.viewer_options.level_text);
    match preview_levels(&state.viewer_options.level_text) {
        Ok(levels) if levels.is_empty() => {
            ui.small("Default levels will be generated from the data.")
        }
        Ok(levels) => ui.small(format!(
            "{} levels: {}",
            levels.len(),
            format_levels(&levels)
        )),
        Err(message) => ui.colored_label(egui::Color32::RED, message),
    };

    ui.separator();
    ui.heading("Calculation");
    let mut subdivisions = state
        .calculation_options
        .sampling_subdivisions
        .unwrap_or(24);
    if ui
        .add(
            egui::DragValue::new(&mut subdivisions)
                .range(2..=2_000)
                .prefix("sampling: "),
        )
        .changed()
    {
        state.calculation_options.sampling_subdivisions = Some(subdivisions);
        state.invalidate_projection();
    }
    if ui
        .checkbox(
            &mut state.calculation_options.regularize,
            "Regularize paths",
        )
        .changed()
    {
        state.invalidate_projection();
    }
    ui.add_enabled_ui(state.calculation_options.regularize, |ui| {
        if ui
            .add(
                egui::DragValue::new(&mut state.viewer_options.regularization_spacing)
                    .range(0.000_1..=1.0)
                    .speed(0.001)
                    .prefix("spacing: "),
            )
            .changed()
        {
            state.calculation_options.regularization_spacing =
                Some(state.viewer_options.regularization_spacing);
            state.invalidate_projection();
        }
    });
    if state.dirty.projection {
        ui.colored_label(egui::Color32::YELLOW, "Calculation settings changed.");
    }
    if ui.button("Apply / recalculate").clicked() {
        apply = true;
    }

    ui.separator();
    ui.heading("Layers");
    let mut render_changed = false;
    render_changed |= ui
        .checkbox(&mut state.render_options.show_isotherms, "Stable isotherms")
        .changed();
    render_changed |= ui
        .checkbox(
            &mut state.render_options.show_univariants,
            "Stable univariants",
        )
        .changed();
    render_changed |= ui
        .checkbox(
            &mut state.render_options.show_binary_invariants,
            "Binary invariants",
        )
        .changed();
    render_changed |= ui
        .checkbox(
            &mut state.render_options.show_invariants,
            "Interior invariants",
        )
        .changed();
    render_changed |= ui
        .checkbox(&mut state.render_options.show_grid, "Sampling grid points")
        .changed();
    render_changed |= ui
        .checkbox(
            &mut state.render_options.show_samples,
            "Source sample points",
        )
        .changed();
    render_changed |= ui
        .checkbox(&mut state.render_options.show_labels, "Axis labels")
        .changed();
    render_changed |= ui
        .checkbox(&mut state.render_options.show_legend, "Legend")
        .changed();
    if render_changed {
        state.mark_render_dirty();
    }

    ui.separator();
    ui.heading("Paths");
    let old_mode = state.viewer_options.path_display;
    egui::ComboBox::from_label("Display")
        .selected_text(path_mode_name(state.viewer_options.path_display))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut state.viewer_options.path_display,
                PathDisplayMode::Raw,
                "Raw only",
            );
            ui.selectable_value(
                &mut state.viewer_options.path_display,
                PathDisplayMode::Regularized,
                "Regularized only",
            );
            ui.selectable_value(
                &mut state.viewer_options.path_display,
                PathDisplayMode::Overlay,
                "Overlay",
            );
        });
    if old_mode != state.viewer_options.path_display {
        state.render_options.path_mode = render_path_mode(state.viewer_options.path_display);
        state.mark_render_dirty();
    }

    ui.collapsing("Appearance", |ui| {
        let mut changed = false;
        changed |= ui
            .add(egui::DragValue::new(&mut state.render_options.line_width).range(1..=12))
            .changed();
        ui.label("line width");
        changed |= ui
            .add(egui::DragValue::new(&mut state.render_options.marker_size).range(2..=30))
            .changed();
        ui.label("marker size");
        if changed {
            state.mark_render_dirty();
        }
    });

    ui.collapsing("Diagnostics", |ui| {
        let mut changed = false;
        changed |= ui
            .checkbox(
                &mut state.viewer_options.show_path_vertices,
                "Path vertices",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut state.viewer_options.show_contour_endpoints,
                "Contour endpoints",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut state.viewer_options.show_univariant_endpoints,
                "Univariant endpoints",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut state.viewer_options.show_invariant_ids,
                "Invariant node IDs",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut state.viewer_options.show_univariant_ids,
                "Univariant IDs",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut state.viewer_options.show_phase_pair_labels,
                "Phase-pair labels",
            )
            .changed();
        if changed {
            state.mark_render_dirty();
        }
        ui.small("Sampling-grid edge diagnostics are unavailable for irregular source grids.");
    });
    apply
}

pub fn apply_calculation_options(state: &mut ViewerState) -> Result<(), String> {
    let levels = preview_levels(&state.viewer_options.level_text)?;
    state.calculation_options.levels = levels;
    state.calculation_options.regularization_spacing =
        Some(state.viewer_options.regularization_spacing);
    state.invalidate_projection();
    Ok(())
}

fn preview_levels(text: &str) -> Result<Vec<f64>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Ok(Vec::new())
    } else {
        parse_level_spec(trimmed).map_err(|error| error.to_string())
    }
}

fn format_levels(levels: &[f64]) -> String {
    let displayed = levels
        .iter()
        .take(6)
        .map(|level| format!("{level:.3}"))
        .collect::<Vec<_>>()
        .join(", ");
    if levels.len() > 6 {
        format!("{displayed}, …")
    } else {
        displayed
    }
}

fn path_mode_name(mode: PathDisplayMode) -> &'static str {
    match mode {
        PathDisplayMode::Raw => "Raw only",
        PathDisplayMode::Regularized => "Regularized only",
        PathDisplayMode::Overlay => "Overlay",
    }
}

fn render_path_mode(mode: PathDisplayMode) -> RenderPathMode {
    match mode {
        PathDisplayMode::Raw => RenderPathMode::Raw,
        PathDisplayMode::Regularized => RenderPathMode::Regularized,
        PathDisplayMode::Overlay => RenderPathMode::Overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_preview_accepts_ranges_without_calculation() {
        assert_eq!(
            preview_levels("800:900:50").unwrap(),
            vec![800.0, 850.0, 900.0]
        );
    }
}
