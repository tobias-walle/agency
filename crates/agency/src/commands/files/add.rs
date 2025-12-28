use std::path::PathBuf;

use anyhow::Result;

use crate::config::AppContext;
use crate::log_info;
use crate::utils::clipboard::read_image_from_clipboard;
use crate::utils::files::{add_file, add_file_from_bytes};
use crate::utils::log::t;
use crate::utils::task::resolve_id_or_slug;

pub fn run(
  ctx: &AppContext,
  task_ident: &str,
  source: Option<&str>,
  from_clipboard: Option<&str>,
) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;

  let file_ref = if let Some(filename) = from_clipboard {
    let data = read_image_from_clipboard()?;
    add_file_from_bytes(&ctx.paths, &task, filename, &data)?
  } else {
    let source_path = source.ok_or_else(|| {
      anyhow::anyhow!("Provide a source path or use --from-clipboard")
    })?;
    let path = PathBuf::from(source_path);
    add_file(&ctx.paths, &task, &path)?
  };

  log_info!(
    "Added file {} {} to task {}",
    t::id(file_ref.id),
    t::path(&file_ref.name),
    t::slug(&task.slug)
  );

  Ok(())
}
