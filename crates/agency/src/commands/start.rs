use std::collections::HashMap;
use std::os::unix::net::UnixStream;

use anyhow::{Context, Result};

use crate::config::{AppContext, compute_socket_path};
use crate::log_success;
use crate::pty::protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, SessionOpenMeta, TaskMeta, WireCommand, read_frame,
  write_frame,
};
use crate::utils::bootstrap::prepare_worktree_for_task;
use crate::utils::cmd::{CmdCtx, expand_argv};
use crate::utils::command::Command as LocalCommand;
use crate::utils::git::{ensure_branch_at, open_main_repo, repo_workdir_or};
use crate::utils::task::{
  TaskFrontmatter, agent_for_task, branch_name, parse_task_markdown, remove_title,
  resolve_id_or_slug, task_file,
};

/// Start a task's session in the background (no attach).
///
/// Performs the same preparation as `attach` (ensure branch/worktree, compute agent cmd),
/// then establishes a short-lived connection to the daemon, sends `OpenSession`,
/// reads a single `Welcome`, and immediately sends `Detach` and closes.
pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  // Resolve task and load its content
  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  let tf_path = task_file(&ctx.paths, &task);
  let task_text = std::fs::read_to_string(&tf_path)
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

  // Determine base branch from front matter or current HEAD
  let base_branch = frontmatter
    .as_ref()
    .and_then(|fm: &TaskFrontmatter| fm.base_branch.clone())
    .unwrap_or_else(|| crate::utils::git::head_branch(ctx));

  // Ensure task branch exists at the desired start point
  let branch = branch_name(&task);
  let _ = ensure_branch_at(&repo, &branch, &base_branch)?;

  let worktree_dir = prepare_worktree_for_task(ctx, &repo, &task, &branch)?;

  // Build env map and argv
  let mut env_map: HashMap<String, String> = std::env::vars().collect();
  let stripped = remove_title(body, &task.slug);
  env_map.insert("AGENCY_TASK".to_string(), stripped.to_string());

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
      slug: task.slug,
    },
    worktree_dir: worktree_dir.display().to_string(),
    cmd: cmd_wire,
  };

  // Open short-lived connection, send OpenSession, read Welcome, then Detach
  let mut stream = UnixStream::connect(&socket)
    .with_context(|| format!("failed to connect to {}", socket.display()))?;
  // Use default size for headless start
  write_frame(
    &mut stream,
    &C2D::Control(C2DControl::OpenSession {
      meta: open,
      rows: 24,
      cols: 80,
    }),
  )?;
  // Expect Welcome or Error
  match read_frame::<_, D2C>(&mut stream) {
    Ok(D2C::Control(D2CControl::Welcome { session_id, .. })) => {
      // Immediately detach this client
      let _ = write_frame(&mut stream, &C2D::Control(C2DControl::Detach));
      log_success!("Started session {} in background", session_id);
      Ok(())
    }
    Ok(D2C::Control(D2CControl::Error { message })) => anyhow::bail!(message),
    Ok(_) => anyhow::bail!("Protocol: Expected Welcome"),
    Err(err) => Err(err),
  }
}
