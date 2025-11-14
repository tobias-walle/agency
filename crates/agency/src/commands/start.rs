use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::commands::shell::resolve_shell_argv;
use crate::config::AppContext;
use crate::daemon_protocol::TaskMeta;
use crate::utils::bootstrap::prepare_worktree_for_task;
use crate::utils::cmd::{CmdCtx, expand_argv};
use crate::utils::command::Command as LocalCommand;
use crate::utils::command::as_shell_command;
use crate::utils::daemon::list_sessions_for_project;
use crate::utils::git::{ensure_branch_at, open_main_repo, repo_workdir_or};
use crate::utils::interactive;
use crate::utils::task::{agent_for_task, branch_name, read_task_content, resolve_id_or_slug};
use crate::utils::tmux;

/// Start a task's session and attach. Fails if already started.
///
/// Performs the same preparation as `attach` (ensure branch/worktree, compute agent cmd),
/// then attaches to the daemon sending `OpenSession` with the real terminal size.
pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  // Resolve task and load its content
  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  let content = read_task_content(&ctx.paths, &task)?;
  let frontmatter = content.frontmatter.clone();

  // Compute project key (canonical main repo workdir)
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());

  // Determine base branch from front matter or current HEAD
  let base_branch = frontmatter
    .as_ref()
    .and_then(|fm| fm.base_branch.clone())
    .unwrap_or_else(|| crate::utils::git::head_branch(ctx));

  // Ensure task branch exists at the desired start point
  let branch = branch_name(&task);
  let _ = ensure_branch_at(&repo, &branch, &base_branch)?;

  let worktree_dir = prepare_worktree_for_task(ctx, &repo, &task, &branch)?;

  // Build env map and argv
  let description = content.body.trim().to_string();
  let mut env_map: HashMap<String, String> = build_session_env(&repo_root, &description);
  // Also provide the task id for downstream tools
  env_map.insert("AGENCY_TASK_ID".to_string(), task.id.to_string());

  // Select effective agent
  let agent_name = agent_for_task(&ctx.config, frontmatter.as_ref());
  let agent_name = agent_name.ok_or_else(|| {
    let known: Vec<String> = ctx.config.agents.keys().cloned().collect();
    anyhow::anyhow!(
      "no agent selected. Set `agent` in config or add YAML front matter. Known agents: {}",
      known.join(", ")
    )
  })?;
  let agent_cfg = ctx.config.get_agent(&agent_name)?;
  let argv_tmpl = agent_cfg.cmd.clone();
  let ctx_expand = CmdCtx::with_env(
    repo_root
      .canonicalize()
      .unwrap_or(repo_root.clone())
      .display()
      .to_string(),
    env_map.clone(),
  );
  let argv = expand_argv(&argv_tmpl, &ctx_expand);
  let mut cmd_local = LocalCommand::new(&argv)?;
  // Always wrap agent commands in the user's shell to inherit its environment
  let sh_argv = resolve_shell_argv(&ctx.config);
  let sh_prog = sh_argv.first().cloned().unwrap_or_else(|| "/bin/sh".into());
  let sh_args: Vec<String> = sh_argv.into_iter().skip(1).collect();
  let payload = as_shell_command(&cmd_local.program, &cmd_local.args);
  let mut new_args = sh_args;
  new_args.push("-c".into());
  new_args.push(payload);
  cmd_local.program = sh_prog;
  cmd_local.args = new_args;

  let task_meta = TaskMeta {
    id: task.id,
    slug: task.slug.clone(),
  };

  // Fail when a session is already running for this task
  let existing = list_sessions_for_project(ctx)?
    .into_iter()
    .any(|e| e.task.id == task.id && e.task.slug == task.slug);
  if existing {
    anyhow::bail!("Already started. Use attach");
  }

  crate::utils::daemon::notify_after_task_change(ctx, || {
    // Start tmux session and then attach
    tmux::start_session(
      &ctx.config,
      &repo_root,
      &task_meta,
      &worktree_dir,
      &cmd_local.program,
      &cmd_local.args,
    )?;
    interactive::scope(|| tmux::attach_session(&ctx.config, &task_meta))
  })
}

/// Build the environment map passed to agent processes.
///
/// - Starts from the current process environment
/// - Adds `AGENCY_TASK` with the trimmed task description
/// - Adds `AGENCY_ROOT` with the canonicalized main repository root
fn build_session_env(repo_root: &Path, task_description: &str) -> HashMap<String, String> {
  let mut env_map: HashMap<String, String> = std::env::vars().collect();
  env_map.insert(
    "AGENCY_TASK".to_string(),
    task_description.trim().to_string(),
  );
  let root_abs = repo_root
    .canonicalize()
    .unwrap_or_else(|_| repo_root.to_path_buf())
    .display()
    .to_string();
  env_map.insert("AGENCY_ROOT".to_string(), root_abs);
  env_map
}

#[cfg(test)]
mod tests {
  use super::build_session_env;
  use std::fs;
  use std::path::PathBuf;

  #[test]
  fn sets_agency_root_and_task() {
    // Create a temporary directory inside the OS temp dir
    let mut dir = std::env::temp_dir();
    dir.push(format!("agency-root-test-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);

    let env = build_session_env(&dir, "  Body  ");
    let got_root = env.get("AGENCY_ROOT").expect("AGENCY_ROOT present");
    let expected_root = PathBuf::from(&dir)
      .canonicalize()
      .unwrap_or(dir.clone())
      .display()
      .to_string();
    assert_eq!(got_root, &expected_root);
    assert_eq!(env.get("AGENCY_TASK").map(String::as_str), Some("Body"));

    // Cleanup best-effort
    let _ = fs::remove_dir_all(&dir);
  }
}
