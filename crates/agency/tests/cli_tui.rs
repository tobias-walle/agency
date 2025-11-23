mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
#[ignore = "needs-tty"]
fn tui_interactive_creates_and_deletes_task() -> Result<()> {
  TestEnv::run_tty(|env| -> Result<()> {
    env.init_repo()?;

    env.agency_daemon_start()?;

    env.tmux_new_session(&["agency", "tui"])?;

    // Wait for the TUI to render the empty tasks table.
    eprintln!("cli_tui: waiting for initial table");
    env.wait_for(|| {
      let pane = env.tmux_capture_pane()?;
      Ok(pane.contains("ID") && pane.contains("SLUG"))
    })?;

    // Open the New + Start overlay, enter a slug via the TUI,
    // and submit it. The editor and session startup are handled
    // by the agency binary using the configured EDITOR.
    eprintln!("cli_tui: sending N for New+Start");
    env.tmux_send_keys("N")?;

    eprintln!("cli_tui: sending slug and Enter");
    env.tmux_send_keys("tui-task")?;
    env.tmux_send_keys("Enter")?;

    // Wait for TUI+daemon to create the task and log it.
    eprintln!("cli_tui: waiting for created task log");
    env.wait_for(|| {
      let pane = env.tmux_capture_pane()?;
      Ok(pane.contains("Create task tui-task"))
    })?;

    // Exit the TUI cleanly.
    env.tmux_send_keys("C-c")?;

    // Verify via the CLI that the task exists, then delete it.
    env
      .agency()?
      .arg("tasks")
      .assert()
      .success()
      .stdout(predicates::str::contains("tui-task").from_utf8());

    env.agency()?.arg("rm").arg("1").assert().success();

    env.agency_daemon_stop()?;

    Ok(())
  })
}
