use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;

use super::session::PtySession;

#[derive(Default)]
pub(crate) struct Registry {
  pub(crate) sessions: HashMap<(String, u64), Arc<PtySession>>,
  pub(crate) attachments: HashMap<String, Arc<PtySession>>,
}

static REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| Mutex::new(Registry::default()));

pub(crate) fn registry() -> &'static Mutex<Registry> {
  &REGISTRY
}

pub(crate) fn root_key(project_root: &Path) -> String {
  project_root
    .canonicalize()
    .unwrap_or_else(|_| project_root.to_path_buf())
    .display()
    .to_string()
}

#[cfg(test)]
pub fn clear_registry_for_tests() {
  let mut reg = REGISTRY.lock().unwrap();
  reg.sessions.clear();
  reg.attachments.clear();
}
