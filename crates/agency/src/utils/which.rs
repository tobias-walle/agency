use std::path::{Path, PathBuf};

/// Resolve `program` to an executable path by walking PATH entries.
#[must_use]
pub(crate) fn which(program: &str) -> Option<PathBuf> {
  let has_sep = program.contains(std::path::MAIN_SEPARATOR);
  if has_sep {
    let candidate = PathBuf::from(program);
    return if is_executable(&candidate) {
      Some(candidate)
    } else {
      None
    };
  }

  let paths = std::env::var_os("PATH")?;
  std::env::split_paths(&paths)
    .map(|dir| dir.join(program))
    .find(|candidate| is_executable(candidate))
}

/// Returns true when `path` points to a regular executable file.
#[must_use]
pub(crate) fn is_executable(path: &Path) -> bool {
  if !path.is_file() {
    return false;
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
      .map(|meta| meta.permissions().mode() & 0o111 != 0)
      .unwrap_or(false)
  }
  #[cfg(not(unix))]
  {
    true
  }
}
