use assert_cmd::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

fn start_daemon(sock: &std::path::Path) {
  let mut cmd = Command::cargo_bin("agency").expect("compile bin");
  let out = cmd
    .env("AGENCY_SOCKET", sock.as_os_str())
    .args(["daemon", "start"]) // starts background
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&out);
  assert!(text.contains("daemon: running"), "{text}");
}

fn stop_daemon(sock: &std::path::Path) {
  let mut cmd = Command::cargo_bin("agency").expect("compile bin");
  let _ = cmd
    .env("AGENCY_SOCKET", sock.as_os_str())
    .args(["daemon", "stop"]) // best-effort
    .assert()
    .success();
}

#[test]
fn attach_handles_fast_small_chunks_and_final_detach() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");

  // Init git repo
  std::process::Command::new("git").arg("init").current_dir(&root).output().unwrap();
  std::fs::write(root.join("README.md"), "# E2E Test\n").unwrap();
  std::process::Command::new("git").args(["add", "."]).current_dir(&root).output().unwrap();
  std::process::Command::new("git").args(["commit", "-m", "Initial commit"]).current_dir(&root).output().unwrap();

  start_daemon(&sock);

  // init project
  Command::cargo_bin("agency").unwrap()
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["init"]) 
    .assert()
    .success();

  // new task
  Command::cargo_bin("agency").unwrap()
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["new", "feat-fast", "--title", "Fast Input Task"]) 
    .assert()
    .success();

  // start task
  Command::cargo_bin("agency").unwrap()
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["start", "feat-fast"]) 
    .assert()
    .success();

  // attach interactively
  let mut attach = Command::cargo_bin("agency").expect("compile bin");
  attach
    .env("AGENCY_SOCKET", sock.as_os_str())
    .env("AGENCY_DETACH_KEYS", "ctrl-p,ctrl-q")
    .current_dir(&root)
    .args(["attach", "feat-fast"]) 
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child = attach.spawn().expect("spawn attach");

  // Write many tiny chunks rapidly that form near-detach prefixes
  {
    let stdin = child.stdin.as_mut().expect("stdin");
    let noise = b"echo hello\n";
    for b in noise {
      stdin.write_all(&[*b]).unwrap();
      thread::sleep(Duration::from_millis(1));
    }
    // send prefix 0x10 in split tiny chunks
    stdin.write_all(&[0x10]).unwrap();
    thread::sleep(Duration::from_millis(1));
    // send some noise that should flush prior pending correctly
    stdin.write_all(b"xyz").unwrap();
    thread::sleep(Duration::from_millis(1));
    // now complete with 0x10,0x11 sequence at the end
    stdin.write_all(&[0x10]).unwrap();
    thread::sleep(Duration::from_millis(1));
    stdin.write_all(&[0x11]).unwrap(); // detach
  }

  let out = child.wait_with_output().expect("wait");
  assert!(out.status.success());
  let stdout = String::from_utf8_lossy(&out.stdout);
  let stderr = String::from_utf8_lossy(&out.stderr);
  assert!(stdout.contains("Attached. Detach:"), "stdout: {}", stdout);
  assert!(stdout.to_lowercase().contains("hello"), "stdout: {}", stdout);
  assert!(stderr.contains("detached"), "stderr: {}", stderr);

  stop_daemon(&sock);
}
