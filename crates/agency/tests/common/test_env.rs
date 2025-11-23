use anyhow::Result;
use assert_cmd::Command;
use std::path::{Path, PathBuf};
use temp_env::with_vars;
use tempfile::{Builder, TempDir};

#[derive(Debug)]
pub struct TestEnv {
  temp: TempDir,
  runtime_dir: PathBuf,
  xdg_home: PathBuf,
}

impl TestEnv {
  pub fn run<F, R>(f: F) -> R
  where
    F: FnOnce(&TestEnv) -> R,
  {
    let env = TestEnv::new();
    let current_path = std::env::var("PATH").ok();
    let bin_dir = env.xdg_home_bin_dir();
    let path_value = match current_path {
      Some(existing) if !existing.is_empty() => {
        format!("{}:{existing}", bin_dir.display())
      }
      _ => bin_dir.display().to_string(),
    };
    let runtime_dir = env.runtime_dir().to_path_buf();
    with_vars(
      [
        (
          "XDG_CONFIG_HOME",
          Some(env.xdg_home_dir().display().to_string()),
        ),
        ("PATH", Some(path_value)),
        ("XDG_RUNTIME_DIR", Some(runtime_dir.display().to_string())),
        ("EDITOR", Some("bash -lc true".to_string())),
        ("AGENCY_NO_AUTOSTART", Some("1".to_string())),
      ],
      || f(&env),
    )
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
    cmd.env("XDG_RUNTIME_DIR", &self.runtime_dir);
    let uds_path = self.runtime_dir.join("agency.sock");
    let tmux_sock_path = self.runtime_dir.join("agency-tmux.sock");
    cmd.env("AGENCY_SOCKET_PATH", &uds_path);
    cmd.env("AGENCY_TMUX_SOCKET_PATH", &tmux_sock_path);
    cmd.env("AGENCY_TEST", "1");
    Ok(cmd)
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
      let metadata = std::fs::metadata(path).map_err(|err| {
        anyhow::anyhow!("read script metadata at {}: {err}", path.display())
      })?;
      let mut perms = metadata.permissions();
      perms.set_mode(0o755);
      std::fs::set_permissions(path, perms).map_err(|err| {
        anyhow::anyhow!("set script executable at {}: {err}", path.display())
      })?;
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
    let tmux_sock = self.runtime_dir.join("agency-tmux.sock");
    let _ = std::process::Command::new("tmux")
      .arg("-S")
      .arg(tmux_sock)
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
