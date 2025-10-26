use anyhow::Result;

use crate::config::AgencyConfig;
use crate::utils::task::{branch_name, resolve_id_or_slug};

pub fn run(cfg: &AgencyConfig, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(cfg, ident)?;
  let name = branch_name(&tref);
  anstream::println!("{}", name);
  Ok(())
}
