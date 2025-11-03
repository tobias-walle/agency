use anyhow::Result;

use crate::config::AppContext;
use crate::utils::task::{branch_name, resolve_id_or_slug};

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let name = branch_name(&tref);
  anstream::println!("{}", name);
  Ok(())
}
