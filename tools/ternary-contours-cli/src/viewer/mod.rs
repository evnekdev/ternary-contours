//! Native interactive viewer for manual liquidus inspection.

mod app;
pub mod contract;
mod controls;
mod data_editor;
mod grid_inspection;
mod hit_test;
mod interpolation_inspection;
mod save;
mod state;
mod texture;

use std::{error::Error, path::PathBuf};

use crate::{ProjectionOptions, RenderOptions, default_regular_dataset};

pub use app::LiquidusViewerApp;
pub use contract::{EventTrace, GuiContractState, UiAction, UiEffect, UiElementId, ViewerTab};
pub use hit_test::{SelectedFeature, ViewerTransform};
pub use save::{
    default_dialog_directory, default_export_directory, default_filename,
    default_projection_filename, ensure_extension, ensure_tct_extension, sanitize_title,
    save_requires_dialog,
};
pub use state::{CalculationInput, DirtyFlags, PathDisplayMode, ViewerState, ViewerStatus};

pub fn launch(
    input_path: Option<PathBuf>,
    calculation_options: ProjectionOptions,
    render_options: RenderOptions,
) -> Result<(), Box<dyn Error>> {
    let title = render_options.title.clone().unwrap_or_else(|| {
        if input_path.is_none() {
            "Untitled — unsaved".into()
        } else {
            "Ternary contours liquidus viewer".into()
        }
    });
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([render_options.width as f32, render_options.height as f32])
            .with_min_inner_size([640.0, 480.0])
            .with_title(title.clone()),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        &title,
        native_options,
        Box::new(move |_| {
            Ok(Box::new(match input_path {
                Some(input_path) => {
                    LiquidusViewerApp::new(input_path, calculation_options, render_options)
                }
                None => LiquidusViewerApp::new_default(
                    calculation_options,
                    render_options,
                    default_regular_dataset(),
                ),
            }))
        }),
    )?;
    Ok(())
}
