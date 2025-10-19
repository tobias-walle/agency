use std::env;
use std::path::{Path, PathBuf};

use dirs::data_dir;
use dirs::runtime_dir;

use super::types::{ConfigError, Result};

/// Location of the global config file (~/.config/agency/config.toml)
pub fn global_config_path() -> Option<PathBuf> {
  dirs::config_dir().map(|p| p.join("agency").join("config.toml"))
}

/// Location of the project config file (./.agency/config.toml)
pub fn project_config_path(project_root: &Path) -> PathBuf {
  project_root.join(".agency").join("config.toml")
}

/// Resolve the socket path using AGENCY_SOCKET or platform defaults.
pub fn resolve_socket_path() -> Result<PathBuf> {
  let env_socket = env::var("AGENCY_SOCKET").ok().map(PathBuf::from);
  // Prefer runtime_dir for ephemeral sockets; fall back to data_dir
  let base_dir = runtime_dir().or(data_dir());
  if let Some(val) = env_socket {
    return Ok(val);
  }
  if let Some(dir) = base_dir {
    return Ok(dir.join("agency.sock"));
  }
  Err(ConfigError::UnsupportedPlatform)
}

#[cfg(test)]
pub(crate) fn resolve_socket_path_for(env_socket: Option<PathBuf>) -> Result<PathBuf> {
  // Prefer runtime_dir for ephemeral sockets; fall back to data_dir
  let base_dir = runtime_dir().or(data_dir());
  if let Some(val) = env_socket {
    return Ok(val);
  }
  if let Some(dir) = base_dir {
    return Ok(dir.join("agency.sock"));
  }
  Err(ConfigError::UnsupportedPlatform)
}
