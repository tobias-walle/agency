use std::fs;
use std::path::PathBuf;

use tracing::{info, warn};

use crate::adapters::fs as fsutil;
use crate::domain::task::{Status, Task, TaskId};

/// On daemon start, optionally resume tasks by marking previously running ones as stopped.
pub fn resume_running_tasks_if_configured() {
  if let Some(root_os) = std::env::var_os("AGENCY_RESUME_ROOT") {
    let root = PathBuf::from(root_os);
    if !root.exists() {
      return;
    }
    let tasks_dir = fsutil::tasks_dir(&root);
    if !tasks_dir.exists() {
      return;
    }
    info!(event = "daemon_resume_scan", root = %root.display(), "scanning for running tasks to resume");
    match fs::read_dir(&tasks_dir) {
      Ok(read_dir) => {
        let mut running_count = 0;
        let mut stopped_count = 0;
        let mut error_count = 0;
        for entry in read_dir.flatten() {
          let name = entry.file_name();
          let name = name.to_string_lossy().to_string();
          if let Ok((TaskId(id), slug)) = Task::parse_filename(&name) {
            let path = entry.path();
            match fs::read_to_string(&path) {
              Ok(contents) => match Task::from_markdown(TaskId(id), slug.clone(), &contents) {
                Ok(mut task) => {
                  if task.front_matter.status == Status::Running {
                    running_count += 1;
                    match task.transition_to(Status::Stopped) {
                      Ok(()) => match task.to_markdown() {
                        Ok(markdown) => {
                          if let Err(error) = fs::write(&path, markdown) {
                            error_count += 1;
                            warn!(event = "daemon_resume_mark_stopped_write_fail", id, slug = %slug, error = %error, "failed to persist stopped status");
                          } else {
                            stopped_count += 1;
                          }
                        }
                        Err(error) => {
                          error_count += 1;
                          warn!(event = "daemon_resume_mark_stopped_serialize_fail", id, slug = %slug, error = %error, "failed to serialize task after marking stopped");
                        }
                      },
                      Err(error) => {
                        error_count += 1;
                        warn!(event = "daemon_resume_mark_stopped_transition_fail", id, slug = %slug, error = %error, "failed to mark task as stopped");
                      }
                    }
                  }
                }
                Err(error) => {
                  error_count += 1;
                  warn!(event = "daemon_resume_parse_fail", id, slug = %slug, error = %error, "failed to parse task markdown");
                }
              },
              Err(error) => {
                error_count += 1;
                warn!(event = "daemon_resume_read_fail", id, slug = %slug, error = %error, "failed to read task file");
              }
            }
          }
        }
        info!(event = "daemon_resume_mark_stopped", root = %root.display(), running = running_count, stopped = stopped_count, errors = error_count, "marked running tasks as stopped");
      }
      Err(error) => {
        warn!(event = "daemon_resume_read_dir_fail", root = %root.display(), error = %error, "failed to read tasks directory");
      }
    }
  }
}
