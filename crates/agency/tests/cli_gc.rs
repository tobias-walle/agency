mod common;

use anyhow::Result;

#[test]
fn gc_removes_orphans_safely() -> Result<()> {
  common::test_env::TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, slug) = env.new_task("alpha", &["--draft"])?;
    env.bootstrap_task(id)?;

    env.git_new_branch(98, "orphan")?;
    env.git_add_worktree(99, "ghost")?;

    assert!(env.branch_exists(98, "orphan")?);
    assert!(env.git_worktree_exists(99, "ghost"));
    assert!(env.branch_exists(id, &slug)?);
    assert!(env.git_worktree_exists(id, &slug));

    let output = env.agency_gc()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
      stdout.contains("Garbage collected"),
      "gc should report garbage collection in stdout"
    );

    assert!(!env.branch_exists(98, "orphan")?);
    assert!(!env.git_worktree_exists(99, "ghost"));

    let valid_wt = env.worktree_dir_path(id, &slug);
    assert!(env.branch_exists(id, &slug)?);
    assert!(valid_wt.is_dir());

    Ok(())
  })
}
