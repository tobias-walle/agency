mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;

#[test]
#[ignore = "needs-tty"]
fn tui_interactive_creates_and_deletes_task() -> Result<()> {
  TestEnv::run_tty(|env| -> Result<()> {
    env.init_repo()?;

    // Pre-create a task so the TUI can operate on it without going through
    // the interactive New-task overlay, which requires an editor-driven
    // description and is not stable for automated tests.
    env.new_task("tui-task", &[])?;

    env.agency_daemon_start()?;

    env.tmux_new_session(&["agency", "tui"])?;

    env.wait_for(|| {
      let pane = env.tmux_capture_pane()?;
      Ok(pane.contains("ID") && pane.contains("SLUG") && pane.contains("tui-task"))
    })?;

    env.tmux_send_keys("X")?;

    env.wait_for(|| {
      let pane = env.tmux_capture_pane()?;
      Ok(pane.contains("agency rm"))
    })?;

    // Wait for the daemon and TUI to process the delete and refresh the task
    // list, polling until the slug disappears or a timeout is reached.
    env.wait_for(|| {
      let pane = env.tmux_capture_pane()?;
      let still_has_task = pane
        .lines()
        .any(|line| line.contains("tui-task") && !line.contains("Removed task"));
      Ok(!still_has_task)
    })?;

    env.tmux_send_keys("C-c")?;

    env.agency_daemon_stop()?;

    Ok(())
  })
}
