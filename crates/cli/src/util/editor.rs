use std::process::Command;

pub fn edit_text(initial: &str) -> std::io::Result<String> {
  let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
  tracing::debug!(event = "cli_editor_resolved", editor = %editor, "resolved editor");

  let mut path = std::env::temp_dir();
  let fname = format!(
    "agency-edit-{}-{}.md",
    std::process::id(),
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_millis()
  );
  path.push(fname);
  std::fs::write(&path, initial)?;

  tracing::debug!(event = "cli_editor_launch", path = %path.display(), "launching editor");
  let status = Command::new(&editor)
    .arg(&path)
    .status()
    .map_err(|e| std::io::Error::other(format!("failed to launch editor '{}': {}", editor, e)))?;
  if !status.success() {
    return Err(std::io::Error::other(format!(
      "editor exited with status: {}",
      status
    )));
  }

  let body = std::fs::read_to_string(&path)?;
  let _ = std::fs::remove_file(&path);
  tracing::debug!(
    event = "cli_task_body_ready",
    len = body.len(),
    "editor produced body"
  );
  Ok(body)
}
