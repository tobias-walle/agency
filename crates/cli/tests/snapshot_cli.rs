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
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
"#;
  pretty_assertions::assert_eq!(text, expected);
}

#[test]
fn daemon_status_output_is_deterministic() {
  let mut cmd = Command::cargo_bin("orchestra").expect("compile bin");
  let output = cmd
    .args(["daemon", "status"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&output);
  pretty_assertions::assert_eq!(text, "daemon: stopped\n");
}
