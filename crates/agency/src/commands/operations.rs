use anyhow::Result;

use crate::commands::{attach, complete, edit, merge, open, reset, rm, shell, start, stop};
use crate::config::AppContext;

/// Represents all operations that can be performed on tasks through the TUI.
#[derive(Clone, Debug)]
pub enum Operation {
  Attach { session_id: u64 },
  Complete { task: String, base: Option<String> },
  Edit { task: String },
  Merge { task: String, base: Option<String> },
  Open { task: String },
  Reset { task: String },
  Remove { task: String },
  Shell { task: String },
  Start { task: String, attach: bool },
  Stop { task: Option<String>, session_id: Option<u64> },
}

impl Operation {
  /// Returns the CLI command string equivalent for this operation.
  pub fn cli_command(&self) -> String {
    match self {
      Self::Attach { session_id } => format!("agency attach --session {session_id}"),
      Self::Complete { task, base } => {
        if let Some(base_branch) = base {
          format!("agency complete {task} --branch {base_branch}")
        } else {
          format!("agency complete {task}")
        }
      }
      Self::Edit { task } => format!("agency edit {task}"),
      Self::Merge { task, base } => {
        if let Some(base_branch) = base {
          format!("agency merge {task} --branch {base_branch}")
        } else {
          format!("agency merge {task}")
        }
      }
      Self::Open { task } => format!("agency open {task}"),
      Self::Reset { task } => format!("agency reset {task}"),
      Self::Remove { task } => format!("agency rm {task}"),
      Self::Shell { task } => format!("agency shell {task}"),
      Self::Start { task, attach } => {
        if *attach {
          format!("agency start {task}")
        } else {
          format!("agency start --no-attach {task}")
        }
      }
      Self::Stop { task, session_id } => {
        if let Some(sid) = session_id {
          format!("agency stop --session {sid}")
        } else if let Some(task_ident) = task {
          format!("agency stop --task {task_ident}")
        } else {
          "agency stop".to_string()
        }
      }
    }
  }
}

/// Execute an operation using the appropriate command implementation.
///
/// # Errors
/// Returns an error if the operation fails to execute.
pub fn execute(ctx: &AppContext, op: &Operation) -> Result<()> {
  match op {
    Operation::Attach { session_id } => attach::run_join_session(ctx, *session_id),
    Operation::Complete { task, base } => {
      complete::run_force(ctx, task, base.as_deref())
    }
    Operation::Edit { task } => edit::run(ctx, task),
    Operation::Merge { task, base } => merge::run(ctx, task, base.as_deref()),
    Operation::Open { task } => open::run(ctx, task),
    Operation::Reset { task } => reset::run(ctx, task),
    Operation::Remove { task } => rm::run_force(ctx, task),
    Operation::Shell { task } => shell::run(ctx, task),
    Operation::Start { task, attach } => start::run_with_attach(ctx, task, *attach),
    Operation::Stop { task, session_id } => {
      stop::run(ctx, task.as_deref(), *session_id)
    }
  }
}
