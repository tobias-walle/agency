use std::process::Command;

use anyhow::{Result, bail};

/// Read image data from the system clipboard.
///
/// # Errors
/// Returns an error if no image is in the clipboard or if clipboard access fails.
pub fn read_image_from_clipboard() -> Result<Vec<u8>> {
  #[cfg(target_os = "macos")]
  {
    read_image_macos()
  }
  #[cfg(target_os = "linux")]
  {
    read_image_linux()
  }
  #[cfg(not(any(target_os = "macos", target_os = "linux")))]
  {
    bail!("Clipboard image reading not supported on this platform")
  }
}

#[cfg(target_os = "macos")]
fn read_image_macos() -> Result<Vec<u8>> {
  use std::io::Read as _;

  let temp_dir = std::env::temp_dir();
  let temp_path_buf = temp_dir.join(format!("agency-clipboard-{}.png", std::process::id()));
  let temp_path = temp_path_buf.to_str().ok_or_else(|| {
    anyhow::anyhow!("Could not create temp file path")
  })?;

  let script = format!(
    r#"
    set theFile to POSIX file "{temp_path}"
    try
      set imageData to the clipboard as «class PNGf»
      set fileRef to open for access theFile with write permission
      write imageData to fileRef
      close access fileRef
      return "ok"
    on error errMsg
      return "error: " & errMsg
    end try
    "#
  );

  let output = Command::new("osascript")
    .arg("-e")
    .arg(&script)
    .output()?;

  let stdout = String::from_utf8_lossy(&output.stdout);
  if !output.status.success() || stdout.contains("error:") {
    let _ = std::fs::remove_file(&temp_path_buf);
    bail!("No image in clipboard. Copy an image first");
  }

  let mut file = std::fs::File::open(&temp_path_buf)?;
  let mut data = Vec::new();
  file.read_to_end(&mut data)?;
  let _ = std::fs::remove_file(&temp_path_buf);

  if data.is_empty() {
    bail!("No image in clipboard. Copy an image first");
  }

  Ok(data)
}

#[cfg(target_os = "linux")]
fn read_image_linux() -> Result<Vec<u8>> {
  use std::process::Stdio;

  let output = Command::new("xclip")
    .args(["-selection", "clipboard", "-t", "image/png", "-o"])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output();

  let output = match output {
    Ok(o) => o,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
      bail!("xclip not found. Install it with your package manager")
    }
    Err(e) => return Err(e.into()),
  };

  if !output.status.success() || output.stdout.is_empty() {
    bail!("No image in clipboard. Copy an image first");
  }

  Ok(output.stdout)
}
