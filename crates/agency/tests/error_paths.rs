mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;
use std::fs;

#[test]
fn global_config_with_invalid_toml() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.write_xdg_config("agency/agency.toml", "[agents.test]\ncmd = [1, 2,")?;
    env.init_repo()?;

    env
      .agency()?
      .arg("tasks")
      .assert()
      .failure()
      .stderr(predicate::str::contains("invalid TOML").from_utf8());

    Ok(())
  })
}

#[test]
fn project_config_with_type_mismatch() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let agency_dir = env.path().join(".agency");
    fs::create_dir_all(&agency_dir)?;
    fs::write(
      agency_dir.join("agency.toml"),
      "[agents.test]\ncmd = \"string-instead-of-array\"",
    )?;

    env
      .agency()?
      .arg("tasks")
      .assert()
      .failure()
      .stderr(predicate::str::contains("parse").from_utf8());

    Ok(())
  })
}

#[test]
fn bootstrap_non_existent_task_fails() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("bootstrap")
      .arg("999")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}

#[test]
fn open_non_existent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("open")
      .arg("nonexistent")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}

#[test]
fn rm_non_existent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("rm")
      .arg("nonexistent-slug")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}

#[test]
fn merge_non_existent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("merge")
      .arg("999")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}

#[test]
fn reset_non_existent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("reset")
      .arg("nonexistent")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}

#[test]
fn complete_non_existent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("complete")
      .arg("nonexistent")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}

#[test]
fn new_with_empty_slug_after_normalization() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("new")
      .arg("--draft")
      .arg("___")
      .assert()
      .failure()
      .stderr(predicate::str::contains("invalid slug").from_utf8());

    Ok(())
  })
}

#[test]
fn new_with_only_special_chars() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("new")
      .arg("--draft")
      .arg("***")
      .assert()
      .failure()
      .stderr(predicate::str::contains("invalid slug").from_utf8());

    Ok(())
  })
}


#[test]
fn attach_without_tty() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("test-task", &[])?;
    env.bootstrap_task(id)?;

    env
      .agency()?
      .arg("attach")
      .arg(id.to_string())
      .assert()
      .failure()
      .stderr(predicate::str::contains("requires").from_utf8());

    Ok(())
  })
}

#[test]
fn tui_without_tty() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("tui")
      .assert()
      .success()
      .stdout(predicate::str::contains("requires a TTY").from_utf8());

    Ok(())
  })
}

#[test]
fn edit_non_existent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("edit")
      .arg("nonexistent")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}


#[test]
fn init_non_interactive_with_yes_flag() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env
      .agency()?
      .arg("init")
      .arg("--yes")
      .assert()
      .success();

    let agency_dir = env.path().join(".agency");
    assert!(agency_dir.join("agency.toml").exists());

    Ok(())
  })
}

#[test]
fn rm_non_interactive_with_yes_flag() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("test-rm", &[])?;
    env.bootstrap_task(id)?;

    env
      .agency()?
      .arg("rm")
      .arg(id.to_string())
      .arg("--yes")
      .assert()
      .success();

    Ok(())
  })
}

#[test]
fn task_file_with_corrupt_frontmatter() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let tasks_dir = env.path().join(".agency").join("tasks");
    fs::create_dir_all(&tasks_dir)?;

    fs::write(
      tasks_dir.join("1-corrupt.md"),
      "---\nagent: [unclosed\n---\n\nBody content",
    )?;

    let output = env.agency()?.arg("tasks").output()?;

    assert!(
      output.status.success(),
      "tasks command should succeed even with corrupt frontmatter"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
      stdout.contains("corrupt"),
      "corrupt task should still be listed"
    );

    Ok(())
  })
}

#[test]
fn defaults_show_with_no_project_config() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env.agency()?.arg("defaults").assert().success();

    Ok(())
  })
}

#[test]
fn path_non_existent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("path")
      .arg("nonexistent")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}

#[test]
fn branch_non_existent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("branch")
      .arg("nonexistent")
      .assert()
      .failure()
      .stderr(predicate::str::contains("not found").from_utf8());

    Ok(())
  })
}

#[test]
fn info_requires_task_context() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("info")
      .assert()
      .failure()
      .stderr(predicate::str::contains("agency context").from_utf8());

    Ok(())
  })
}
