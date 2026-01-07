//! Common error messages used across the codebase.
//! Centralizes error message strings to ensure consistency and simplify maintenance.

// TTY-related errors
pub(crate) const ATTACH_REQUIRES_TTY: &str =
  "attach requires an interactive terminal (TTY). Run this command in an interactive shell or terminal.";
pub(crate) const ATTACH_FOLLOW_REQUIRES_TTY: &str =
  "attach --follow requires an interactive terminal (TTY). Run this command in an interactive shell or terminal.";

// Process exit errors
pub(crate) const SHELL_NON_ZERO_EXIT: &str = "shell exited with non-zero status";
pub(crate) const EDITOR_NON_ZERO_EXIT: &str = "editor exited with non-zero status";
#[cfg(target_os = "macos")]
pub(crate) const OPEN_NON_ZERO_EXIT: &str = "open exited with non-zero status";
#[cfg(not(target_os = "macos"))]
pub(crate) const XDG_OPEN_NON_ZERO_EXIT: &str = "xdg-open exited with non-zero status";

// Clipboard errors
pub(crate) const NO_IMAGE_IN_CLIPBOARD: &str = "No image in clipboard. Copy an image first";

// Protocol errors for daemon communication
pub(crate) const PROTOCOL_ERROR_EXPECTED_PROJECT_STATE: &str =
  "Protocol error: Expected ProjectState reply";
pub(crate) const PROTOCOL_ERROR_EXPECTED_TUI_REGISTERED: &str =
  "Protocol error: expected TuiRegistered reply";
pub(crate) const PROTOCOL_ERROR_EXPECTED_TUI_LIST: &str =
  "Protocol error: expected TuiList reply";

// Git command errors - format functions
pub(crate) fn git_command_failed(command: &str, status: impl std::fmt::Display) -> String {
  format!("git {command} failed: status={status}")
}

// Worktree errors
pub(crate) fn worktree_not_found(path: impl std::fmt::Display, task_id: u32) -> String {
  format!(
    "worktree not found at {path}. Run `agency bootstrap {task_id}` or `agency start {task_id}` first"
  )
}

// Daemon errors
pub(crate) fn daemon_error(message: impl std::fmt::Display) -> String {
  format!("Daemon error: {message}")
}

// Tmux errors
pub(crate) fn tmux_server_not_ready(timeout: impl std::fmt::Display) -> String {
  format!("tmux server did not become ready within {timeout}")
}

pub(crate) fn tmux_server_not_ready_with_stderr(
  timeout: impl std::fmt::Display,
  stderr: impl std::fmt::Display,
) -> String {
  format!("tmux server did not become ready within {timeout}: {stderr}")
}
