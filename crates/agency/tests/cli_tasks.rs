mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;

#[test]
fn tasks_shows_stored_base_branch_not_current_head() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    // Create and switch to a feature branch
    env.git_create_branch("feature-branch")?;
    env.git_checkout("feature-branch")?;

    // Create a task while on the feature branch
    let (id, slug) = env.new_task("my-task", &[])?;

    // Verify task file contains feature-branch as base_branch
    let task_file = env.task_file_path(id, &slug);
    let data = std::fs::read_to_string(&task_file)?;
    assert!(
      data.contains("base_branch: feature-branch"),
      "task frontmatter should contain base_branch: feature-branch, got:\n{data}"
    );

    // Switch back to main
    env.git_checkout("main")?;

    // Run `agency tasks` and verify BASE column still shows feature-branch
    let output = env.agency()?.arg("tasks").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
      stdout.contains("feature-branch"),
      "tasks output should show feature-branch as BASE, got:\n{stdout}"
    );
    assert!(
      !stdout.contains(" main "),
      "tasks output should NOT show main as BASE for the task, got:\n{stdout}"
    );

    Ok(())
  })
}
