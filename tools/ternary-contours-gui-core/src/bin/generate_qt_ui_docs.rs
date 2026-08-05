use std::{env, fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/qt/ui-object-inventory.md");
    let generated = ternary_contours_gui_core::qt_ui_inventory_markdown();
    if env::args().skip(1).any(|argument| argument == "--check") {
        match fs::read_to_string(&path) {
            Ok(checked_in) if checked_in == generated => ExitCode::SUCCESS,
            Ok(_) => {
                eprintln!("generated Qt UI inventory is stale: {}", path.display());
                ExitCode::FAILURE
            }
            Err(error) => {
                eprintln!("cannot read {}: {error}", path.display());
                ExitCode::FAILURE
            }
        }
    } else {
        match fs::write(&path, generated) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("cannot write {}: {error}", path.display());
                ExitCode::FAILURE
            }
        }
    }
}
