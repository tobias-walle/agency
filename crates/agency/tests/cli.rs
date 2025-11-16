mod common;

use anyhow::Result;
use gix as git;
use predicates::prelude::*;
use std::path::Path;
use temp_env::with_vars;

fn write_executable_script(path: &Path, body: &str) -> Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::write(path, body)?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
  }
  Ok(())
}

#[test]
fn gc_removes_orphans_safely() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Create a valid task and prepare its branch/worktree
  let (id, slug) = env.new_task("alpha", &["--draft"])?;
  env.bootstrap_task(id)?;

  // Resolve HEAD commit via git process for simplicity
  let out = std::process::Command::new("git")
    .current_dir(env.path())
    .args(["rev-parse", "HEAD"])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .output()?;
  assert!(out.status.success(), "git rev-parse HEAD should succeed");
  let head_hex = String::from_utf8_lossy(&out.stdout).trim().to_string();
  let head = gix::ObjectId::from_hex(head_hex.as_bytes()).expect("valid hex id");

  // Orphan branch WITHOUT worktree: agency/98-orphan at current HEAD
  let repo = git::discover(env.path())?;
  let _r = repo.reference(
    "refs/heads/agency/98-orphan",
    head,
    gix::refs::transaction::PreviousValue::MustNotExist,
    "create orphan branch",
  )?;

  // Orphan worktree linked to a different orphan branch
  let orphan_wt = env.worktree_dir_path(99, "ghost");
  {
    // Create the branch to link the worktree against
    let _r2 = repo.reference(
      "refs/heads/agency/99-ghost",
      head,
      gix::refs::transaction::PreviousValue::MustNotExist,
      "create orphan branch for worktree",
    )?;
    let status = std::process::Command::new("git")
      .current_dir(env.path())
      .args([
        "worktree",
        "add",
        "--quiet",
        orphan_wt.display().to_string().as_str(),
        "agency/99-ghost",
      ])
      .status()?;
    assert!(status.success(), "git worktree add should succeed");
  }

  // Sanity: orphan branch and orphan worktree exist; valid ones exist too
  {
    let full_orphan = format!("refs/heads/{}", env.branch_name(98, "orphan"));
    assert!(repo.find_reference(&full_orphan).is_ok());
    assert!(orphan_wt.is_dir());

    let full_valid = format!("refs/heads/{}", env.branch_name(id, &slug));
    assert!(repo.find_reference(&full_valid).is_ok());
    let valid_wt = env.worktree_dir_path(id, &slug);
    assert!(valid_wt.is_dir());
  }

  // Run gc
  {
    let mut cmd = env.bin_cmd()?;
    cmd.arg("gc");
    cmd
      .assert()
      .success()
      .stdout(predicates::str::contains("Garbage collected").from_utf8());
  }

  // Orphans should be removed safely: orphan worktree pruned, orphan branch without worktree deleted
  {
    let full_orphan = format!("refs/heads/{}", env.branch_name(98, "orphan"));
    assert!(repo.find_reference(&full_orphan).is_err());
    assert!(!orphan_wt.exists());

    let full_valid = format!("refs/heads/{}", env.branch_name(id, &slug));
    assert!(repo.find_reference(&full_valid).is_ok());
    let valid_wt = env.worktree_dir_path(id, &slug);
    assert!(valid_wt.is_dir());
  }

  Ok(())
}

#[test]
fn setup_creates_global_config_via_wizard() -> Result<()> {
  let env = common::TestEnv::new();
  let xdg_temp = common::tempdir_in_sandbox();
  let config_home = xdg_temp.path().to_path_buf();
  let bin_dir = config_home.join("bin");
  write_executable_script(&bin_dir.join("claude"), "#!/usr/bin/env bash\nexit 0\n")?;

  with_vars(
    [
      ("XDG_CONFIG_HOME", Some(config_home.display().to_string())),
      ("PATH", Some(bin_dir.display().to_string())),
    ],
    || {
      let mut cmd = env.bin_cmd().unwrap();
      cmd.arg("setup").write_stdin("claude\n\n\n");
      cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("agency defaults").from_utf8())
        .stdout(predicates::str::contains("agency init").from_utf8());
    },
  );

  let cfg_file = config_home.join("agency").join("agency.toml");
  assert!(
    cfg_file.is_file(),
    "setup should write global config at {}",
    cfg_file.display()
  );
  let data = std::fs::read_to_string(&cfg_file)?;
  assert!(
    data.contains("agent = \"claude\""),
    "config should record selected agent: {data}"
  );
  assert!(
    !data.contains("keybindings"),
    "no keybinding overrides present by default: {data}"
  );
  Ok(())
}

#[test]
fn setup_updates_existing_config_and_warns() -> Result<()> {
  let env = common::TestEnv::new();
  let xdg_temp = common::tempdir_in_sandbox();
  let config_home = xdg_temp.path().to_path_buf();
  let agency_dir = config_home.join("agency");
  std::fs::create_dir_all(&agency_dir)?;
  let cfg = agency_dir.join("agency.toml");
  std::fs::write(
    &cfg,
    r#"agent = "claude"
[bootstrap]
include = ["scripts"]
"#,
  )?;

  with_vars(
    [("XDG_CONFIG_HOME", Some(config_home.display().to_string()))],
    || {
      let mut cmd = env.bin_cmd().unwrap();
      cmd.arg("setup").write_stdin("opencode\nzsh\n\n");
      cmd
        .assert()
        .success()
        .stdout(predicates::str::contains("existing config").from_utf8());
    },
  );

  let data = std::fs::read_to_string(&cfg)?;
  assert!(
    data.contains("agent = \"opencode\""),
    "agent should be updated: {data}"
  );
  assert!(
    data.contains("shell = [\"zsh\"]"),
    "shell command override should be persisted: {data}"
  );
  assert!(
    data.contains("[bootstrap]"),
    "unrelated keys must be preserved when rewriting config: {data}"
  );
  Ok(())
}

#[test]
fn defaults_prints_embedded_config() -> Result<()> {
  let env = common::TestEnv::new();
  let mut cmd = env.bin_cmd()?;
  cmd.arg("defaults");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains("Embedded agency defaults").from_utf8())
    .stdout(predicates::str::contains("[agents.claude]").from_utf8());
  Ok(())
}

#[test]
fn ps_autostarts_daemon_when_missing() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  if !env.sockets_available() {
    eprintln!("Skipping ps_autostarts_daemon_when_missing: Unix sockets not available in sandbox");
    return Ok(());
  }

  // Run tasks without starting daemon; autostart should kick in
  let mut cmd = env.bin_cmd()?;
  cmd.arg("tasks");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains("ID SLUG").from_utf8());

  // Stop daemon to clean up
  let mut daemon_stop = env.bin_cmd()?;
  daemon_stop.arg("daemon").arg("stop");
  daemon_stop.assert().success();

  Ok(())
}

#[test]
fn daemon_reports_version_via_protocol() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  if !env.sockets_available() {
    eprintln!(
      "Skipping daemon_reports_version_via_protocol: Unix sockets not available in sandbox"
    );
    return Ok(());
  }

  // Ensure daemon is running
  let mut daemon_start = env.bin_cmd()?;
  daemon_start.arg("daemon").arg("start");
  daemon_start.assert().success();

  // Connect and request version
  with_vars(
    [(
      "XDG_RUNTIME_DIR",
      Some(env.runtime_dir().display().to_string()),
    )],
    || -> Result<()> {
      let cwd = env.path().to_path_buf();
      let cfg = agency::config::load_config(&cwd)?;
      let socket = agency::config::compute_socket_path(&cfg);
      let mut stream = std::os::unix::net::UnixStream::connect(&socket)?;
      agency::daemon_protocol::write_frame(
        &mut stream,
        &agency::daemon_protocol::C2D::Control(agency::daemon_protocol::C2DControl::GetVersion),
      )?;
      let reply: agency::daemon_protocol::D2C = agency::daemon_protocol::read_frame(&mut stream)?;
      match reply {
        agency::daemon_protocol::D2C::Control(agency::daemon_protocol::D2CControl::Version {
          version,
        }) => {
          let cli_version = env!("CARGO_PKG_VERSION");
          assert_eq!(
            version, cli_version,
            "daemon version must match CLI version",
          );
        }
        other @ agency::daemon_protocol::D2C::Control(_) => panic!("unexpected reply: {other:?}"),
      }
      Ok(())
    },
  )?;

  let mut daemon_stop = env.bin_cmd()?;
  daemon_stop.arg("daemon").arg("stop");
  daemon_stop.assert().success();

  Ok(())
}

#[test]
fn init_scaffolds_files_after_confirmation() -> Result<()> {
  let env = common::TestEnv::new();
  let root = env.path().to_path_buf();
  let mut cmd = env.bin_cmd()?;
  cmd.arg("init").write_stdin("y\n");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(".agency/agency.toml").from_utf8())
    .stdout(predicates::str::contains(".agency/setup.sh").from_utf8())
    .stdout(predicates::str::contains(".gitignore").from_utf8());

  let agency_dir = root.join(".agency");
  assert!(agency_dir.is_dir(), ".agency directory should be created");
  let cfg = agency_dir.join("agency.toml");
  assert!(cfg.is_file(), "empty agency.toml should be created");
  let script = agency_dir.join("setup.sh");
  assert!(script.is_file(), "bootstrap script should be created");
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let perms = std::fs::metadata(&script)?.permissions();
    assert!(
      perms.mode() & 0o111 != 0,
      "setup.sh should be executable: mode {:o}",
      perms.mode()
    );
  }
  let script_body = std::fs::read_to_string(&script)?;
  assert!(
    script_body.contains("#!/usr/bin/env bash"),
    "script should include shebang for editing: {script_body}"
  );
  Ok(())
}

#[test]
fn init_appends_gitignore_when_missing() -> Result<()> {
  let env = common::TestEnv::new();
  let gi = env.path().join(".gitignore");
  assert!(!gi.exists(), ".gitignore should not exist before init");

  let mut cmd = env.bin_cmd()?;
  cmd.arg("init").write_stdin("y\n");
  cmd.assert().success();

  let contents = std::fs::read_to_string(&gi)?;
  assert_eq!(
    contents, ".agency/*\n!.agency/setup.sh\n",
    ".gitignore should contain agency entries"
  );
  Ok(())
}

#[test]
fn init_skips_gitignore_when_agency_entry_exists() -> Result<()> {
  let env = common::TestEnv::new();
  let gi = env.path().join(".gitignore");
  std::fs::write(&gi, "# existing\n.agency\n")?;

  let mut cmd = env.bin_cmd()?;
  cmd.arg("init").write_stdin("y\n");
  cmd.assert().success();

  let contents = std::fs::read_to_string(&gi)?;
  assert_eq!(
    contents, "# existing\n.agency\n",
    ".gitignore should remain unchanged when .agency entry exists"
  );
  Ok(())
}

#[test]
fn new_creates_markdown_branch_and_worktree() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Create task
  let (id, slug) = env.new_task("alpha-task", &[])?;

  // Check markdown
  let file = env.task_file_path(id, &slug);
  assert!(
    file.is_file(),
    "task file should exist at {}",
    file.display()
  );

  // Lazy worktrees: branch and worktree are created on attach
  assert!(!env.branch_exists(id, &slug)?);
  let wt_dir = env.worktree_dir_path(id, &slug);
  assert!(!wt_dir.exists());

  // Prepare worktree via bootstrap command
  env.bootstrap_task(id)?;
  assert!(env.branch_exists(id, &slug)?);
  assert!(wt_dir.is_dir());

  Ok(())
}

#[test]
fn new_persists_description_when_provided() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Provide explicit description and keep as draft (no attach)
  let (id, slug) = env.new_task(
    "desc-task",
    &["--draft", "--description", "Automated test body"],
  )?;
  let file = env.task_file_path(id, &slug);
  let data = std::fs::read_to_string(&file)?;
  assert!(data.contains("Automated test body"));
  Ok(())
}

#[test]
fn new_accepts_draft_flag() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Run without helper to ensure the flag is accepted
  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--draft").arg("epsilon-task");
  cmd.assert().success();

  Ok(())
}

#[test]
fn new_runs_default_bootstrap_cmd_when_present() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Write default bootstrap script at <root>/.agency/setup.sh
  let agen_dir = env.path().join(".agency");
  std::fs::create_dir_all(&agen_dir)?;
  let script = agen_dir.join("setup.sh");
  std::fs::write(
    &script,
    "#!/usr/bin/env bash\n\n echo bootstrap-script-output && echo ok > boot.out\n",
  )?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(&script)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms)?;
  }

  let (id, slug) = env.new_task("boot-cmd", &["--draft"])?;
  // Prepare and run bootstrap without PTY
  env.bootstrap_task(id)?;
  let wt = env.worktree_dir_path(id, &slug);
  assert!(wt.join("boot.out").is_file());

  Ok(())
}

#[test]
fn new_skips_default_bootstrap_when_missing() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  // Ensure no .agency/setup.sh exists
  std::fs::create_dir_all(env.path().join(".agency"))?;

  let (id, slug) = env.new_task("no-boot-script", &[])?;
  // Prepare worktree without daemon/PTY
  env.bootstrap_task(id)?;
  let wt = env.worktree_dir_path(id, &slug);
  assert!(!wt.join("boot.out").exists());
  Ok(())
}

#[test]
fn new_supports_placeholder_root_in_bootstrap_cmd() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Project config with custom bootstrap cmd using <root>
  let agen_dir = env.path().join(".agency");
  std::fs::create_dir_all(&agen_dir)?;
  std::fs::write(
    agen_dir.join("agency.toml"),
    "[bootstrap]\ncmd=[\"bash\",\"-lc\",\"echo <root> > root.txt\"]\n",
  )?;

  let (id, slug) = env.new_task("boot-root", &[])?;
  // Bootstrap to run configured command
  env.bootstrap_task(id)?;
  let wt = env.worktree_dir_path(id, &slug);
  let data = std::fs::read_to_string(wt.join("root.txt"))?;
  let expect_root = env
    .path()
    .canonicalize()
    .unwrap_or_else(|_| env.path().to_path_buf())
    .display()
    .to_string();
  assert_eq!(data.trim(), expect_root);
  Ok(())
}

#[test]
fn attach_follow_conflicts_with_task_and_session() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  // Prevent autostart to avoid side effects
  with_vars([("AGENCY_NO_AUTOSTART", Some("1".to_string()))], || {
    // Conflicts: task positional with --follow
    let mut cmd = env.bin_cmd().unwrap();
    cmd.arg("attach").arg("123").arg("--follow");
    cmd.assert().failure();

    // Conflicts: --session with --follow
    let mut cmd2 = env.bin_cmd().unwrap();
    cmd2
      .arg("attach")
      .arg("--session")
      .arg("99")
      .arg("--follow");
    cmd2.assert().failure();
  });
  Ok(())
}

#[test]
fn new_writes_yaml_header_when_agent_specified() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("alpha-task", &["-a", "sh", "--description", ""])?;
  // Check markdown content includes YAML front matter
  let file = env.task_file_path(id, &slug);
  let data = std::fs::read_to_string(&file)?;
  assert!(
    data.starts_with("---\n"),
    "file should start with YAML '---' block"
  );
  assert!(
    data.contains("agent: sh\n"),
    "front matter should contain agent: sh"
  );
  assert!(
    data.contains("base_branch: main\n"),
    "front matter should contain base_branch: main"
  );
  assert_eq!(
    data, "---\nagent: sh\nbase_branch: main\n---\n",
    "task should contain only front matter when no description"
  );

  Ok(())
}

#[test]
fn path_prints_absolute_worktree_path_by_id_and_slug() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("beta-task", &[])?;

  let expected = env.worktree_dir_path(id, &slug);
  let expected_canon = expected.canonicalize().unwrap_or(expected.clone());
  let expected_str = expected_canon.display().to_string() + "\n";

  // path by id
  let mut cmd = env.bin_cmd()?;
  cmd.arg("path").arg(id.to_string());
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(expected_str.clone()).from_utf8());

  // path by slug
  let mut cmd = env.bin_cmd()?;
  cmd.arg("path").arg(&slug);
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(expected_str).from_utf8());

  Ok(())
}

#[test]
fn branch_prints_branch_name_by_id_and_slug() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("gamma-task", &[])?;

  // by id
  let mut cmd = env.bin_cmd()?;
  cmd.arg("branch").arg(id.to_string());
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(env.branch_name(id, &slug)).from_utf8());

  // by slug
  let mut cmd = env.bin_cmd()?;
  cmd.arg("branch").arg(&slug);
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(env.branch_name(id, &slug)).from_utf8());

  Ok(())
}

#[test]
fn rm_confirms_and_removes_on_y_or_y() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("delta-task", &[])?;

  // Ensure branch/worktree exist for this test
  env.bootstrap_task(id)?;

  // Run rm and cancel (pipe stdin via assert_cmd)
  let mut cmd = env.bin_cmd()?;
  cmd
    .arg("rm")
    .arg(id.to_string())
    .write_stdin("n\n")
    .assert()
    .success()
    .stdout(predicates::str::contains("Cancelled").from_utf8());

  // Ensure still present
  let repo = git::discover(env.path())?;
  let full = format!("refs/heads/{}", env.branch_name(id, &slug));
  assert!(repo.find_reference(&full).is_ok());
  assert!(env.task_file_path(id, &slug).is_file());
  assert!(env.worktree_dir_path(id, &slug).is_dir());

  // Run rm and confirm with Y (pipe stdin via assert_cmd)
  let mut cmd = env.bin_cmd()?;
  cmd
    .arg("rm")
    .arg(&slug)
    .write_stdin("Y\n")
    .assert()
    .success()
    .stdout(predicates::str::contains("Removed task, branch, and worktree").from_utf8());

  // Verify removal
  let repo = git::discover(env.path())?;
  let full = format!("refs/heads/{}", env.branch_name(id, &slug));
  assert!(repo.find_reference(&full).is_err());
  assert!(!env.task_file_path(id, &slug).exists());
  assert!(!env.worktree_dir_path(id, &slug).exists());

  Ok(())
}

#[test]
fn new_rejects_slugs_starting_with_digits() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("1invalid");
  cmd
    .assert()
    .failure()
    .stderr(predicates::str::contains("invalid slug: must start with a letter").from_utf8());

  Ok(())
}

#[test]
fn new_auto_suffixes_duplicate_slug_to_slug2() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (_id1, _slug1) = env.new_task("alpha", &[])?;
  let (id2, slug2) = env.new_task("alpha", &[])?;

  // Check file for the second task (branch/worktree are created on attach)
  let file = env.task_file_path(id2, &slug2);
  assert!(file.is_file());

  Ok(())
}

#[test]
fn new_increments_trailing_number_slug() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (_id1, _slug1) = env.new_task("alpha2", &[])?;
  let (id2, slug2) = env.new_task("alpha2", &[])?;

  // Check artifacts for the second task (branch/worktree are created on attach)
  let file = env.task_file_path(id2, &slug2);
  assert!(file.is_file());

  Ok(())
}

#[test]
fn ps_lists_id_and_slug_in_order() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  if !env.sockets_available() {
    eprintln!("Skipping ps_lists_id_and_slug_in_order: Unix sockets not available in sandbox");
    return Ok(());
  }
  let (id1, slug1) = env.new_task("alpha-task", &[])?;
  let (id2, slug2) = env.new_task("beta-task", &[])?;

  let mut daemon_start = env.bin_cmd()?;
  daemon_start.arg("daemon").arg("start");
  daemon_start.assert().success();

  // Run tasks
  let mut cmd = env.bin_cmd()?;
  cmd.arg("tasks");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains("ID SLUG").from_utf8())
    .stdout(
      predicates::str::is_match(r"STATUS +UNCOMMITTED +COMMITS +BASE +AGENT[^\n]*\n")
        .unwrap()
        .from_utf8(),
    )
    .stdout(
      predicates::str::is_match(format!(r"\b{}\s+{}\b", id1, regex::escape(&slug1)))
        .unwrap()
        .from_utf8(),
    )
    .stdout(
      predicates::str::is_match(format!(r"\b{}\s+{}\b", id2, regex::escape(&slug2)))
        .unwrap()
        .from_utf8(),
    )
    .stdout(predicates::str::contains("Draft").from_utf8());

  let mut daemon_stop = env.bin_cmd()?;
  daemon_stop.arg("daemon").arg("stop");
  daemon_stop.assert().success();

  Ok(())
}

#[test]
fn ps_handles_empty_state() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  if !env.sockets_available() {
    eprintln!("Skipping ps_handles_empty_state: Unix sockets not available in sandbox");
    return Ok(());
  }

  let mut daemon_start = env.bin_cmd()?;
  daemon_start.arg("daemon").arg("start");
  daemon_start.assert().success();

  // Run tasks with no tasks
  let mut cmd = env.bin_cmd()?;
  cmd.arg("tasks");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains("ID SLUG").from_utf8())
    .stdout(
      predicates::str::is_match(r"STATUS +UNCOMMITTED +COMMITS +BASE +AGENT.*\n")
        .unwrap()
        .from_utf8(),
    );

  let mut daemon_stop = env.bin_cmd()?;
  daemon_stop.arg("daemon").arg("stop");
  daemon_stop.assert().success();

  Ok(())
}

#[test]
fn ps_bails_when_daemon_not_running() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  with_vars([("AGENCY_NO_AUTOSTART", Some("1".to_string()))], || {
    let mut cmd = env.bin_cmd().unwrap();
    cmd.arg("tasks");
    cmd.assert().success().stdout(
      predicates::str::contains("ID SLUG STATUS UNCOMMITTED COMMITS BASE AGENT").from_utf8(),
    );
  });

  Ok(())
}

#[test]
fn sessions_bails_when_daemon_not_running() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  with_vars([("AGENCY_NO_AUTOSTART", Some("1".to_string()))], || {
    let mut cmd = env.bin_cmd().unwrap();
    cmd.arg("sessions");
    cmd.assert().failure().stderr(
      predicates::str::contains("Daemon not running. Please start it with `agency daemon start`")
        .from_utf8(),
    );
  });

  Ok(())
}

#[test]
fn new_bootstraps_git_ignored_root_files_with_defaults() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Create .gitignore at repo root
  let gi = env.path().join(".gitignore");
  std::fs::write(
    &gi,
    ".env\n.env.local\nsecrets.txt\n.venv/\n.direnv/\nbig.bin\n",
  )?;

  // Create ignored root files and dirs
  std::fs::write(env.path().join(".env"), "KEY=VALUE\n")?;
  std::fs::write(env.path().join(".env.local"), "LOCAL=1\n")?;
  std::fs::write(env.path().join("secrets.txt"), "secret\n")?;
  // big.bin >= 10MB
  let big = env.path().join("big.bin");
  let f = std::fs::File::create(&big)?;
  f.set_len(10 * 1024 * 1024 + 1)?;
  // dirs
  std::fs::create_dir_all(env.path().join(".venv"))?;
  std::fs::write(env.path().join(".venv").join("pkg.txt"), "x\n")?;
  std::fs::create_dir_all(env.path().join(".direnv"))?;
  std::fs::write(env.path().join(".direnv").join("env.txt"), "x\n")?;

  // Create task and bootstrap to populate worktree
  let (id, slug) = env.new_task("bootstrap-a", &[])?;
  env.bootstrap_task(id)?;
  let wt = env.worktree_dir_path(id, &slug);

  // Files present (<=10MB)
  assert!(wt.join(".env").is_file());
  assert!(wt.join(".env.local").is_file());
  assert!(wt.join("secrets.txt").is_file());
  // Excluded by size; dirs are not copied unless included
  assert!(!wt.join("big.bin").exists());
  assert!(!wt.join(".venv").exists());
  assert!(!wt.join(".direnv").exists());
  // Always excluded from copying
  assert!(!wt.join(".agency").exists());

  Ok(())
}

#[test]
fn new_bootstrap_respects_config_includes_and_excludes() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // .gitignore
  std::fs::write(
    env.path().join(".gitignore"),
    ".env\n.env.local\nsecrets.txt\n.venv/\n.direnv/\n",
  )?;
  // Files/dirs
  std::fs::write(env.path().join(".env"), "KEY=VALUE\n")?;
  std::fs::write(env.path().join(".env.local"), "LOCAL=1\n")?;
  std::fs::write(env.path().join("secrets.txt"), "secret\n")?;
  std::fs::create_dir_all(env.path().join(".venv"))?;
  std::fs::write(env.path().join(".venv").join("pkg.txt"), "x\n")?;
  std::fs::create_dir_all(env.path().join(".direnv"))?;
  std::fs::write(env.path().join(".direnv").join("env.txt"), "x\n")?;

  // Project config: include .venv
  let proj_cfg_dir = env.path().join(".agency");
  std::fs::create_dir_all(&proj_cfg_dir)?;
  std::fs::write(
    proj_cfg_dir.join("agency.toml"),
    "[bootstrap]\ninclude=[\".venv\"]\n",
  )?;

  // XDG config: include .direnv, exclude .env.local
  let xdg_root = common::tmp_root().join("xdg-config");
  let agency_dir = xdg_root.join("agency");
  std::fs::create_dir_all(&agency_dir)?;
  std::fs::write(
    agency_dir.join("agency.toml"),
    "[bootstrap]\ninclude=[\".direnv\"]\nexclude=[\".env.local\"]\n",
  )?;

  // Scope XDG path only for this call
  with_vars(
    [("XDG_CONFIG_HOME", Some(xdg_root.display().to_string()))],
    || {
      let (id, slug) = env.new_task("bootstrap-b", &[]).unwrap();
      // Bootstrap to create worktree and run command
      env.bootstrap_task(id).unwrap();
      let wt = env.worktree_dir_path(id, &slug);
      assert!(wt.join(".env").is_file());
      assert!(wt.join("secrets.txt").is_file());
      assert!(wt.join(".venv").is_dir());
      assert!(wt.join(".venv").join("pkg.txt").is_file());
      assert!(wt.join(".direnv").is_dir());
      assert!(wt.join(".direnv").join("env.txt").is_file());
      // excluded via XDG override
      assert!(!wt.join(".env.local").exists());
      // always excluded from copying
      assert!(!wt.join(".agency").exists());
    },
  );

  Ok(())
}

#[test]
fn open_opens_worktree_via_editor() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, _slug) = env.new_task("open-task", &["--draft"])?;

  with_vars([("EDITOR", Some("true".to_string()))], || {
    let mut cmd = env.bin_cmd().unwrap();
    cmd.arg("open").arg(id.to_string());
    cmd.assert().success();
  });

  Ok(())
}

#[test]
fn complete_marks_status_completed_and_uses_env() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Create a task
  let (id, slug) = env.new_task("complete-a", &["--draft"])?;

  // Mark complete by explicit id
  {
    let mut cmd = env.bin_cmd()?;
    cmd.arg("complete").arg(id.to_string());
    cmd.assert().success();
  }

  // Verify completed flag file exists
  {
    let flag = env
      .path()
      .join(".agency")
      .join("state")
      .join("completed")
      .join(format!("{id}-{slug}"));
    assert!(
      flag.is_file(),
      "completed flag should exist at {}",
      flag.display()
    );
  }

  // Create another task and mark complete via env var
  let (id2, slug2) = env.new_task("complete-b", &["--draft"])?;
  {
    let mut cmd = env.bin_cmd()?;
    cmd.arg("complete");
    cmd.env("AGENCY_TASK_ID", id2.to_string());
    cmd.assert().success();
  }
  // Verify completed flag exists for second task
  {
    let flag2 = env
      .path()
      .join(".agency")
      .join("state")
      .join("completed")
      .join(format!("{id2}-{slug2}"));
    assert!(
      flag2.is_file(),
      "completed flag should exist at {}",
      flag2.display()
    );
  }

  Ok(())
}

#[test]
fn reset_clears_completed_status() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("complete-reset", &["--draft"])?;

  // Mark complete
  {
    let mut cmd = env.bin_cmd()?;
    cmd.arg("complete").arg(id.to_string());
    cmd.assert().success();
  }

  // Reset should clear the flag
  {
    let mut cmd = env.bin_cmd()?;
    cmd.arg("reset").arg(id.to_string());
    cmd.assert().success();
  }

  // Verify completed flag cleared
  {
    let flag = env
      .path()
      .join(".agency")
      .join("state")
      .join("completed")
      .join(format!("{id}-{slug}"));
    assert!(
      !flag.exists(),
      "completed flag should be removed after reset: {}",
      flag.display()
    );
  }

  Ok(())
}

#[test]
fn merge_fast_forwards_and_cleans_up() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("merge-task", &["--draft"])?;
  // Prepare branch/worktree for merge
  env.bootstrap_task(id)?;

  // Create a new commit on the task branch (empty tree commit)
  let repo = git::discover(env.path())?;
  let empty_tree = git::ObjectId::empty_tree(repo.object_hash());
  let head = repo.head_commit()?;
  let parent_id = head.id();
  let task_ref = format!("refs/heads/{}", env.branch_name(id, &slug));
  let new_id = repo.commit(task_ref.as_str(), "test", empty_tree, [parent_id])?;

  // Merge back to base (main) and clean up
  let mut cmd = env.bin_cmd()?;
  cmd.arg("merge").arg(id.to_string());
  cmd.assert().success();

  // Verify base advanced to new commit
  let main_ref = repo.find_reference("refs/heads/main")?;
  let tgt = main_ref.target();
  let main_target = tgt.try_id().expect("id");
  let main_hex = main_target.to_hex().to_string();
  let new_hex = new_id.to_hex().to_string();
  assert_eq!(main_hex, new_hex);

  // Verify cleanup
  assert!(!env.branch_exists(id, &slug)?);
  assert!(!env.task_file_path(id, &slug).exists());
  assert!(!env.worktree_dir_path(id, &slug).exists());

  Ok(())
}

#[test]
fn merge_stashes_and_restores_dirty_base() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("merge-dirty", &["--draft"])?;
  // Prepare branch/worktree
  env.bootstrap_task(id)?;

  // Create a new commit on the task branch (empty tree commit)
  let repo = git::discover(env.path())?;
  let empty_tree = git::ObjectId::empty_tree(repo.object_hash());
  let head = repo.head_commit()?;
  let parent_id = head.id();
  let task_ref = format!("refs/heads/{}", env.branch_name(id, &slug));
  let _ = repo.commit(task_ref.as_str(), "test", empty_tree, [parent_id])?;

  // Create a tracked file and commit it on base
  let tracked = env.path().join("tracked.txt");
  std::fs::write(&tracked, b"v1")?;
  let _ = std::process::Command::new("git")
    .current_dir(env.path())
    .args(["add", "tracked.txt"])
    .status()?;
  let _ = std::process::Command::new("git")
    .current_dir(env.path())
    .args(["commit", "-m", "add tracked"])
    .status()?;
  // Modify the tracked file without committing to make base dirty
  std::fs::write(&tracked, b"v2")?;

  // Merge should proceed even though base is dirty
  let mut cmd = env.bin_cmd()?;
  cmd.arg("merge").arg(id.to_string()).assert().success();

  // Working tree should still contain the uncommitted change
  let contents = std::fs::read_to_string(&tracked)?;
  assert_eq!(contents, "v2");

  // No stash entries should remain queued
  let stash_list = std::process::Command::new("git")
    .current_dir(env.path())
    .arg("stash")
    .arg("list")
    .output()?;
  assert!(stash_list.status.success());
  assert!(
    String::from_utf8_lossy(&stash_list.stdout)
      .trim()
      .is_empty(),
    "expected no lingering stash entries"
  );

  // Verify cleanup happened after successful merge
  assert!(!env.branch_exists(id, &slug)?);
  assert!(!env.task_file_path(id, &slug).exists());
  assert!(!env.worktree_dir_path(id, &slug).exists());

  Ok(())
}

#[test]
fn merge_refreshes_checked_out_base_worktree() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("merge-refresh", &["--draft"])?;
  // Prepare branch/worktree
  env.bootstrap_task(id)?;

  // Create a new commit on the task branch (empty tree commit)
  let repo = git::discover(env.path())?;
  let empty_tree = git::ObjectId::empty_tree(repo.object_hash());
  let head = repo.head_commit()?;
  let parent_id = head.id();
  let task_ref = format!("refs/heads/{}", env.branch_name(id, &slug));
  let _ = repo.commit(task_ref.as_str(), "test", empty_tree, [parent_id])?;

  // Merge back; since base is checked out and clean, we should refresh working tree
  let mut cmd = env.bin_cmd()?;
  let output = cmd.arg("merge").arg(id.to_string()).output()?;
  assert!(output.status.success());
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(stdout.contains("Refreshed checked-out working tree"));

  // And the repo remains clean
  let status_out = std::process::Command::new("git")
    .current_dir(env.path())
    .arg("status")
    .arg("--porcelain")
    .arg("--untracked-files=no")
    .output()?;
  assert!(status_out.status.success());
  assert!(
    String::from_utf8_lossy(&status_out.stdout)
      .trim()
      .is_empty()
  );

  Ok(())
}

#[test]
fn merge_fails_when_no_changes() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Create a new task without making any commits on the task branch
  let (id, slug) = env.new_task("merge-no-changes", &["--draft"])?;
  // Prepare branch/worktree (no changes introduced)
  env.bootstrap_task(id)?;

  // Attempt to merge should fail due to no differences vs base
  let mut cmd = env.bin_cmd()?;
  let output = cmd.arg("merge").arg(id.to_string()).output()?;
  assert!(
    !output.status.success(),
    "merge unexpectedly succeeded for no-op task"
  );

  // Ensure resources are retained after failure
  assert!(
    env.branch_exists(id, &slug)?,
    "branch should remain after failed merge"
  );
  assert!(
    env.task_file_path(id, &slug).exists(),
    "task file should remain"
  );
  assert!(
    env.worktree_dir_path(id, &slug).exists(),
    "worktree should remain"
  );

  Ok(())
}

#[test]
fn edit_opens_markdown_via_editor() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, _slug) = env.new_task("edit-task", &["--draft"])?;

  // Use a no-op editor that exits successfully
  with_vars([("EDITOR", Some("bash -lc true".to_string()))], || {
    let mut cmd = env.bin_cmd().unwrap();
    cmd.arg("edit").arg(id.to_string());
    cmd.assert().success();
  });

  Ok(())
}

#[test]
fn reset_prunes_worktree_and_branch_keeps_markdown() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("reset-task", &["--draft"])?;

  // Ensure branch/worktree exist before reset
  env.bootstrap_task(id)?;
  assert!(env.branch_exists(id, &slug)?);
  assert!(env.worktree_dir_path(id, &slug).is_dir());
  assert!(env.task_file_path(id, &slug).is_file());

  // First reset
  let mut cmd = env.bin_cmd()?;
  cmd.arg("reset").arg(id.to_string());
  cmd.assert().success();

  // Verify branch/worktree removed, markdown kept
  assert!(!env.branch_exists(id, &slug)?);
  assert!(!env.worktree_dir_path(id, &slug).exists());
  assert!(env.task_file_path(id, &slug).is_file());

  // Idempotent second reset
  let mut cmd = env.bin_cmd()?;
  cmd.arg("reset").arg(id.to_string());
  cmd.assert().success();

  Ok(())
}
