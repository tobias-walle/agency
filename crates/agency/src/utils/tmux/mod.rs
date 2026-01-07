mod common;
mod config;
mod pane;
mod server;
mod session;

// Re-export common utilities
pub use common::{tmux_args_base, tmux_socket_path};

// Re-export server management functions
pub use server::{ensure_server, ensure_server_inherit_stderr, is_server_running, stop_server};

// Re-export session operations
pub use session::{
  attach_session, kill_session, list_sessions_for_project, prepare_session_for_attach,
  session_name, spawn_attach_session, start_session,
};

// Re-export config functions
pub use config::tmux_set_env_local;

// Re-export pane operations
pub use pane::{send_keys, send_keys_enter};
