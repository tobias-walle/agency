use anyhow::Result;

use crate::commands::shell::resolve_shell_argv;
use crate::config::AppContext;
use crate::daemon_protocol::TaskMeta;
use crate::utils::command::as_shell_command;
use crate::utils::daemon::list_sessions_for_project;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::interactive;
use crate::utils::task::resolve_id_or_slug;
use crate::utils::tmux;
use crate::utils::{
  cmd::{CmdCtx, expand_argv},
  command::Command as LocalCommand,
};

pub fn run_with_task(ctx: &AppContext, ident: &str) -> Result<()> {
  // Initialize env_logger similar to pty-demo main
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .format_timestamp_secs()
    .try_init();

  // Resolve task
  let task = resolve_id_or_slug(&ctx.paths, ident)?;

  // Compute project key (canonical main repo workdir)
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());

  // Query existing sessions and join the latest for this task; error if none
  let entries = list_sessions_for_project(ctx)?;
  let target = entries
    .into_iter()
    .filter(|e| e.task.id == task.id && e.task.slug == task.slug)
    .max_by_key(|e| e.created_at_ms);

  let task_meta = TaskMeta {
    id: task.id,
    slug: task.slug.clone(),
  };
  if target.is_some() {
    return interactive::scope(|| tmux::attach_session(&ctx.config, &task_meta));
  }
  // Auto-start when missing: build agent command and start session, then attach
  let content = crate::utils::task::read_task_content(&ctx.paths, &task)?;
  let frontmatter = content.frontmatter.clone();
  let description = content.body.trim().to_string();
  let mut env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
  env_map.insert("AGENCY_TASK".to_string(), description);
  let root_abs = repo_root
    .canonicalize()
    .unwrap_or(repo_root.clone())
    .display()
    .to_string();
  env_map.insert("AGENCY_ROOT".to_string(), root_abs);
  env_map.insert("AGENCY_TASK_ID".to_string(), task.id.to_string());
  let agent_name = crate::utils::task::agent_for_task(&ctx.config, frontmatter.as_ref())
    .ok_or_else(|| anyhow::anyhow!("no agent selected"))?;
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
  // Always wrap agent commands in user's shell
  let sh_argv = resolve_shell_argv(&ctx.config);
  let sh_prog = sh_argv.first().cloned().unwrap_or_else(|| "/bin/sh".into());
  let sh_args: Vec<String> = sh_argv.into_iter().skip(1).collect();
  let payload = as_shell_command(&cmd_local.program, &cmd_local.args);
  let mut new_args = sh_args;
  new_args.push("-c".into());
  new_args.push(payload);
  cmd_local.program = sh_prog;
  cmd_local.args = new_args;
  let worktree_dir = crate::utils::task::worktree_dir(&ctx.paths, &task);
  crate::utils::daemon::notify_after_task_change(ctx, || {
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

pub fn run_join_session(ctx: &AppContext, session_id: u64) -> Result<()> {
  let entries = list_sessions_for_project(ctx)?;
  let Some(si) = entries.into_iter().find(|e| e.session_id == session_id) else {
    anyhow::bail!("Session not found: {session_id}");
  };
  interactive::scope(|| tmux::attach_session(&ctx.config, &si.task))
}
