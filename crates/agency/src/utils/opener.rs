use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use super::error_messages;

/// Open a file or directory with the system default application.
///
/// # Errors
/// Returns an error if the open command fails or is not available.
pub fn open_with_default(path: &Path) -> Result<()> {
  let path_str = path
    .canonicalize()
    .unwrap_or_else(|_| path.to_path_buf())
    .display()
    .to_string();

  #[cfg(target_os = "macos")]
  {
    let status = Command::new("open").arg(&path_str).status()?;
    if !status.success() {
      bail!(error_messages::OPEN_NON_ZERO_EXIT);
    }
    Ok(())
  }

  #[cfg(target_os = "linux")]
  {
    let status = Command::new("xdg-open").arg(&path_str).status();
    match status {
      Ok(s) if s.success() => Ok(()),
      Ok(_) => bail!(error_messages::XDG_OPEN_NON_ZERO_EXIT),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
        bail!("xdg-open not found. Install xdg-utils package")
      }
      Err(e) => Err(e.into()),
    }
  }

  #[cfg(not(any(target_os = "macos", target_os = "linux")))]
  {
    let _ = path_str;
    bail!("open_with_default not supported on this platform")
  }
}
