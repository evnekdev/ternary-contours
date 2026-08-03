//! Feature-gated native viewer entry point.

use std::{error::Error, path::PathBuf};

use crate::{ProjectionOptions, RenderOptions};

pub fn launch(
    _input_path: PathBuf,
    _calculation_options: ProjectionOptions,
    _render_options: RenderOptions,
) -> Result<(), Box<dyn Error>> {
    Err("the native viewer is not available in this build".into())
}
