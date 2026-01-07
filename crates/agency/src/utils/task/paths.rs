use std::path::PathBuf;

use crate::config::AgencyPaths;

use super::metadata::TaskRef;

pub fn branch_name(task: &TaskRef) -> String {
  format!("agency/{}-{}", task.id, task.slug)
}

pub fn worktree_name(task: &TaskRef) -> String {
  format!("{}-{}", task.id, task.slug)
}

pub fn worktree_dir(paths: &AgencyPaths, task: &TaskRef) -> PathBuf {
  paths.worktrees_dir().join(worktree_name(task))
}

pub fn task_file(paths: &AgencyPaths, task: &TaskRef) -> PathBuf {
  paths
    .tasks_dir()
    .join(format!("{}-{}.md", task.id, task.slug))
}
