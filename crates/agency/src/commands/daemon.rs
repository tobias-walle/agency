use anyhow::Result;

use crate::pty::daemon as pty_daemon;

pub fn run() -> Result<()> {
  // Initialize env_logger similar to pty-demo main
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .format_timestamp_secs()
    .try_init();

  pty_daemon::run_daemon()
}
