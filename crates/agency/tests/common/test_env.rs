use crate::common::tmux::TMUX_SESSION_NAME;
use anyhow::Result;
use assert_cmd::{Command, cargo};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use temp_env::with_vars;
use tempfile::{Builder, TempDir};

#[derive(Debug)]
pub struct TestEnv {
  temp: TempDir,
  runtime_dir: PathBuf,
  xdg_home: PathBuf,
}

impl TestEnv {
  fn run_with_editor<F, R>(editor_cmd: &str, f: F) -> R
  where
    F: FnOnce(&TestEnv) -> R,
  {
    let env = TestEnv::new();
    let agency_bin = cargo::cargo_bin("agency");
    let agency_shim = format!("#!/usr/bin/env bash\n\"{}\" \"$@\"\n", agency_bin.display());
    env
      .add_xdg_home_bin("agency", &agency_shim)
      .expect("create agency shim in XDG bin");
    let vi_shim = "#!/usr/bin/env bash
set -e
file=\"$1\"
if [ -z \"$file\" ]; then
  exit 1
fi
printf '%s\n' 'Automated test' >\"$file\"
";
    env
      .add_xdg_home_bin("vi", vi_shim)
      .expect("create vi shim in XDG bin");
    let current_path = std::env::var("PATH").ok();
    let bin_dir = env.xdg_home_bin_dir();
    let path_value = match current_path {
      Some(existing) if !existing.is_empty() => {
        format!("{}:{existing}", bin_dir.display())
      }
      _ => bin_dir.display().to_string(),
    };
    let runtime_dir = env.runtime_dir().to_path_buf();
    let uds_path = runtime_dir.join("agency.sock");
    let tmux_sock_path = runtime_dir.join("agency-tmux.sock");
    with_vars(
      [
        (
          "XDG_CONFIG_HOME",
          Some(env.xdg_home_dir().display().to_string()),
        ),
        ("PATH", Some(path_value)),
        ("XDG_RUNTIME_DIR", Some(runtime_dir.display().to_string())),
        ("EDITOR", Some(editor_cmd.to_string())),
        ("AGENCY_NO_AUTOSTART", Some("1".to_string())),
        ("AGENCY_SOCKET_PATH", Some(uds_path.display().to_string())),
        (
          "AGENCY_TMUX_SOCKET_PATH",
          Some(tmux_sock_path.display().to_string()),
        ),
      ],
      || f(&env),
    )
  }

  pub fn run<F, R>(f: F) -> R
  where
    F: FnOnce(&TestEnv) -> R,
  {
    Self::run_with_editor("bash -lc true", f)
  }

  pub fn run_tty<F, R>(f: F) -> R
  where
    F: FnOnce(&TestEnv) -> R,
  {
    Self::run_with_editor("vi", f)
  }

  #[allow(clippy::unused_self)]
  pub fn wait_for<F>(&self, mut assert_fn: F) -> Result<()>
  where
    F: FnMut() -> Result<bool>,
  {
    let timeout = Duration::from_secs(1);
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

  pub fn new() -> Self {
    let root = tmp_root();
    let temp = Builder::new()
      .prefix("agency-test-")
      .tempdir_in(root)
      .expect("temp dir");
    let workdir = temp.path();
    let agen_dir = workdir.join(".agency");
    let _ = std::fs::create_dir_all(&agen_dir);
    let cfg_path = agen_dir.join("agency.toml");
    let cfg = "[agents.sh]\ncmd = [\"sh\"]\n";
    if let Err(err) = std::fs::write(&cfg_path, cfg) {
      panic!("write test agent config failed: {err}");
    }

    let runtime_dir = runtime_dir_create();

    let nanos = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|duration| duration.as_nanos())
      .unwrap_or(0);
    let xdg_home = tmp_root().join(format!("xdg-{nanos}"));
    let _ = std::fs::create_dir_all(&xdg_home);

    Self {
      temp,
      runtime_dir,
      xdg_home,
    }
  }

  pub fn with_env_vars<F, R>(&self, vars: &[(&str, Option<String>)], f: F) -> R
  where
    F: FnOnce(&TestEnv) -> R,
  {
    with_vars(vars, || f(self))
  }

  pub fn path(&self) -> &std::path::Path {
    self.temp.path()
  }

  pub fn runtime_dir(&self) -> &std::path::Path {
    &self.runtime_dir
  }

  pub fn xdg_home_dir(&self) -> &std::path::Path {
    &self.xdg_home
  }

  pub fn xdg_home_bin_dir(&self) -> std::path::PathBuf {
    self.xdg_home_dir().join("bin")
  }

  pub fn sockets_available(&self) -> bool {
    #[cfg(unix)]
    {
      use std::os::unix::net::UnixListener;
      let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
      let probe = self.runtime_dir.join(format!("probe-{nanos}.sock"));
      match UnixListener::bind(&probe) {
        Ok(_listener) => {
          let _ = std::fs::remove_file(&probe);
          true
        }
        Err(_) => false,
      }
    }
    #[cfg(not(unix))]
    {
      false
    }
  }

  pub fn agency(&self) -> Result<Command> {
    let mut cmd = Command::cargo_bin("agency")?;
    cmd.current_dir(self.path());
    Ok(cmd)
  }

  pub fn agency_tty(&self) -> std::process::Command {
    let bin_path = cargo::cargo_bin("agency");
    let mut cmd = std::process::Command::new(bin_path);
    cmd.current_dir(self.path());
    cmd
  }

  pub fn write_executable_script(&self, path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|err| {
        anyhow::anyhow!(
          "create parent dir for script under {}: {err}",
          self.path().display()
        )
      })?;
    }
    std::fs::write(path, body).map_err(|err| {
      anyhow::anyhow!(
        "write script body at {} relative to {}: {err}",
        path.display(),
        self.path().display()
      )
    })?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      let metadata = std::fs::metadata(path)
        .map_err(|err| anyhow::anyhow!("read script metadata at {}: {err}", path.display()))?;
      let mut perms = metadata.permissions();
      perms.set_mode(0o755);
      std::fs::set_permissions(path, perms)
        .map_err(|err| anyhow::anyhow!("set script executable at {}: {err}", path.display()))?;
    }
    Ok(())
  }

  pub fn write_file(&self, relative: &str, body: &str) -> Result<std::path::PathBuf> {
    let path = self.path().join(relative);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|err| {
        anyhow::anyhow!(
          "create parent dir for file under {}: {err}",
          self.path().display()
        )
      })?;
    }
    std::fs::write(&path, body).map_err(|err| {
      anyhow::anyhow!(
        "write file body at {} relative to {}: {err}",
        path.display(),
        self.path().display()
      )
    })?;
    Ok(path)
  }

  pub fn add_xdg_home_bin(&self, name: &str, body: &str) -> Result<std::path::PathBuf> {
    let bin_dir = self.xdg_home_bin_dir();
    let script_path = bin_dir.join(name);
    self.write_executable_script(&script_path, body)?;
    Ok(script_path)
  }

  pub fn write_xdg_config(&self, relative: &str, body: &str) -> Result<std::path::PathBuf> {
    let path = self.xdg_home_dir().join(relative);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|err| {
        anyhow::anyhow!(
          "create parent dir for XDG config under {}: {err}",
          self.xdg_home_dir().display()
        )
      })?;
    }
    std::fs::write(&path, body).map_err(|err| {
      anyhow::anyhow!(
        "write XDG config at {} under {}: {err}",
        path.display(),
        self.xdg_home_dir().display()
      )
    })?;
    Ok(path)
  }
}

pub fn tmp_root() -> std::path::PathBuf {
  let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let workspace_root = manifest_dir
    .parent()
    .and_then(|parent| parent.parent())
    .unwrap_or(&manifest_dir)
    .to_path_buf();
  let root = workspace_root.join("target").join("test-tmp");
  let _ = std::fs::create_dir_all(&root);
  root
}

pub fn tempdir_in_sandbox() -> TempDir {
  let root = tmp_root();
  Builder::new()
    .prefix("agency-test-")
    .tempdir_in(root)
    .expect("temp dir")
}

pub fn runtime_dir_create() -> std::path::PathBuf {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|duration| duration.as_nanos())
    .unwrap_or(0);
  let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let workspace_root = manifest_dir
    .parent()
    .and_then(|parent| parent.parent())
    .unwrap_or(&manifest_dir)
    .to_path_buf();
  let runtime_base = workspace_root.join("target").join(".r");
  let _ = std::fs::create_dir_all(&runtime_base);
  let dir = runtime_base.join(format!("r{nanos}"));
  let _ = std::fs::create_dir_all(&dir);
  dir
}

impl Drop for TestEnv {
  fn drop(&mut self) {
    let _ = std::process::Command::new("tmux")
      .arg("-S")
      .arg(self.tmux_tests_socket_path())
      .arg("kill-session")
      .arg("-t")
      .arg(TMUX_SESSION_NAME)
      .status();
    let tmux_sock = self.runtime_dir.join("agency-tmux.sock");
    let _ = std::process::Command::new("tmux")
      .arg("-S")
      .arg(&tmux_sock)
      .arg("kill-server")
      .status();

    if let Ok(mut cmd) = Command::cargo_bin("agency") {
      let _ = cmd
        .current_dir(self.path())
        .arg("daemon")
        .arg("stop")
        .output();
    }
    let _ = std::fs::remove_dir_all(&self.runtime_dir);
  }
}
