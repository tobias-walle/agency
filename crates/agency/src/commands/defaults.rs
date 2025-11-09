use std::io::{self, Write};

use anyhow::Result;

use crate::log_info;

const EMBEDDED_CONFIG: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/defaults/agency.toml"));

pub fn run() -> Result<()> {
  log_info!("Embedded agency defaults (read-only)");
  let mut stdout = io::stdout().lock();
  writeln!(stdout, "{EMBEDDED_CONFIG}")?;
  Ok(())
}
