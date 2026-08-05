//! Regenerate or verify checked-in GUI contract documentation.

use std::{env, fs, path::PathBuf, process::ExitCode};

use ternary_contours_cli::viewer::contract::generated_documentation;

fn main() -> ExitCode {
    let check = env::args().skip(1).any(|argument| argument == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/gui");
    let mut stale = Vec::new();
    for (name, generated) in generated_documentation() {
        let path = root.join(name);
        if check {
            match fs::read_to_string(&path) {
                Ok(existing) if existing == generated => {}
                _ => stale.push(path),
            }
        } else if let Err(error) = fs::write(&path, generated) {
            eprintln!("could not write {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    }
    if stale.is_empty() {
        ExitCode::SUCCESS
    } else {
        for path in stale {
            eprintln!("GUI contract documentation is stale: {}", path.display());
        }
        eprintln!(
            "run cargo run -p ternary-contours-cli --features viewer --bin generate-gui-contract-docs"
        );
        ExitCode::FAILURE
    }
}
