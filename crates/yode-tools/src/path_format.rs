use std::path::Path;

pub(crate) fn display_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn relative_display_slash(path: &Path, base: &Path) -> String {
    display_slash(path.strip_prefix(base).unwrap_or(path))
}
