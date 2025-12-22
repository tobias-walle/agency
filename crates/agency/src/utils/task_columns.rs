use owo_colors::OwoColorize as _;

use crate::config::{AgencyConfig, AgencyPaths, AppContext};
use crate::daemon_protocol::SessionInfo;
use crate::utils::status::{TaskStatus, derive_status, is_task_completed};
use crate::utils::task::{
  TaskFrontmatter, TaskFrontmatterExt, TaskRef, agent_for_task, read_task_frontmatter, worktree_dir,
};

/// Git metrics for a task (uncommitted changes, commits ahead).
#[derive(Clone, Debug, Default)]
pub struct GitMetrics {
  pub uncommitted_add: u64,
  pub uncommitted_del: u64,
  pub commits_ahead: u64,
}

/// Raw data for a single task row. All display logic lives in `TaskColumn::cell()`.
#[derive(Clone, Debug)]
pub struct TaskRow {
  pub paths: AgencyPaths,
  pub config: AgencyConfig,
  pub task: TaskRef,
  pub session: Option<SessionInfo>,
  pub git_metrics: GitMetrics,
  pub wt_exists: bool,
  pub frontmatter: Option<TaskFrontmatter>,
}

impl TaskRow {
  /// Create a new `TaskRow` from raw components.
  /// Pre-computes filesystem-dependent values (worktree existence, frontmatter).
  #[must_use]
  pub fn new(
    ctx: &AppContext,
    task: TaskRef,
    session: Option<&SessionInfo>,
    git_metrics: GitMetrics,
  ) -> Self {
    Self {
      paths: ctx.paths.clone(),
      config: ctx.config.clone(),
      wt_exists: worktree_dir(&ctx.paths, &task).exists(),
      frontmatter: read_task_frontmatter(&ctx.paths, &task),
      task,
      session: session.cloned(),
      git_metrics,
    }
  }

  #[must_use]
  pub fn id(&self) -> u32 {
    self.task.id
  }

  #[must_use]
  pub fn session_id(&self) -> Option<u64> {
    self.session.as_ref().map(|s| s.session_id)
  }
}

/// Columns available for the task table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskColumn {
  Id,
  Slug,
  Status,
  Uncommitted,
  Commits,
  Base,
  Agent,
}

impl TaskColumn {
  /// All columns in display order.
  pub const ALL: &[TaskColumn] = &[
    TaskColumn::Id,
    TaskColumn::Slug,
    TaskColumn::Status,
    TaskColumn::Uncommitted,
    TaskColumn::Commits,
    TaskColumn::Base,
    TaskColumn::Agent,
  ];

  /// Column header text.
  #[must_use]
  pub fn header(self) -> &'static str {
    match self {
      TaskColumn::Id => "ID",
      TaskColumn::Slug => "SLUG",
      TaskColumn::Status => "STATUS",
      TaskColumn::Uncommitted => "UNCOMMITTED",
      TaskColumn::Commits => "COMMITS",
      TaskColumn::Base => "BASE",
      TaskColumn::Agent => "AGENT",
    }
  }

  /// Column flex weight for TUI display (like CSS flexbox flex-grow).
  /// Higher values = wider columns. Default is 1.
  #[must_use]
  pub fn weight(self) -> u8 {
    match self {
      TaskColumn::Slug | TaskColumn::Agent => 2,
      _ => 1,
    }
  }

  /// Calculate percentage widths from weights for all columns.
  #[must_use]
  pub fn width_percentages() -> Vec<u16> {
    let total_weight: u16 = Self::ALL.iter().map(|c| u16::from(c.weight())).sum();
    Self::ALL
      .iter()
      .map(|c| u16::from(c.weight()) * 100 / total_weight)
      .collect()
  }

  /// Format cell value with ANSI colors.
  ///
  /// Returns an ANSI-colored string suitable for CLI output.
  /// For TUI, use `ansi_to_spans()` from `tui/colors.rs` to convert.
  #[must_use]
  pub fn cell(self, row: &TaskRow, pending_delete: bool) -> String {
    match self {
      TaskColumn::Id => row.task.id.to_string(),
      TaskColumn::Slug => row.task.slug.clone(),
      TaskColumn::Status => Self::format_status(row, pending_delete),
      TaskColumn::Uncommitted => {
        let plus_str = if row.git_metrics.uncommitted_add > 0 {
          format!("+{}", row.git_metrics.uncommitted_add)
            .green()
            .to_string()
        } else {
          "+0".dimmed().to_string()
        };
        let minus_str = if row.git_metrics.uncommitted_del > 0 {
          format!("-{}", row.git_metrics.uncommitted_del)
            .red()
            .to_string()
        } else {
          "-0".dimmed().to_string()
        };
        format!("{plus_str}{minus_str}")
      }
      TaskColumn::Commits => {
        if row.git_metrics.commits_ahead == 0 {
          "-".dimmed().to_string()
        } else {
          row.git_metrics.commits_ahead.to_string().cyan().to_string()
        }
      }
      TaskColumn::Base => row.frontmatter.base_branch_or(|| "main".to_string()),
      TaskColumn::Agent => {
        agent_for_task(&row.config, row.frontmatter.as_ref()).unwrap_or_else(|| "-".to_string())
      }
    }
  }

  fn format_status(row: &TaskRow, pending_delete: bool) -> String {
    if pending_delete {
      return "Loading".dimmed().to_string();
    }
    let base_status = derive_status(row.session.as_ref(), row.wt_exists);
    let status = if is_task_completed(&row.paths, &row.task) {
      TaskStatus::Completed
    } else {
      base_status
    };
    match status {
      TaskStatus::Running | TaskStatus::Completed => status.label().green().to_string(),
      TaskStatus::Idle => status.label().blue().to_string(),
      TaskStatus::Exited | TaskStatus::Stopped => status.label().red().to_string(),
      TaskStatus::Draft => status.label().yellow().to_string(),
      TaskStatus::Other(s) => s,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::{AgencyPaths, AppContext};
  use crate::daemon_protocol::{SessionInfo, TaskMeta};
  use crate::utils::term::strip_ansi_control_codes;

  fn make_ctx() -> (tempfile::TempDir, AppContext) {
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    (dir, ctx)
  }

  fn make_task(id: u32, slug: &str) -> TaskRef {
    TaskRef {
      id,
      slug: slug.to_string(),
    }
  }

  fn make_session(session_id: u64, task_id: u32, slug: &str, status: &str) -> SessionInfo {
    SessionInfo {
      session_id,
      task: TaskMeta {
        id: task_id,
        slug: slug.to_string(),
      },
      status: status.to_string(),
      ..Default::default()
    }
  }

  #[test]
  fn header_returns_expected_values() {
    assert_eq!(TaskColumn::Id.header(), "ID");
    assert_eq!(TaskColumn::Slug.header(), "SLUG");
    assert_eq!(TaskColumn::Status.header(), "STATUS");
    assert_eq!(TaskColumn::Uncommitted.header(), "UNCOMMITTED");
    assert_eq!(TaskColumn::Commits.header(), "COMMITS");
    assert_eq!(TaskColumn::Base.header(), "BASE");
    assert_eq!(TaskColumn::Agent.header(), "AGENT");
  }

  #[test]
  fn width_percentages_are_calculated_from_weights() {
    let widths = TaskColumn::width_percentages();
    assert_eq!(widths.len(), TaskColumn::ALL.len());
    let total: u16 = widths.iter().sum();
    assert!(
      (95..=100).contains(&total),
      "Total should be close to 100, got {total}"
    );
  }

  #[test]
  fn cell_returns_colored_output() {
    let (_dir, ctx) = make_ctx();
    let task = make_task(42, "test-task");
    let session = make_session(1, 42, "test-task", "Running");
    let git_metrics = GitMetrics {
      uncommitted_add: 5,
      uncommitted_del: 3,
      commits_ahead: 2,
    };
    let row = TaskRow::new(&ctx, task, Some(&session), git_metrics);

    let id_cell = TaskColumn::Id.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&id_cell), "42");

    let status_cell = TaskColumn::Status.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&status_cell), "Running");
    assert!(status_cell.contains("\x1b[")); // Has ANSI codes

    let unc_cell = TaskColumn::Uncommitted.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&unc_cell), "+5-3");

    let commits_cell = TaskColumn::Commits.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&commits_cell), "2");
  }

  #[test]
  fn pending_delete_shows_loading() {
    let (_dir, ctx) = make_ctx();
    let task = make_task(1, "test");
    let session = make_session(1, 1, "test", "Running");
    let row = TaskRow::new(&ctx, task, Some(&session), GitMetrics::default());

    let status_cell = TaskColumn::Status.cell(&row, true);
    assert_eq!(strip_ansi_control_codes(&status_cell), "Loading");
  }

  #[test]
  fn uncommitted_zeros_are_dimmed() {
    let (_dir, ctx) = make_ctx();
    let task = make_task(1, "test");
    let row = TaskRow::new(&ctx, task, None, GitMetrics::default());

    let cell = TaskColumn::Uncommitted.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&cell), "+0-0");
    assert!(cell.contains("\x1b[")); // Has ANSI codes for dimmed
  }

  #[test]
  fn commits_zero_shows_dash() {
    let (_dir, ctx) = make_ctx();
    let task = make_task(1, "test");
    let row = TaskRow::new(&ctx, task, None, GitMetrics::default());

    let cell = TaskColumn::Commits.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&cell), "-");
    assert!(cell.contains("\x1b[")); // Has ANSI codes for dimmed
  }
}
