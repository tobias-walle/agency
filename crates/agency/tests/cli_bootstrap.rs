mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use temp_env::with_vars;

#[test]
fn new_bootstraps_git_ignored_root_files_with_defaults() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let gi = env.path().join(".gitignore");
    std::fs::write(
      &gi,
      ".env\n.env.local\nsecrets.txt\n.venv/\n.direnv/\nbig.bin\n",
    )?;

    env.write_file(".env", "KEY=VALUE\n")?;
    env.write_file(".env.local", "LOCAL=1\n")?;
    env.write_file("secrets.txt", "secret\n")?;
    let big = env.path().join("big.bin");
    let file = std::fs::File::create(&big)?;
    file.set_len(10 * 1024 * 1024 + 1)?;
    env.write_file(".venv/pkg.txt", "x\n")?;
    env.write_file(".direnv/env.txt", "x\n")?;

    let (id, slug) = env.new_task("bootstrap-a", &[])?;
    env.bootstrap_task(id)?;
    let wt = env.worktree_dir_path(id, &slug);

    assert!(wt.join(".env").is_file());
    assert!(wt.join(".env.local").is_file());
    assert!(wt.join("secrets.txt").is_file());
    assert!(!wt.join("big.bin").exists());
    assert!(!wt.join(".venv").exists());
    assert!(!wt.join(".direnv").exists());
    // Root's .agency config should not be copied (.agency/local is allowed for worktree-local files)
    assert!(!wt.join(".agency/agency.toml").exists());

    Ok(())
  })
}

#[test]
fn new_bootstrap_respects_config_includes_and_excludes() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    std::fs::write(
      env.path().join(".gitignore"),
      ".env\n.env.local\nsecrets.txt\n.venv/\n.direnv/\n",
    )?;

    std::fs::write(env.path().join(".env"), "KEY=VALUE\n")?;
    std::fs::write(env.path().join(".env.local"), "LOCAL=1\n")?;
    std::fs::write(env.path().join("secrets.txt"), "secret\n")?;
    std::fs::create_dir_all(env.path().join(".venv"))?;
    std::fs::write(env.path().join(".venv").join("pkg.txt"), "x\n")?;
    std::fs::create_dir_all(env.path().join(".direnv"))?;
    std::fs::write(env.path().join(".direnv").join("env.txt"), "x\n")?;

    let proj_cfg_dir = env.path().join(".agency");
    std::fs::create_dir_all(&proj_cfg_dir)?;
    std::fs::write(
      proj_cfg_dir.join("agency.toml"),
      "[bootstrap]\ninclude=[\".venv\"]\n",
    )?;

    let xdg_root = common::test_env::tmp_root().join("xdg-config");
    let agency_dir = xdg_root.join("agency");
    std::fs::create_dir_all(&agency_dir)?;
    std::fs::write(
      agency_dir.join("agency.toml"),
      "[bootstrap]\ninclude=[\".direnv\"]\nexclude=[\".env.local\"]\n",
    )?;

    with_vars(
      [("XDG_CONFIG_HOME", Some(xdg_root.display().to_string()))],
      || {
        let (id, slug) = env.new_task("bootstrap-b", &[]).unwrap();
        env.bootstrap_task(id).unwrap();
        let wt = env.worktree_dir_path(id, &slug);
        assert!(wt.join(".env").is_file());
        assert!(wt.join("secrets.txt").is_file());
        assert!(wt.join(".venv").is_dir());
        assert!(wt.join(".venv").join("pkg.txt").is_file());
        assert!(wt.join(".direnv").is_dir());
        assert!(wt.join(".direnv").join("env.txt").is_file());
        assert!(!wt.join(".env.local").exists());
        // Root's .agency config should not be copied (.agency/local is allowed for worktree-local files)
        assert!(!wt.join(".agency/agency.toml").exists());
      },
    );

    Ok(())
  })
}
