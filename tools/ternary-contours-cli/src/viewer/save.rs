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

/// Whether a save request needs to show Save As instead of writing directly.
pub const fn save_requires_dialog(unsaved: bool, save_as: bool) -> bool {
    unsaved || save_as
}
/// Add `.tct` when omitted and reject incompatible extensions.
pub fn ensure_tct_extension(path: PathBuf) -> Result<PathBuf, String> {
    match path.extension().and_then(|extension| extension.to_str()) {
        None => Ok(path.with_extension("tct")),
        Some(extension) if extension.eq_ignore_ascii_case("tct") => Ok(path),
        Some(extension) => Err(format!(
            "incompatible file extension .{extension}; choose a .tct file"
        )),
    }
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
}
