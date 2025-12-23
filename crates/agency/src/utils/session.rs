use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::commands::shell::resolve_shell_argv;
use crate::config::AppContext;
use crate::daemon_protocol::TaskMeta;
use crate::utils::bootstrap::prepare_worktree_for_task;
use crate::utils::cmd::{CmdCtx, expand_argv};
use crate::utils::command::as_shell_command;
use crate::utils::git::{ensure_branch_at, open_main_repo, repo_workdir_or, rev_parse};
use crate::utils::interactive;
use crate::utils::task::{
  TaskFrontmatterExt, TaskRef, agent_for_task, branch_name, read_task_content,
};
use crate::utils::tmux;

/// Build the standard Agency environment variables for a task.
/// Returns a `HashMap` with inherited env vars plus `AGENCY_TASK`, `AGENCY_ROOT`, and `AGENCY_TASK_ID`.
pub fn build_task_env(
  task_id: u32,
  task_description: &str,
  repo_root: &Path,
) -> HashMap<String, String> {
  let mut env_map: HashMap<String, String> = std::env::vars().collect();
  env_map.insert("AGENCY_TASK".to_string(), task_description.to_string());
  let root_abs = repo_root
    .canonicalize()
    .unwrap_or_else(|_| repo_root.to_path_buf())
    .display()
    .to_string();
  env_map.insert("AGENCY_ROOT".to_string(), root_abs);
  env_map.insert("AGENCY_TASK_ID".to_string(), task_id.to_string());
  env_map
}

pub struct SessionPlan {
  pub task_meta: TaskMeta,
  pub repo_root: PathBuf,
  pub worktree_dir: PathBuf,
  pub agent_program: String,
  pub agent_args: Vec<String>,
  pub env_map: HashMap<String, String>,
  pub shell_argv: Vec<String>,
}

pub fn build_session_plan(ctx: &AppContext, task: &TaskRef) -> Result<SessionPlan> {
  // Load content and front matter
  let content = read_task_content(&ctx.paths, task)?;
  let frontmatter = content.frontmatter.clone();
  let description = content.body.trim().to_string();

  // Compute repo root and ensure branch/worktree
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let base_branch = frontmatter.base_branch(ctx);
  // Ensure base branch resolves
  if rev_parse(&repo_root, &base_branch).is_err() {
    anyhow::bail!(
      "No worktree can be created as base branch has no commits. Please create an initial commit in your basebranch, e.g. by using `touch README.md; git add .; git commit -m 'init'`."
    );
  }
  let branch = branch_name(task);
  let _ = ensure_branch_at(&repo, &branch, &base_branch)?;
  let worktree_dir = prepare_worktree_for_task(ctx, &repo, task, &branch)?;

  // Build env map
  let env_map = build_task_env(task.id, &description, &repo_root);

  // Select agent and expand argv
  let agent_name = agent_for_task(&ctx.config, frontmatter.as_ref()).ok_or_else(|| {
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
  if argv.is_empty() {
    anyhow::bail!("expanded agent argv is empty");
  }
  let agent_program = argv[0].clone();
  let agent_args = if argv.len() > 1 {
    argv[1..].to_vec()
  } else {
    Vec::new()
  };
  let shell_argv = resolve_shell_argv(&ctx.config);

  let task_meta = TaskMeta {
    id: task.id,
    slug: task.slug.clone(),
  };
  Ok(SessionPlan {
    task_meta,
    repo_root,
    worktree_dir,
    agent_program,
    agent_args,
    env_map,
    shell_argv,
  })
}

pub fn start_session_for_task(ctx: &AppContext, plan: &SessionPlan, attach: bool) -> Result<()> {
  // Start interactive shell as pane process
  let sh_prog = plan
    .shell_argv
    .first()
    .cloned()
    .unwrap_or_else(|| "/bin/sh".into());
  let sh_args: Vec<String> = plan.shell_argv.iter().skip(1).cloned().collect();
  tmux::start_session(
    &ctx.config,
    &plan.repo_root,
    &plan.task_meta,
    &plan.worktree_dir,
    &sh_prog,
    &sh_args,
  )?;

  // Inject environment into session
  let target = tmux::session_name(plan.task_meta.id, &plan.task_meta.slug);
  for (k, v) in &plan.env_map {
    let _ = tmux::tmux_set_env_local(&ctx.config, &target, k, v);
  }

  // Send agent command into the shell using POSIX quoting
  let run = as_shell_command(&plan.agent_program, &plan.agent_args);
  tmux::send_keys(&ctx.config, &target, &run)?;
  tmux::send_keys_enter(&ctx.config, &target)?;

  if attach {
    interactive::scope(|| tmux::attach_session(&ctx.config, &plan.task_meta))
  } else {
    Ok(())
  }
}
