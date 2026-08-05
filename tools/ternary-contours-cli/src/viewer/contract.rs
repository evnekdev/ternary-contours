//! Legacy egui widget adapters for the framework-neutral GUI contract.
//!
//! All state, action, effect, registry, and documentation code lives in
//! `ternary-contours-gui-core`. This module is intentionally limited to egui
//! identity wrappers required while the legacy viewer remains supported.

pub use ternary_contours_gui_core::*;

use eframe::egui;

pub fn button(
    ui: &mut egui::Ui,
    id: UiElementId,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.push_id(id, |ui| ui.button(label)).inner
}

pub fn checkbox(
    ui: &mut egui::Ui,
    id: UiElementId,
    value: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.push_id(id, |ui| ui.checkbox(value, label)).inner
}

pub fn text_edit_singleline(
    ui: &mut egui::Ui,
    id: UiElementId,
    value: &mut String,
) -> egui::Response {
    ui.push_id(id, |ui| ui.text_edit_singleline(value)).inner
}

pub fn selectable_value<T: PartialEq>(
    ui: &mut egui::Ui,
    id: UiElementId,
    current: &mut T,
    value: T,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.push_id(id, |ui| ui.selectable_value(current, value, label))
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_risk_toolbar_paths_use_contract_wrappers() {
        let app = include_str!("app.rs");
        assert!(app.contains("contract::button(ui, UiElementId::"));
        for label in [
            "Open",
            "Save",
            "Save As",
            "Export SVG",
            "Export PNG",
            "Export lines CSV",
        ] {
            assert!(
                !app.contains(&format!("ui.button(\"{label}\")")),
                "{label} bypasses the GUI contract"
            );
        }
        assert!(
            app.contains("self.dispatch_contract(ctx, UiElementId::Open, UiAction::OpenRequested)")
        );
        assert!(app.contains("UiAction::CalculationSettingsCommitted"));
    }

    #[test]
    fn every_effect_kind_has_a_viewer_executor_arm() {
        let app = include_str!("app.rs");
        for effect in UiEffectKind::ALL {
            assert!(
                app.contains(&format!("UiEffect::{effect:?}")),
                "legacy viewer lacks executor for {effect:?}"
            );
        }
    }
}
