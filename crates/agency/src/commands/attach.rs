use anyhow::Result;

use crate::config::{AppContext, compute_socket_path};
use crate::pty::client as pty_client;
use crate::pty::protocol::{ProjectKey, SessionOpenMeta, TaskMeta, WireCommand};
use crate::utils::daemon::list_sessions_for_project;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::interactive;
use crate::utils::task::resolve_id_or_slug;

pub fn run_with_task(ctx: &AppContext, ident: &str) -> Result<()> {
  // Initialize env_logger similar to pty-demo main
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .format_timestamp_secs()
    .try_init();

  // Resolve task
  let task = resolve_id_or_slug(&ctx.paths, ident)?;

  // Compute socket path from config
  let socket = compute_socket_path(&ctx.config);

  // Compute project key (canonical main repo workdir)
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };

  // Query existing sessions and join the latest for this task; error if none
  let entries = list_sessions_for_project(ctx)?;
  let target = entries
    .into_iter()
    .filter(|e| e.task.id == task.id && e.task.slug == task.slug)
    .max_by_key(|e| e.created_at_ms);

  let Some(session) = target else {
    anyhow::bail!("Session not running. Please start it first");
  };

  // For join, the open meta is unused; provide minimal values.
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

  interactive::scope(|| {
    pty_client::run_attach(&socket, open, Some(session.session_id), &ctx.config)
  })
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
  interactive::scope(|| pty_client::run_attach(&socket, open, Some(session_id), &ctx.config))
}
