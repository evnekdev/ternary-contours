use std::{env, fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/gui");
    let check = env::args().skip(1).any(|argument| argument == "--check");
    let files = ternary_contours_gui_core::generated_documentation();
    for (name, generated) in files {
        let path = directory.join(name);
        if check {
            match fs::read_to_string(&path) {
                Ok(checked_in) if checked_in == generated => {}
                Ok(_) => {
                    eprintln!("generated GUI document is stale: {}", path.display());
                    return ExitCode::FAILURE;
                }
                Err(error) => {
                    eprintln!("cannot read {}: {error}", path.display());
                    return ExitCode::FAILURE;
                }
            }
        } else if let Err(error) = fs::write(&path, generated) {
            eprintln!("cannot write {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
