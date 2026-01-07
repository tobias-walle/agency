use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use super::metadata::TaskRef;

static TRAILING_NUM_RE: OnceLock<Regex> = OnceLock::new();

/// Compute a unique slug by scanning the tasks dir and appending or incrementing a trailing number.
/// Examples:
/// - base `alpha` with existing {`alpha`} -> `alpha2`
/// - base `alpha2` with existing {`alpha2`} -> `alpha3`
/// - base `alpha` with existing {`alpha`, `alpha2`, `alpha3`} -> `alpha4`
pub fn compute_unique_slug(tasks: &Path, base: &str) -> Result<String> {
  // Collect existing slugs
  let mut existing: HashSet<String> = HashSet::new();
  if tasks.exists() {
    for entry in
      std::fs::read_dir(tasks).with_context(|| format!("failed to read {}", tasks.display()))?
    {
      let entry = entry?;
      let path = entry.path();
      if let Some(tf) = TaskRef::from_task_file(&path) {
        existing.insert(tf.slug);
      }
    }
  }

  // If the base isn't taken, use it directly
  if !existing.contains(base) {
    return Ok(base.to_string());
  }

  // Determine prefix and base numeric suffix (if any) using regex
  let re_trailing = TRAILING_NUM_RE
    .get_or_init(|| Regex::new(r"^(?P<prefix>.*?)(?P<num>\d+)$").expect("valid regex"));
  let (prefix, base_n): (&str, u64) = if let Some(c) = re_trailing.captures(base) {
    let p = c.name("prefix").map_or("", |m| m.as_str());
    let n = c
      .name("num")
      .map_or(0, |m| m.as_str().parse::<u64>().unwrap_or(0));
    (p, n)
  } else {
    (base, 0)
  };

  // Find the highest numeric suffix among existing slugs for this prefix
  let mut max_n = if base_n == 0 { 1 } else { base_n };
  let dyn_re =
    Regex::new(&format!(r"^{}(?P<num>\d+)?$", regex::escape(prefix))).expect("valid regex");
  for s in &existing {
    if let Some(caps) = dyn_re.captures(s) {
      let n = caps
        .name("num")
        .and_then(|m| m.as_str().parse::<u64>().ok())
        .unwrap_or(1);
      max_n = max_n.max(n);
    }
  }

  // Next available number
  let next = max_n.saturating_add(1);
  Ok(format!("{prefix}{next}"))
}

/// Compute the next global id as `max(existing_ids) + 1`.
pub fn next_id(tasks: &Path) -> Result<u32> {
  let mut max_id: u32 = 0;
  if tasks.exists() {
    for entry in
      std::fs::read_dir(tasks).with_context(|| format!("failed to read {}", tasks.display()))?
    {
      let entry = entry?;
      let path = entry.path();
      if let Some(tf) = TaskRef::from_task_file(&path)
        && tf.id > max_id
      {
        max_id = tf.id;
      }
    }
  }
  Ok(max_id.saturating_add(1))
}

pub fn normalize_and_validate_slug(input: &str) -> Result<String> {
  // Slugify: lowercase, replace any non-alphanumeric with '-', collapse runs,
  // and trim leading/trailing '-'. Allow Unicode alphanumerics.
  let lowered = input.to_lowercase();
  let mut out = String::with_capacity(lowered.len());
  for ch in lowered.chars() {
    if ch.is_alphanumeric() {
      out.push(ch);
    } else if !out.ends_with('-') {
      out.push('-');
    }
  }
  // Trim leading/trailing '-'
  let trimmed = out.trim_matches('-').to_string();
  if trimmed.is_empty() {
    bail!("invalid slug: empty after slugify");
  }
  // Enforce starting with a letter to keep branch/task names readable
  if !trimmed
    .chars()
    .next()
    .is_some_and(|c| c.is_ascii_alphabetic())
  {
    bail!("invalid slug: must start with a letter");
  }
  Ok(trimmed)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use tempfile::TempDir;

  #[test]
  fn compute_unique_slug_returns_base_if_available() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();

    let result = compute_unique_slug(&tasks, "alpha").expect("should succeed");
    assert_eq!(result, "alpha");
  }

  #[test]
  fn compute_unique_slug_appends_2_when_base_taken() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1-alpha.md"), "test").unwrap();

    let result = compute_unique_slug(&tasks, "alpha").expect("should succeed");
    assert_eq!(result, "alpha2");
  }

  #[test]
  fn compute_unique_slug_increments_existing_number() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1-alpha2.md"), "test").unwrap();

    let result = compute_unique_slug(&tasks, "alpha2").expect("should succeed");
    assert_eq!(result, "alpha3");
  }

  #[test]
  fn compute_unique_slug_finds_max_and_increments() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1-alpha.md"), "test").unwrap();
    fs::write(tasks.join("2-alpha2.md"), "test").unwrap();
    fs::write(tasks.join("3-alpha3.md"), "test").unwrap();

    let result = compute_unique_slug(&tasks, "alpha").expect("should succeed");
    assert_eq!(result, "alpha4");
  }

  #[test]
  fn compute_unique_slug_handles_gaps_in_sequence() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1-alpha.md"), "test").unwrap();
    fs::write(tasks.join("2-alpha5.md"), "test").unwrap();

    let result = compute_unique_slug(&tasks, "alpha").expect("should succeed");
    assert_eq!(result, "alpha6");
  }

  #[test]
  fn compute_unique_slug_handles_prefix_only_match() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1-alpha.md"), "test").unwrap();
    fs::write(tasks.join("2-alphabeta.md"), "test").unwrap();

    let result = compute_unique_slug(&tasks, "alpha").expect("should succeed");
    assert_eq!(result, "alpha2");
  }

  #[test]
  fn compute_unique_slug_works_with_empty_dir() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");

    let result = compute_unique_slug(&tasks, "beta").expect("should succeed");
    assert_eq!(result, "beta");
  }

  #[test]
  fn compute_unique_slug_ignores_non_md_files() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1-gamma.txt"), "test").unwrap();
    fs::write(tasks.join("gamma.md"), "test").unwrap();

    let result = compute_unique_slug(&tasks, "gamma").expect("should succeed");
    assert_eq!(result, "gamma");
  }

  #[test]
  fn next_id_returns_1_for_empty_dir() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");

    let result = next_id(&tasks).expect("should succeed");
    assert_eq!(result, 1);
  }

  #[test]
  fn next_id_returns_max_plus_one() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1-alpha.md"), "test").unwrap();
    fs::write(tasks.join("3-beta.md"), "test").unwrap();
    fs::write(tasks.join("2-gamma.md"), "test").unwrap();

    let result = next_id(&tasks).expect("should succeed");
    assert_eq!(result, 4);
  }

  #[test]
  fn next_id_ignores_non_task_files() {
    let dir = TempDir::new().expect("tmp");
    let tasks = dir.path().join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("5-task.md"), "test").unwrap();
    fs::write(tasks.join("999-invalid.txt"), "test").unwrap();
    fs::write(tasks.join("not-a-task.md"), "test").unwrap();

    let result = next_id(&tasks).expect("should succeed");
    assert_eq!(result, 6);
  }

  #[test]
  fn normalize_and_validate_slug_handles_unicode() {
    assert_eq!(
      normalize_and_validate_slug("café").unwrap(),
      "café"
    );
    assert_eq!(
      normalize_and_validate_slug("naïve-approach").unwrap(),
      "naïve-approach"
    );
  }

  #[test]
  fn normalize_and_validate_slug_trims_hyphens() {
    assert_eq!(
      normalize_and_validate_slug("-alpha-").unwrap(),
      "alpha"
    );
    assert_eq!(
      normalize_and_validate_slug("--beta--").unwrap(),
      "beta"
    );
  }

  #[test]
  fn normalize_and_validate_slug_collapses_multiple_hyphens() {
    assert_eq!(
      normalize_and_validate_slug("foo---bar").unwrap(),
      "foo-bar"
    );
    assert_eq!(
      normalize_and_validate_slug("a____b").unwrap(),
      "a-b"
    );
  }

  #[test]
  fn normalize_and_validate_slug_rejects_leading_digit() {
    assert!(normalize_and_validate_slug("9lives").is_err());
    assert!(normalize_and_validate_slug("42answer").is_err());
  }

  #[test]
  fn normalize_and_validate_slug_accepts_trailing_numbers() {
    assert_eq!(
      normalize_and_validate_slug("task123").unwrap(),
      "task123"
    );
  }

  #[test]
  fn normalize_and_validate_slug_handles_special_chars() {
    assert_eq!(
      normalize_and_validate_slug("fix@bug#123").unwrap(),
      "fix-bug-123"
    );
    assert_eq!(
      normalize_and_validate_slug("task (v2)").unwrap(),
      "task-v2"
    );
  }
}
