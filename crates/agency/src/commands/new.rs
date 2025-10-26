use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize as _;

use crate::config::AgencyConfig;

pub fn run(cfg: &AgencyConfig, slug: &str) -> Result<()> {
  let slug = normalize_and_validate_slug(slug)?;

  let tasks = cfg.tasks_dir();
  let created = ensure_dir(&tasks)?;
  if created {
    anstream::println!("Created folder {}", ".agency/tasks".cyan());
  }

  if slug_exists(&tasks, &slug)? {
    bail!("Task with slug {} already exists", slug);
  }

  let id = next_id(&tasks)?;
  let file_path = tasks.join(format!("{}-{}.md", id, slug));
  let content = format!("# Task {}: {}\n", id, slug);
  fs::write(&file_path, content)
    .with_context(|| format!("failed to write {}", file_path.display()))?;

  anstream::println!("Task {} with id {} created ✨", slug.cyan(), id.cyan());

  Ok(())
}

fn ensure_dir(dir: &Path) -> Result<bool> {
  if dir.exists() {
    return Ok(false);
  }
  fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
  Ok(true)
}

fn normalize_and_validate_slug(input: &str) -> Result<String> {
  let lowered = input.to_lowercase();
  if lowered.is_empty() {
    bail!("invalid slug: empty");
  }
  if !lowered.chars().all(|c| c.is_alphanumeric() || c == '-') {
    bail!("invalid slug: only Unicode letters, digits and '-' allowed");
  }
  Ok(lowered)
}

fn slug_exists(tasks: &Path, slug: &str) -> Result<bool> {
  if !tasks.exists() {
    return Ok(false);
  }
  for entry in fs::read_dir(tasks).with_context(|| format!("failed to read {}", tasks.display()))? {
    let entry = entry?;
    let path = entry.path();
    if path.is_file()
      && let Some(name) = path.file_name().and_then(OsStr::to_str)
      && let Some((prefix, rest)) = name.split_once('-')
      && prefix.chars().all(|c| c.is_ascii_digit())
      && rest.ends_with(".md")
    {
      let slug_part = &rest[..rest.len() - 3];
      if slug_part == slug {
        return Ok(true);
      }
    }
  }
  Ok(false)
}

fn next_id(tasks: &Path) -> Result<u32> {
  let mut max_id: u32 = 0;
  if tasks.exists() {
    for entry in
      fs::read_dir(tasks).with_context(|| format!("failed to read {}", tasks.display()))?
    {
      let entry = entry?;
      let path = entry.path();
      if path.is_file()
        && let Some(name) = path.file_name().and_then(OsStr::to_str)
        && let Some((prefix, rest)) = name.split_once('-')
        && rest.ends_with(".md")
        && let Ok(id) = prefix.parse::<u32>()
        && id > max_id
      {
        max_id = id;
      }
    }
  }
  Ok(max_id.saturating_add(1))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalize_validates() {
    assert!(normalize_and_validate_slug("märchen-test").is_ok());
    assert!(normalize_and_validate_slug("").is_err());
    assert!(normalize_and_validate_slug("bad/slug").is_err());
  }
}
