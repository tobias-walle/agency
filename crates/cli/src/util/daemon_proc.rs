use crate::rpc::client;
use std::path::{Path, PathBuf};

pub fn resolve_socket() -> Option<PathBuf> {
  agency_core::config::resolve_socket_path().ok()
}

pub fn ensure_daemon_running() -> PathBuf {
  let sock = match resolve_socket() {
    Some(p) => p,
    None => {
      eprintln!("could not resolve socket path");
      std::process::exit(1);
    }
  };

  let ok = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap()
    .block_on(async { client::daemon_status(&sock).await.is_ok() });
  if ok {
    return sock;
  }

  let resume_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
  let _ = spawn_daemon_background(&sock, &resume_root);

  let _ = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap()
    .block_on(async {
      for _ in 0..20u8 {
        if client::daemon_status(&sock).await.is_ok() {
          return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
      }
      false
    });
  sock
}

pub fn spawn_daemon_background(sock: &Path, resume_root: &Path) -> std::io::Result<()> {
  let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agency"));
  let mut cmd = std::process::Command::new(exe);
  cmd.arg("daemon").arg("run");
  cmd.env("AGENCY_SOCKET", sock);
  cmd.env("AGENCY_RESUME_ROOT", resume_root);
  cmd
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
  let _ = cmd.spawn()?;
  Ok(())
}
