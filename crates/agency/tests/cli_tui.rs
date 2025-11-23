mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use expectrl::{Eof, Expect};
use std::time::{Duration, Instant};

fn wait_for<F>(timeout: Duration, mut assert_fn: F) -> Result<()>
where
  F: FnMut() -> Result<bool>,
{
  let deadline = Instant::now() + timeout;
  loop {
    if assert_fn()? {
      return Ok(());
    }
    assert!(
      Instant::now() < deadline,
      "condition not met within timeout"
    );
    std::thread::sleep(Duration::from_millis(200));
  }
}

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

    let mut session = env.agency_tui()?;

    session.expect("ID")?;
    session.expect("SLUG")?;
    session.expect("tui-task")?;

    session.send("X")?;
    session.expect("agency rm")?;

    // Wait for the daemon and TUI to process the delete and refresh the task
    // list, polling until the slug disappears or a timeout is reached.
    wait_for(Duration::from_secs(10), || {
      let still_visible = session.is_matched("tui-task")?;
      Ok(!still_visible)
    })?;

    session.send("\u{3}")?;
    session.expect(Eof)?;

    env.agency_daemon_stop()?;

    Ok(())
  })
}
