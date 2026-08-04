use crate::{RenderPathMode, SourceInterpolation, parse_level_spec};
use eframe::egui;
use ternary_contours::{BinaryExtrapolation, CubicAlphaMethod};

use super::state::{PathDisplayMode, ViewerState, ViewerStatus};

/// UI changes classified by their invalidation cost.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControlChange {
    /// A committed numerical setting changed and should be recalculated.
    pub calculation_changed: bool,
    /// The explicit recovery command was requested.
    pub recalculate_now: bool,
}

pub fn show(ui: &mut egui::Ui, state: &mut ViewerState) -> ControlChange {
    let mut change = ControlChange::default();
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

    if let Some(projection) = state.active_projection() {
        let binary = projection
            .stable_boundaries
            .nodes
            .iter()
            .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Binary(_)))
            .count();
        let interior = projection
            .stable_boundaries
            .nodes
            .iter()
            .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Interior(_)))
            .count();
        ui.small(format!(
            "projection: {} stable polygons, {} isotherm paths, {} univariants, {} binary + {} interior invariants",
            projection.diagnostics.stable_polygon_count,
            projection.diagnostics.contour_path_count,
            projection.diagnostics.univariant_count,
            binary,
            interior
        ));
    }

    ui.separator();
    ui.heading("Levels");
    ui.label("Comma list or start:stop:step");
    let response = ui.text_edit_singleline(&mut state.viewer_options.level_text);
    if edit_cancelled(ui, &response) {
        state.viewer_options.level_text = state
            .calculation_options
            .levels
            .iter()
            .map(f64::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        state.viewer_options.level_error = None;
    } else if edit_finished(ui, &response) {
        match preview_levels(&state.viewer_options.level_text) {
            Ok(levels) => {
                state.calculation_options.levels = levels;
                state.viewer_options.level_error = None;
                state.invalidate_projection();
                change.calculation_changed = true;
            }
            Err(error) => state.viewer_options.level_error = Some(error),
        }
    }
    match preview_levels(&state.viewer_options.level_text) {
        Ok(levels) if levels.is_empty() => {
            ui.small("Default levels will be generated from the data.")
        }
        Ok(levels) => ui.small(format!(
            "{} levels: {}",
            levels.len(),
            format_levels(&levels)
        )),
        Err(error) => ui.colored_label(egui::Color32::RED, error),
    };
    if let Some(error) = &state.viewer_options.level_error {
        ui.colored_label(egui::Color32::RED, error);
    }

    ui.separator();
    ui.heading("Calculation");
    ui.label("Source interpolation");
    let old_interpolation = state.calculation_options.source_interpolation;
    let mut interpolation = old_interpolation;
    let cubic_supported = cubic_source_supported(state);
    egui::ComboBox::from_id_salt("source_interpolation")
        .selected_text(interpolation_name(interpolation))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut interpolation, SourceInterpolation::Linear, "Linear");
            let cubic_option = cubic_default_from(interpolation);
            ui.add_enabled_ui(cubic_supported, |ui| {
                ui.selectable_value(&mut interpolation, cubic_option, "Cubic alpha");
            });
        });
    if !cubic_supported {
        ui.small(
            "Cubic alpha is unavailable while a participating field uses an irregular grid; use Linear Delaunay.",
        );
    }
    if old_interpolation != interpolation {
        state.calculation_options.source_interpolation = interpolation;
        state.invalidate_projection();
        change.calculation_changed = true;
    }

    if let SourceInterpolation::CubicAlpha {
        mut method,
        mut continuation,
    } = state.calculation_options.source_interpolation
    {
        ui.label("Cubic slope estimation");
        let old_method = method;
        egui::ComboBox::from_id_salt("cubic_slope_method")
            .selected_text(cubic_method_name(method))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut method, CubicAlphaMethod::Akima, "Akima");
                ui.selectable_value(&mut method, CubicAlphaMethod::Makima, "Makima");
                ui.selectable_value(&mut method, CubicAlphaMethod::Pchip, "PCHIP");
                ui.selectable_value(&mut method, CubicAlphaMethod::Steffen, "Steffen");
            });
        ui.small("These are one-dimensional edge-slope estimators used by the ternary cubic-alpha model.");
        ui.label("Continuation outside the local derivative stencil");
        let old_continuation = continuation;
        egui::ComboBox::from_id_salt("cubic_continuation")
            .selected_text(continuation_name(continuation))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut continuation,
                    BinaryExtrapolation::RawBarycentric,
                    "Raw barycentric",
                );
                ui.selectable_value(&mut continuation, BinaryExtrapolation::Muggianu, "Muggianu");
                ui.selectable_value(&mut continuation, BinaryExtrapolation::Kohler, "Kohler");
            });
        if old_method != method || old_continuation != continuation {
            state.calculation_options.source_interpolation = SourceInterpolation::CubicAlpha {
                method,
                continuation,
            };
            state.invalidate_projection();
            change.calculation_changed = true;
        }
    } else {
        ui.add_enabled_ui(false, |ui| {
            ui.label("Cubic slope estimation");
            ui.label("Continuation outside the local derivative stencil");
        });
    }

    ui.horizontal(|ui| {
        ui.label("Sampling subdivisions");
        let response = ui.text_edit_singleline(&mut state.viewer_options.sampling_text);
        if edit_cancelled(ui, &response) {
            state.viewer_options.sampling_text = state
                .calculation_options
                .sampling_subdivisions
                .unwrap_or(24)
                .to_string();
            state.viewer_options.sampling_error = None;
        } else if edit_finished(ui, &response) {
            match parse_sampling(&state.viewer_options.sampling_text) {
                Ok(subdivisions) => {
                    state.calculation_options.sampling_subdivisions = Some(subdivisions);
                    state.viewer_options.sampling_error = None;
                    state.invalidate_projection();
                    change.calculation_changed = true;
                }
                Err(error) => state.viewer_options.sampling_error = Some(error),
            }
        }
    });
    if let Some(error) = &state.viewer_options.sampling_error {
        ui.colored_label(egui::Color32::RED, error);
    }
    ui.small(
        "Sampling resolution refines envelope/path extraction. Linear interpolation remains piecewise planar between source vertices; use cubic alpha for a smoother source model.",
    );

    if ui
        .checkbox(
            &mut state.calculation_options.regularize,
            "Regularize paths",
        )
        .changed()
    {
        state.invalidate_projection();
        change.calculation_changed = true;
    }
    ui.add_enabled_ui(state.calculation_options.regularize, |ui| {
        ui.horizontal(|ui| {
            ui.label("Regularization spacing");
            let response =
                ui.text_edit_singleline(&mut state.viewer_options.regularization_spacing_text);
            if edit_cancelled(ui, &response) {
                state.viewer_options.regularization_spacing_text = state
                    .calculation_options
                    .regularization_spacing
                    .unwrap_or(0.02)
                    .to_string();
                state.viewer_options.regularization_spacing_error = None;
            } else if edit_finished(ui, &response) {
                match parse_positive_spacing(&state.viewer_options.regularization_spacing_text) {
                    Ok(spacing) => {
                        state.viewer_options.regularization_spacing = spacing;
                        state.calculation_options.regularization_spacing = Some(spacing);
                        state.viewer_options.regularization_spacing_error = None;
                        state.invalidate_projection();
                        change.calculation_changed = true;
                    }
                    Err(error) => state.viewer_options.regularization_spacing_error = Some(error),
                }
            }
        });
    });
    if let Some(error) = &state.viewer_options.regularization_spacing_error {
        ui.colored_label(egui::Color32::RED, error);
    }
    if state.dirty.projection {
        ui.colored_label(
            egui::Color32::YELLOW,
            "Recalculation will start automatically.",
        );
    }
    if ui
        .add_enabled(
            !matches!(state.status, ViewerStatus::Calculating),
            egui::Button::new("Recalculate now"),
        )
        .clicked()
    {
        if preview_levels(&state.viewer_options.level_text).is_ok()
            && state.viewer_options.sampling_error.is_none()
            && state.viewer_options.regularization_spacing_error.is_none()
        {
            change.recalculate_now = true;
        } else {
            state.viewer_options.level_error =
                Some("Fix the highlighted calculation input before recalculating.".into());
        }
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
        .checkbox(
            &mut state.render_options.show_corner_labels,
            "Component names at corners",
        )
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
        ui.checkbox(
            &mut state.viewer_options.show_path_vertices,
            "Path vertices (raw yellow; regularized cyan)",
        );
        ui.checkbox(
            &mut state.viewer_options.show_contour_endpoints,
            "Contour endpoints",
        );
        ui.checkbox(
            &mut state.viewer_options.show_univariant_endpoints,
            "Univariant endpoints",
        );
        ui.checkbox(
            &mut state.viewer_options.show_invariant_ids,
            "Invariant node IDs",
        );
        ui.checkbox(
            &mut state.viewer_options.show_univariant_ids,
            "Univariant IDs",
        );
        ui.checkbox(
            &mut state.viewer_options.show_phase_pair_labels,
            "Phase-pair labels",
        );
        ui.small(
            "Diagnostic visibility is view-only and never recalculates or redraws the bitmap.",
        );
    });
    change
}

fn cubic_default_from(interpolation: SourceInterpolation) -> SourceInterpolation {
    match interpolation {
        cubic @ SourceInterpolation::CubicAlpha { .. } => cubic,
        SourceInterpolation::Linear => SourceInterpolation::CubicAlpha {
            method: CubicAlphaMethod::Makima,
            continuation: BinaryExtrapolation::Muggianu,
        },
    }
}

fn cubic_source_supported(state: &ViewerState) -> bool {
    state.dataset.as_ref().is_none_or(|dataset| {
        dataset.grids.iter().all(|grid| {
            grid.fields()
                .iter()
                .filter(|field| field.property == "T")
                .all(|_| matches!(grid, crate::TabulatedGrid::Regular(_)))
        })
    })
}

fn interpolation_name(interpolation: SourceInterpolation) -> &'static str {
    match interpolation {
        SourceInterpolation::Linear => "Linear",
        SourceInterpolation::CubicAlpha { .. } => "Cubic alpha",
    }
}

fn cubic_method_name(method: CubicAlphaMethod) -> &'static str {
    match method {
        CubicAlphaMethod::Akima => "Akima",
        CubicAlphaMethod::Makima => "Makima",
        CubicAlphaMethod::Pchip => "PCHIP",
        CubicAlphaMethod::Steffen => "Steffen",
        _ => "Unsupported",
    }
}

fn continuation_name(continuation: BinaryExtrapolation) -> &'static str {
    match continuation {
        BinaryExtrapolation::RawBarycentric => "Raw barycentric",
        BinaryExtrapolation::Muggianu => "Muggianu",
        BinaryExtrapolation::Kohler => "Kohler",
        _ => "Unsupported",
    }
}

fn edit_finished(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.lost_focus()
        || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
}

fn edit_cancelled(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape))
}

fn parse_sampling(text: &str) -> Result<usize, String> {
    let value = text
        .trim()
        .parse::<usize>()
        .map_err(|_| "sampling subdivisions must be a whole number".to_owned())?;
    (2..=2_000)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| "sampling subdivisions must be between 2 and 2000".into())
}

fn parse_positive_spacing(text: &str) -> Result<f64, String> {
    let value = text
        .trim()
        .parse::<f64>()
        .map_err(|_| "regularization spacing must be a finite positive number".to_owned())?;
    (value.is_finite() && value > 0.0 && value <= 1.0)
        .then_some(value)
        .ok_or_else(|| "regularization spacing must be in (0, 1]".into())
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
        format!("{displayed}, ?")
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

    #[test]
    fn text_inputs_only_accept_valid_committed_values() {
        assert_eq!(parse_sampling("30"), Ok(30));
        assert!(parse_sampling("3.0").is_err());
        assert!(parse_sampling("-3").is_err());
        assert_eq!(parse_positive_spacing("0.025"), Ok(0.025));
        assert!(parse_positive_spacing("0").is_err());
    }

    #[test]
    fn cubic_continuation_is_separate_from_interpolation_family() {
        let interpolation = SourceInterpolation::CubicAlpha {
            method: CubicAlphaMethod::Makima,
            continuation: BinaryExtrapolation::Kohler,
        };
        let options = interpolation.cubic_options().unwrap();
        assert_eq!(options.method, CubicAlphaMethod::Makima);
        assert_eq!(options.extrapolation, BinaryExtrapolation::Kohler);
    }
}
