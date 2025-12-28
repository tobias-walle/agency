use std::io::Write as _;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use crate::config::AppContext;
use crate::utils::context::is_in_worktree;
use crate::utils::files::{display_path, list_files};
use crate::utils::task::resolve_id_or_slug;
use crate::utils::which;

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  if which::which("fzf").is_none() {
    bail!("fzf is not installed. Install it from https://github.com/junegunn/fzf");
  }

  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  let files = list_files(&ctx.paths, &task)?;

  if files.is_empty() {
    bail!("No files attached to task");
  }

  let in_worktree = is_in_worktree(&ctx.paths);

  let lines: Vec<String> = files
    .iter()
    .map(|f| {
      let path = display_path(&ctx.paths, &task, f, in_worktree);
      format!("{}\t{}\t{}", f.id, f.name, path)
    })
    .collect();

  let input = lines.join("\n");
  let selected = run_fzf(&input)?;

  let Some(selected) = selected else {
    std::process::exit(1);
  };

  let id = selected
    .split('\t')
    .next()
    .and_then(|s| s.parse::<u32>().ok());

  let Some(id) = id else {
    bail!("Failed to parse file ID from selection");
  };

  println!("{id}");
  Ok(())
}

fn run_fzf(input: &str) -> Result<Option<String>> {
  let mut child = Command::new("fzf")
    .args(["--no-multi", "--height=~50%"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;

  if let Some(stdin) = child.stdin.as_mut() {
    stdin.write_all(input.as_bytes())?;
  }

  let output = child.wait_with_output()?;

  if !output.status.success() {
    return Ok(None);
  }

  let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
  if selected.is_empty() {
    return Ok(None);
  }

  Ok(Some(selected))
}
