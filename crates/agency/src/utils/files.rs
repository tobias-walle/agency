use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::config::AgencyPaths;
use crate::utils::task::TaskRef;
use crate::utils::term::print_table;

static FILE_NAME_RE: OnceLock<Regex> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
  pub id: u32,
  pub name: String,
}

impl FileRef {
  pub fn filename(&self) -> String {
    format_file_name(self.id, &self.name)
  }
}

pub fn files_dir_for_task(paths: &AgencyPaths, task: &TaskRef) -> PathBuf {
  paths
    .files_dir()
    .join(format!("{}-{}", task.id, task.slug))
}

pub fn file_path(paths: &AgencyPaths, task: &TaskRef, file: &FileRef) -> PathBuf {
  files_dir_for_task(paths, task).join(file.filename())
}

pub fn local_files_dir() -> PathBuf {
  PathBuf::from(".agency").join("local").join("files")
}

pub fn local_files_path(worktree: &Path) -> PathBuf {
  worktree.join(local_files_dir())
}

pub fn list_files(paths: &AgencyPaths, task: &TaskRef) -> Result<Vec<FileRef>> {
  let dir = files_dir_for_task(paths, task);
  if !dir.exists() {
    return Ok(Vec::new());
  }
  let mut out = Vec::new();
  for entry in
    fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
  {
    let entry = entry?;
    let path = entry.path();
    if !path.is_file() {
      continue;
    }
    let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
      continue;
    };
    if let Some((id, name)) = parse_file_name(filename) {
      out.push(FileRef { id, name });
    }
  }
  out.sort_by_key(|f| f.id);
  Ok(out)
}

pub fn resolve_file(paths: &AgencyPaths, task: &TaskRef, ident: &str) -> Result<FileRef> {
  let files = list_files(paths, task)?;

  if let Ok(id) = ident.parse::<u32>() {
    for file in &files {
      if file.id == id {
        return Ok(file.clone());
      }
    }
    bail!("File {id} not found");
  }

  let matches: Vec<&FileRef> = files.iter().filter(|f| f.name == ident).collect();
  match matches.len() {
    0 => bail!("File '{ident}' not found. Use ID or exact filename"),
    1 => Ok(matches[0].clone()),
    _ => bail!("Multiple files match '{ident}'. Use file ID instead"),
  }
}

pub fn has_files(paths: &AgencyPaths, task: &TaskRef) -> bool {
  let dir = files_dir_for_task(paths, task);
  if !dir.exists() {
    return false;
  }
  fs::read_dir(&dir)
    .ok()
    .is_some_and(|entries| entries.flatten().any(|e| e.path().is_file()))
}

pub fn next_file_id(paths: &AgencyPaths, task: &TaskRef) -> Result<u32> {
  let files = list_files(paths, task)?;
  let max_id = files.iter().map(|f| f.id).max().unwrap_or(0);
  Ok(max_id.saturating_add(1))
}

pub fn add_file(paths: &AgencyPaths, task: &TaskRef, source: &Path) -> Result<FileRef> {
  if !source.exists() {
    bail!("Source file not found: {}", source.display());
  }

  let original_name = source
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_string();

  let data = fs::read(source)
    .with_context(|| format!("failed to read {}", source.display()))?;

  add_file_from_bytes(paths, task, &original_name, &data)
}

pub fn add_file_from_bytes(
  paths: &AgencyPaths,
  task: &TaskRef,
  name: &str,
  data: &[u8],
) -> Result<FileRef> {
  let dir = files_dir_for_task(paths, task);
  fs::create_dir_all(&dir)
    .with_context(|| format!("failed to create {}", dir.display()))?;

  let id = next_file_id(paths, task)?;
  let unique_name = ensure_unique_name(paths, task, name)?;
  let file_ref = FileRef {
    id,
    name: unique_name,
  };

  let dest_path = file_path(paths, task, &file_ref);
  fs::write(&dest_path, data)
    .with_context(|| format!("failed to write {}", dest_path.display()))?;

  Ok(file_ref)
}

pub fn remove_file(paths: &AgencyPaths, task: &TaskRef, file: &FileRef) -> Result<()> {
  let path = file_path(paths, task, file);
  if path.exists() {
    fs::remove_file(&path)
      .with_context(|| format!("failed to remove {}", path.display()))?;
  }
  Ok(())
}

fn ensure_unique_name(paths: &AgencyPaths, task: &TaskRef, name: &str) -> Result<String> {
  let files = list_files(paths, task)?;
  let existing_names: std::collections::HashSet<String> =
    files.iter().map(|f| f.name.clone()).collect();

  if !existing_names.contains(name) {
    return Ok(name.to_string());
  }

  let (stem, ext) = split_stem_ext(name);
  let mut counter = 2u32;
  loop {
    let candidate = if ext.is_empty() {
      format!("{stem}-{counter}")
    } else {
      format!("{stem}-{counter}.{ext}")
    };
    if !existing_names.contains(&candidate) {
      crate::log_warn!("Renamed to {candidate} (name already exists)");
      return Ok(candidate);
    }
    counter = counter.saturating_add(1);
    if counter > 1000 {
      bail!("Could not find unique name for {name}");
    }
  }
}

fn split_stem_ext(name: &str) -> (&str, &str) {
  match name.rfind('.') {
    Some(pos) if pos > 0 => (&name[..pos], &name[pos + 1..]),
    _ => (name, ""),
  }
}

pub fn parse_file_name(filename: &str) -> Option<(u32, String)> {
  let re = FILE_NAME_RE.get_or_init(|| Regex::new(r"^(\d+)-(.+)$").expect("valid regex"));
  let caps = re.captures(filename)?;
  let id_str = caps.get(1)?.as_str();
  let name = caps.get(2)?.as_str().to_string();
  let id = id_str.parse::<u32>().ok()?;
  Some((id, name))
}

pub fn format_file_name(id: u32, name: &str) -> String {
  format!("{id}-{name}")
}

pub fn display_path(
  paths: &AgencyPaths,
  task: &TaskRef,
  file: &FileRef,
  in_worktree: bool,
) -> String {
  if in_worktree {
    local_files_dir()
      .join(file.filename())
      .display()
      .to_string()
  } else {
    file_path(paths, task, file).display().to_string()
  }
}

/// Print a formatted table of files to stdout.
///
/// # Errors
/// This function does not return errors; it always succeeds.
pub fn print_files_table(
  paths: &AgencyPaths,
  task: &TaskRef,
  files: &[FileRef],
  in_worktree: bool,
) {
  let headers = ["ID", "FILENAME", "PATH"];
  let rows: Vec<Vec<String>> = files
    .iter()
    .map(|file| {
      let path = display_path(paths, task, file, in_worktree);
      vec![file.id.to_string(), file.name.clone(), path]
    })
    .collect();
  print_table(&headers, &rows);
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::TempDir;

  fn make_paths(dir: &TempDir) -> AgencyPaths {
    AgencyPaths::new(dir.path(), dir.path())
  }

  fn make_task() -> TaskRef {
    TaskRef {
      id: 1,
      slug: "test-task".to_string(),
    }
  }

  #[test]
  fn list_files_returns_empty_for_missing_dir() {
    let dir = TempDir::new().unwrap();
    let paths = make_paths(&dir);
    let task = make_task();
    let files = list_files(&paths, &task).unwrap();
    assert!(files.is_empty());
  }

  #[test]
  fn list_files_parses_file_format_correctly() {
    let dir = TempDir::new().unwrap();
    let paths = make_paths(&dir);
    let task = make_task();

    let files_dir = files_dir_for_task(&paths, &task);
    fs::create_dir_all(&files_dir).unwrap();
    fs::write(files_dir.join("1-screenshot.png"), b"png data").unwrap();
    fs::write(files_dir.join("2-spec.pdf"), b"pdf data").unwrap();
    fs::write(files_dir.join("invalid-name.txt"), b"ignored").unwrap();

    let files = list_files(&paths, &task).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].id, 1);
    assert_eq!(files[0].name, "screenshot.png");
    assert_eq!(files[1].id, 2);
    assert_eq!(files[1].name, "spec.pdf");
  }

  #[test]
  fn next_file_id_returns_1_for_empty_dir() {
    let dir = TempDir::new().unwrap();
    let paths = make_paths(&dir);
    let task = make_task();
    let id = next_file_id(&paths, &task).unwrap();
    assert_eq!(id, 1);
  }

  #[test]
  fn next_file_id_increments_from_max() {
    let dir = TempDir::new().unwrap();
    let paths = make_paths(&dir);
    let task = make_task();

    let files_dir = files_dir_for_task(&paths, &task);
    fs::create_dir_all(&files_dir).unwrap();
    fs::write(files_dir.join("1-a.txt"), b"").unwrap();
    fs::write(files_dir.join("5-b.txt"), b"").unwrap();

    let id = next_file_id(&paths, &task).unwrap();
    assert_eq!(id, 6);
  }

  #[test]
  fn add_file_copies_to_correct_location() {
    let dir = TempDir::new().unwrap();
    let paths = make_paths(&dir);
    let task = make_task();

    let source = dir.path().join("source.txt");
    fs::write(&source, b"content").unwrap();

    let file_ref = add_file(&paths, &task, &source).unwrap();
    assert_eq!(file_ref.id, 1);
    assert_eq!(file_ref.name, "source.txt");

    let dest = file_path(&paths, &task, &file_ref);
    assert!(dest.exists());
    assert_eq!(fs::read(&dest).unwrap(), b"content");
  }

  #[test]
  fn remove_file_deletes_correctly() {
    let dir = TempDir::new().unwrap();
    let paths = make_paths(&dir);
    let task = make_task();

    let files_dir = files_dir_for_task(&paths, &task);
    fs::create_dir_all(&files_dir).unwrap();
    let file_path_full = files_dir.join("1-test.txt");
    fs::write(&file_path_full, b"data").unwrap();

    let file_ref = FileRef {
      id: 1,
      name: "test.txt".to_string(),
    };
    remove_file(&paths, &task, &file_ref).unwrap();
    assert!(!file_path_full.exists());
  }

  #[test]
  fn resolve_file_by_id_works() {
    let dir = TempDir::new().unwrap();
    let paths = make_paths(&dir);
    let task = make_task();

    let files_dir = files_dir_for_task(&paths, &task);
    fs::create_dir_all(&files_dir).unwrap();
    fs::write(files_dir.join("3-doc.pdf"), b"").unwrap();

    let file = resolve_file(&paths, &task, "3").unwrap();
    assert_eq!(file.id, 3);
    assert_eq!(file.name, "doc.pdf");
  }

  #[test]
  fn resolve_file_by_name_works() {
    let dir = TempDir::new().unwrap();
    let paths = make_paths(&dir);
    let task = make_task();

    let files_dir = files_dir_for_task(&paths, &task);
    fs::create_dir_all(&files_dir).unwrap();
    fs::write(files_dir.join("1-readme.md"), b"").unwrap();

    let file = resolve_file(&paths, &task, "readme.md").unwrap();
    assert_eq!(file.id, 1);
    assert_eq!(file.name, "readme.md");
  }

  #[test]
  fn parse_file_name_works() {
    assert_eq!(parse_file_name("1-doc.pdf"), Some((1, "doc.pdf".to_string())));
    assert_eq!(parse_file_name("12-my-file.txt"), Some((12, "my-file.txt".to_string())));
    assert_eq!(parse_file_name("invalid.txt"), None);
    assert_eq!(parse_file_name("no-number"), None);
  }
}
