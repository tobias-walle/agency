use std::fs;

use anyhow::{Context, Result};

use crate::config::{AppContext, compute_socket_path};
use crate::pty::client as pty_client;
use crate::utils::task::{resolve_id_or_slug, task_file};

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

  // Compute socket path from config
  let socket = compute_socket_path(&ctx.config);

  pty_client::run_attach(&socket, Some(task_text))
}
