use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use jsonrpsee::core::RpcResult;
use jsonrpsee::server::RpcModule;
use jsonrpsee::types::ErrorObjectOwned;
use tracing::info;

use crate::adapters::{fs as fsutil, git as gitutil};
use crate::domain::task::{Status, Task, TaskFrontMatter, TaskId};
use crate::rpc::{
  TaskInfo, TaskListParams, TaskListResponse, TaskNewParams, TaskStartParams, TaskStartResult,
};

use super::super::task_index::{find_task_path_by_ref, next_task_id, read_task_info};

/// Register task lifecycle APIs: task.new, task.status, task.start.
pub fn register(module: &mut RpcModule<PathBuf>) {
  // ---- task.new ----
  module
    .register_method(
      "task.new",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: TaskNewParams = params.parse()?;
        let root = PathBuf::from(&p.project_root);
        fsutil::ensure_layout(&root)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let tasks_dir = fsutil::tasks_dir(&root);
        let id = next_task_id(&tasks_dir)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let slug = p.slug;
        let fm = TaskFrontMatter {
          base_branch: p.base_branch,
          status: Status::Draft,
          labels: p.labels,
          created_at: Utc::now(),
          agent: p.agent,
          session_id: None,
        };
        let task = Task {
          id: TaskId(id),
          slug: slug.clone(),
          front_matter: fm,
          body: p.body.unwrap_or_default(),
        };
        let md = task
          .to_markdown()
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let file_path = tasks_dir.join(Task::format_filename(task.id, &slug));
        fs::write(&file_path, md)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        info!(event = "task_new", id, slug = %slug, path = %file_path.display(), "task created");
        let info = TaskInfo {
          id,
          slug,
          status: Status::Draft,
        };
        Ok(serde_json::json!(info))
      },
    )
    .expect("register task.new");

  // ---- task.status ----
  module
    .register_method(
      "task.status",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: TaskListParams = params.parse()?;
        let root = PathBuf::from(&p.project_root);
        let tasks_dir = fsutil::tasks_dir(&root);
        let mut tasks: Vec<TaskInfo> = Vec::new();
        if tasks_dir.exists() {
          for entry in fs::read_dir(&tasks_dir)
            .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?
          {
            let entry =
              entry.map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Ok((TaskId(id), slug)) = Task::parse_filename(&name)
              && let Ok(info) = read_task_info(&entry.path(), id, slug)
            {
              tasks.push(info);
            }
          }
        }
        tasks.sort_by_key(|t| t.id);
        let resp = TaskListResponse { tasks };
        Ok(serde_json::json!(resp))
      },
    )
    .expect("register task.status");

  // ---- task.start (ensure git worktree + PTY spawn) ----
  module
    .register_method(
      "task.start",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: TaskStartParams = params.parse()?;
        let root = PathBuf::from(&p.project_root);
        let (path, id, slug) = find_task_path_by_ref(&root, &p.task)
          .map_err(|e| ErrorObjectOwned::owned(-32001, e.to_string(), None::<()>))?;
        let s = fs::read_to_string(&path)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let mut task = Task::from_markdown(TaskId(id), slug.clone(), &s)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let repo = match git2::Repository::open(&root) {
          Ok(r) => r,
          Err(_) => {
            return Err(ErrorObjectOwned::owned(-32002, "not a git repository", None::<()>));
          }
        };
        let base_sha = gitutil::resolve_base_branch_tip(&repo, &task.front_matter.base_branch)
          .map_err(|e| ErrorObjectOwned::owned(-32003, e.to_string(), None::<()>))?;
        tracing::info!(event = "task_start_validated", id, slug = %slug, base_branch = %task.front_matter.base_branch, base_sha = %base_sha.to_string(), "validated git base");
        let wt = gitutil::ensure_task_worktree(&repo, &root, id, &slug, &task.front_matter.base_branch)
          .map_err(|e| ErrorObjectOwned::owned(-32005, e.to_string(), None::<()>))?;
        task.transition_to(Status::Running)
          .map_err(|e| ErrorObjectOwned::owned(-32004, e.to_string(), None::<()>))?;
        let md = task
          .to_markdown()
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        fs::write(&path, md)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let config = crate::config::load(Some(&root))
          .map_err(|e| ErrorObjectOwned::owned(-32006, e.to_string(), None::<()>))?;
        let (program, base_args) = crate::agent::resolve_action(&config, &task.front_matter.agent, crate::agent::AgentAction::Start)
          .map_err(|e| ErrorObjectOwned::owned(-32007, e.to_string(), None::<()>))?;
        let env_map = crate::agent::build_env(crate::agent::BuildEnvInput {
          task_id: task.id,
          slug: &task.slug,
          body: &task.body,
          prompt: &task.body,
          project_root: &root,
          worktree_path: &wt,
          session_id: task.front_matter.session_id.as_deref(),
          message: None,
        });
        let substituted_args = crate::agent::substitute_tokens(&base_args, &env_map);
        let env_pairs: Vec<(&str, &str)> = env_map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        tracing::info!(event = "task_start_spawn_prepare", id, slug = %slug, agent = ?task.front_matter.agent, program = %program, args_len = substituted_args.len(), env_vars = env_pairs.len(), "spawning agent process");
        if let Err(error) = crate::adapters::pty::spawn_command(
          &root,
          id,
          &slug,
          &wt,
          &program,
          &substituted_args,
          &env_pairs,
        ) {
          tracing::error!(event = "task_start_spawn_failed", id, slug = %slug, program = %program, error = %error, "failed to spawn agent process");
          return Err(ErrorObjectOwned::owned(-32008, error.to_string(), None::<()>));
        }
        tracing::info!(event = "task_start_spawn_ok", id, slug = %slug, program = %program, args_len = substituted_args.len(), env_vars = env_pairs.len(), "spawned agent process");
        let res = TaskStartResult { id, slug, status: Status::Running };
        Ok(serde_json::json!(res))
      },
    )
    .expect("register task.start");
}
