use anyhow::Result;

use crate::config::AppContext;
use crate::log_info;
use crate::utils::log::t;
use crate::utils::task::{
  edit_task_description, read_task_content, resolve_id_or_slug, write_task_content,
};

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  let mut content = read_task_content(&ctx.paths, &task)?;
  log_info!("Edit description {}", t::slug(&task.slug));
  if let Some(updated_body) =
    edit_task_description(&ctx.paths, &task, ctx.paths.cwd(), &content.body)?
  {
    content.body = updated_body;
    write_task_content(&ctx.paths, &task, &content)?;
  }
  Ok(())
}
