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
