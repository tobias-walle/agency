use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::{AppContext, compute_socket_path};
use crate::pty::client as pty_client;
use crate::pty::protocol::{ProjectKey, SessionOpenMeta, TaskMeta, WireCommand};
use crate::utils::bootstrap::prepare_worktree_for_task;
use crate::utils::cmd::{CmdCtx, expand_argv};
use crate::utils::command::Command as LocalCommand;
use crate::utils::daemon::list_sessions_for_project;
use crate::utils::git::{ensure_branch_at, open_main_repo, repo_workdir_or};
use crate::utils::interactive;
use crate::utils::task::{agent_for_task, branch_name, read_task_content, resolve_id_or_slug};

/// Start a task's session and attach. Fails if already started.
///
/// Performs the same preparation as `attach` (ensure branch/worktree, compute agent cmd),
/// then attaches to the daemon sending `OpenSession` with the real terminal size.
pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  // Resolve task and load its content
  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  let content = read_task_content(&ctx.paths, &task)?;
  let frontmatter = content.frontmatter.clone();

  // Compute socket path from config
  let socket = compute_socket_path(&ctx.config);

  // Compute project key (canonical main repo workdir)
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };

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
  let env_map: HashMap<String, String> = build_session_env(&repo_root, &description);

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
  let cmd_local = LocalCommand::new(&argv)?;

  // Build wire command
  let cmd_wire = WireCommand {
    program: cmd_local.program.clone(),
    args: cmd_local.args.clone(),
    cwd: worktree_dir.display().to_string(),
    env: env_map.into_iter().collect(),
  };

  let open = SessionOpenMeta {
    project,
    task: TaskMeta {
      id: task.id,
      slug: task.slug.clone(),
    },
    worktree_dir: worktree_dir.display().to_string(),
    cmd: cmd_wire,
  };

  // Fail when a session is already running for this task
  let existing = list_sessions_for_project(ctx)?
    .into_iter()
    .any(|e| e.task.id == task.id && e.task.slug == task.slug);
  if existing {
    anyhow::bail!("Already started. Use attach");
  }

  // Attach and open a new session using real terminal size
  interactive::scope(|| pty_client::run_attach(&socket, open, None, &ctx.config))
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
