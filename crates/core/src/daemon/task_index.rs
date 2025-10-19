use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::adapters::fs as fsutil;
use crate::domain::task::{Task, TaskId};
use crate::rpc::TaskInfo;

pub fn next_task_id(tasks_dir: &Path) -> io::Result<u64> {
  let mut max_id = 0u64;
  if tasks_dir.exists() {
    for entry in fs::read_dir(tasks_dir)? {
      let entry = entry?;
      let name = entry.file_name();
      let name = name.to_string_lossy();
      if let Ok((TaskId(id), _slug)) = Task::parse_filename(&name)
        && id > max_id
      {
        max_id = id;
      }
    }
  }
  Ok(max_id + 1)
}

pub fn read_task_info(path: &Path, id: u64, slug: String) -> io::Result<TaskInfo> {
  let s = fs::read_to_string(path)?;
  let t = Task::from_markdown(TaskId(id), slug.clone(), &s)
    .map_err(|e| io::Error::other(e.to_string()))?;
  Ok(TaskInfo {
    id,
    slug,
    status: t.front_matter.status,
  })
}

pub fn find_task_path_by_ref(
  project_root: &Path,
  r: &crate::rpc::TaskRef,
) -> io::Result<(PathBuf, u64, String)> {
  let dir = fsutil::tasks_dir(project_root);
  let mut found: Option<(PathBuf, u64, String)> = None;
  for entry in fs::read_dir(&dir)? {
    let entry = entry?;
    let name = entry.file_name();
    let name = name.to_string_lossy().to_string();
    if let Ok((TaskId(id), slug)) = Task::parse_filename(&name) {
      let mut ok = false;
      if let Some(want) = r.id {
        ok = want == id;
      }
      if !ok && let Some(wslug) = &r.slug {
        ok = &slug == wslug;
      }
      if ok {
        found = Some((entry.path(), id, slug));
        break;
      }
    }
  }
  found.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "task not found"))
}
