#![cfg(unix)]
#![allow(dead_code)]

use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use std::{fs, thread};

use anyhow::Context;
use expectrl::Expect;
use expectrl::session::OsSession as Session;
use fs_extra::file::{self, CopyOptions};

#[must_use]
pub fn bin() -> PathBuf {
  assert_cmd::cargo::cargo_bin!("agency").to_path_buf()
}

pub fn ensure_fake_agent(workdir: &Path) -> anyhow::Result<()> {
  let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fake_agent.py");

  let dst_dir = workdir.join("scripts");
  create_dir_all(&dst_dir)?;

  let dst = dst_dir.join("fake_agent.py");
  file::copy(&src, &dst, &CopyOptions::new())
    .with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;

  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(&dst)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&dst, perms)?;
  }
  Ok(())
}

pub fn spawn_daemon(bin: &Path, workdir: &Path) -> anyhow::Result<Child> {
  // Ensure the test workdir has the fake agent available at ./scripts/fake_agent.py
  ensure_fake_agent(workdir)?;
  let mut cmd = Command::new(bin);
  cmd
    .arg("daemon")
    .arg("run")
    .env("RUST_LOG", "debug")
    .env("XDG_RUNTIME_DIR", workdir.join("tmp"))
    .current_dir(workdir);
  let child = cmd.spawn().context("failed to spawn daemon")?;
  Ok(child)
}

pub fn wait_for_socket(sock: &Path, timeout: Duration) -> anyhow::Result<()> {
  let start = Instant::now();
  while start.elapsed() < timeout {
    if fs::metadata(sock).is_ok() {
      return Ok(());
    }
    thread::sleep(Duration::from_millis(50));
  }
  anyhow::bail!("socket not created at {}", sock.display());
}

pub fn spawn_attach_pty(bin: &Path, workdir: &Path, task_ident: &str) -> anyhow::Result<Session> {
  // Ensure attach runs in the same temp working directory as the daemon
  // so it resolves `./tmp/daemon.sock` correctly.
  let prev = std::env::current_dir()?;
  std::env::set_current_dir(workdir)?;

  let cmd = format!("{} attach {}", bin.display(), task_ident);
  let mut sess = expectrl::spawn(cmd).context("failed to spawn attach client")?;
  sess.set_expect_timeout(Some(Duration::from_secs(2)));

  // Restore previous working directory for the test process.
  std::env::set_current_dir(prev)?;
  Ok(sess)
}

pub fn send_ctrl_q(sess: &mut Session) -> anyhow::Result<()> {
  // Ctrl-Q is 0x11 (DC1)
  sess.send("\x11")?;
  Ok(())
}
