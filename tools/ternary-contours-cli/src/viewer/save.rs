use std::{
    env,
    path::{Path, PathBuf},
};

/// Return the directory to use when opening a native file dialog.
///
/// The caller stores only directories selected successfully by a dialog. The
/// current document is used next, followed by the process working directory.
pub fn default_dialog_directory(
    last_dialog_directory: Option<&Path>,
    document_path: Option<&Path>,
) -> PathBuf {
    if let Some(path) = last_dialog_directory.filter(|path| !path.as_os_str().is_empty()) {
        return path.to_path_buf();
    }
    if let Some(path) = document_path.and_then(Path::parent)
        && !path.as_os_str().is_empty()
    {
        return path.to_path_buf();
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Export dialogs prefer the last successful export, then the current
/// document, then the last Open/Save directory, and finally the working
/// directory. This avoids surprising repository-relative output paths.
pub fn default_export_directory(
    last_export_directory: Option<&Path>,
    document_path: Option<&Path>,
    last_dialog_directory: Option<&Path>,
) -> PathBuf {
    last_export_directory
        .filter(|path| !path.as_os_str().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            document_path
                .and_then(Path::parent)
                .filter(|path| !path.as_os_str().is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| default_dialog_directory(last_dialog_directory, None))
}

/// Convert a dataset title into a deterministic, portable filename stem.
pub fn sanitize_title(title: Option<&str>) -> Option<String> {
    let title = title?.trim();
    if title.is_empty() {
        return None;
    }
    let mut output = String::new();
    for character in title.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            output.push(character);
        } else if !output.ends_with('-') {
            output.push('-');
        }
    }
    let output = output.trim_matches('-');
    (!output.is_empty()).then(|| output.to_owned())
}

/// Generate the filename shown by Save As before the user selects a path.
pub fn default_filename(
    existing_document: Option<&Path>,
    unsaved: bool,
    title: Option<&str>,
) -> String {
    if !unsaved
        && let Some(name) = existing_document
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
        && let Ok(path) = ensure_tct_extension(PathBuf::from(name))
    {
        return path.display().to_string();
    }
    if let Some(title) = sanitize_title(title) {
        return format!("{title}.tct");
    }
    "untitled-ternary-system.tct".into()
}

/// Generate a deterministic projection-export filename for a native dialog.
pub fn default_projection_filename(
    existing_document: Option<&Path>,
    title: Option<&str>,
    suffix: &str,
    extension: &str,
) -> String {
    if let Some(stem) = existing_document
        .and_then(Path::file_stem)
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
    {
        return format!("{stem}-{suffix}.{extension}");
    }
    if let Some(title) = sanitize_title(title) {
        return format!("{title}-{suffix}.{extension}");
    }
    format!("ternary-{suffix}.{extension}")
}

/// Whether a save request needs to show Save As instead of writing directly.
pub const fn save_requires_dialog(unsaved: bool, save_as: bool) -> bool {
    unsaved || save_as
}

/// Add the expected extension when omitted and reject incompatible extensions.
pub fn ensure_extension(path: PathBuf, expected_extension: &str) -> Result<PathBuf, String> {
    match path.extension().and_then(|extension| extension.to_str()) {
        None => Ok(path.with_extension(expected_extension)),
        Some(extension) if extension.eq_ignore_ascii_case(expected_extension) => Ok(path),
        Some(extension) => Err(format!(
            "incompatible file extension .{extension}; choose a .{expected_extension} file"
        )),
    }
}

/// Add .tct when omitted and reject incompatible extensions.
pub fn ensure_tct_extension(path: PathBuf) -> Result<PathBuf, String> {
    ensure_extension(path, "tct")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_priority_is_last_then_document_then_working_directory() {
        assert_eq!(
            default_dialog_directory(
                Some(Path::new("D:/last")),
                Some(Path::new("D:/document/data.tct"))
            ),
            PathBuf::from("D:/last")
        );
        assert_eq!(
            default_dialog_directory(None, Some(Path::new("D:/document/data.tct"))),
            PathBuf::from("D:/document")
        );
        assert_eq!(
            default_dialog_directory(None, None),
            env::current_dir().unwrap()
        );
    }

    #[test]
    fn titles_are_sanitized_and_default_names_are_deterministic() {
        assert_eq!(
            sanitize_title(Some("CaO-PbO-ZnO liquidus projection")),
            Some("CaO-PbO-ZnO-liquidus-projection".into())
        );
        assert_eq!(sanitize_title(Some(" !!! ")), None);
        assert_eq!(
            default_filename(None, true, Some("CaO-PbO-ZnO liquidus projection")),
            "CaO-PbO-ZnO-liquidus-projection.tct"
        );
        assert_eq!(
            default_filename(Some(Path::new("data.tct")), false, Some("Other")),
            "data.tct"
        );
        assert_eq!(
            default_filename(None, true, None),
            "untitled-ternary-system.tct"
        );
    }

    #[test]
    fn save_request_dialog_policy_distinguishes_untitled_and_save_as() {
        assert!(!save_requires_dialog(false, false));
        assert!(save_requires_dialog(false, true));
        assert!(save_requires_dialog(true, false));
    }

    #[test]
    fn extension_policy_adds_tct_and_rejects_other_extensions() {
        assert_eq!(
            ensure_tct_extension(PathBuf::from("data")),
            Ok(PathBuf::from("data.tct"))
        );
        assert_eq!(
            ensure_tct_extension(PathBuf::from("data.TCT")),
            Ok(PathBuf::from("data.TCT"))
        );
        assert!(ensure_tct_extension(PathBuf::from("data.csv")).is_err());
    }

    #[test]
    fn export_directory_filename_and_extension_follow_the_document_policy() {
        assert_eq!(
            default_export_directory(
                Some(Path::new("D:/exports")),
                Some(Path::new("D:/document/data.tct")),
                Some(Path::new("D:/saved")),
            ),
            PathBuf::from("D:/exports")
        );
        assert_eq!(
            default_export_directory(None, Some(Path::new("D:/document/data.tct")), None),
            PathBuf::from("D:/document")
        );
        assert_eq!(
            default_projection_filename(
                Some(Path::new("CaO-PbO-ZnO.tct")),
                Some("Other"),
                "projection",
                "svg",
            ),
            "CaO-PbO-ZnO-projection.svg"
        );
        assert_eq!(
            default_projection_filename(None, None, "lines", "csv"),
            "ternary-lines.csv"
        );
        assert_eq!(
            ensure_extension(PathBuf::from("image"), "png"),
            Ok(PathBuf::from("image.png"))
        );
        assert!(ensure_extension(PathBuf::from("image.svg"), "png").is_err());
    }
}
