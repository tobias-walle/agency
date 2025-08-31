use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn cli_help_output_is_stable() {
  let mut cmd = Command::cargo_bin("orchestra").expect("compile bin");
  let output = cmd
    .arg("--help")
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&output);
  let expected = r#"Orchestra CLI

Usage: orchestra [COMMAND]

Commands:
  daemon  Daemon related commands
  init    Create project scaffolding and config
  new     Create a new task
  start   Start a task
  status  Show task status list
  attach  Attach to a task's PTY session
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
"#;
  pretty_assertions::assert_eq!(text, expected);
}

#[test]
fn daemon_start_status_stop_end_to_end() {
  // Unique socket path in tmp dir
  let mut sock = std::env::temp_dir();
  sock.push(format!("orchestra-test-{}.sock", std::process::id()));
  // Ensure parent exists
  std::fs::create_dir_all(sock.parent().unwrap()).unwrap();

  // Start
  let mut start = Command::cargo_bin("orchestra").expect("compile bin");
  let output = start
    .env("ORCHESTRA_SOCKET", &sock)
    .args(["daemon", "start"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&output);
  assert!(
    text.contains("daemon: running ("),
    "unexpected start output: {text}"
  );

  // Status
  let mut status = Command::cargo_bin("orchestra").expect("compile bin");
  let out2 = status
    .env("ORCHESTRA_SOCKET", &sock)
    .args(["daemon", "status"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text2 = String::from_utf8_lossy(&out2);
  assert!(
    text2.contains("daemon: running ("),
    "unexpected status output: {text2}"
  );

  // Stop
  let mut stop = Command::cargo_bin("orchestra").expect("compile bin");
  let out3 = stop
    .env("ORCHESTRA_SOCKET", &sock)
    .args(["daemon", "stop"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text3 = String::from_utf8_lossy(&out3);
  pretty_assertions::assert_eq!(text3, "daemon: stopped\n");
}
