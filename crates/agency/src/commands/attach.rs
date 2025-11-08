use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};

use crate::config::{AppContext, compute_socket_path};
use crate::pty::client as pty_client;
use crate::pty::protocol::{ProjectKey, SessionOpenMeta, TaskMeta, WireCommand};
use crate::utils::cmd::{CmdCtx, expand_argv};
use crate::utils::command::Command as LocalCommand;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::task::{parse_task_markdown, remove_title, resolve_id_or_slug, task_file};

pub fn run_with_task(ctx: &AppContext, ident: &str) -> Result<()> {
  // Initialize env_logger similar to pty-demo main
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .format_timestamp_secs()
    .try_init();

  // Resolve task and load its content
  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  let tf_path = task_file(&ctx.paths, &task);
  let task_text = fs::read_to_string(&tf_path)
    .with_context(|| format!("failed to read {}", tf_path.display()))?;
  let (frontmatter, body) = parse_task_markdown(&task_text);

  // Compute socket path from config
  let socket = compute_socket_path(&ctx.config);

  // Compute project key (canonical main repo workdir)
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };

  // Worktree dir
  let worktree_dir_path = crate::utils::task::worktree_dir(&ctx.paths, &task);
  let worktree_dir = worktree_dir_path
    .canonicalize()
    .unwrap_or(worktree_dir_path.clone());

  // Build env map and argv
  let mut env_map: HashMap<String, String> = std::env::vars().collect();
  let stripped = remove_title(body, &task.slug);
  env_map.insert("AGENCY_TASK".to_string(), stripped.to_string());

  // Select agent: front matter overrides config default
  let selected_agent = frontmatter
    .and_then(|fm| fm.agent)
    .or_else(|| ctx.config.agent.clone());
  let agent_name = selected_agent.ok_or_else(|| {
    let known: Vec<String> = ctx.config.agents.keys().cloned().collect();
    anyhow::anyhow!(
      "no agent selected. Set `agent` in config or add YAML front matter. Known agents: {}",
      known.join(", ")
    )
  })?;
  // Validate configured agents
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
      slug: task.slug,
    },
    worktree_dir: worktree_dir.display().to_string(),
    cmd: cmd_wire,
  };

  pty_client::run_attach(&socket, open, None, &ctx.config)
}

pub fn run_join_session(ctx: &AppContext, session_id: u64) -> Result<()> {
  // For join, the open meta is unused by the handshake; provide minimal values.
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  let cwd = ctx.paths.cwd().display().to_string();
  let open = SessionOpenMeta {
    project,
    task: TaskMeta {
      id: 0,
      slug: String::new(),
    },
    worktree_dir: cwd.clone(),
    cmd: WireCommand {
      program: "sh".to_string(),
      args: vec!["-l".to_string()],
      cwd,
      env: Vec::new(),
    },
  };
  let socket = compute_socket_path(&ctx.config);
  pty_client::run_attach(&socket, open, Some(session_id), &ctx.config)
}
