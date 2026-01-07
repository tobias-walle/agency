mod common;
mod config;
mod pane;
mod server;
mod session;

// Re-export common utilities
pub(crate) use common::{tmux_args_base, tmux_socket_path};

// Re-export server management functions
pub(crate) use server::{ensure_server, ensure_server_inherit_stderr, is_server_running, stop_server};

// Re-export session operations
pub(crate) use session::{
  attach_session, kill_session, list_sessions_for_project, prepare_session_for_attach,
  session_name, spawn_attach_session, start_session,
};

// Re-export config functions
pub(crate) use config::tmux_set_env_local;

// Re-export pane operations
pub(crate) use pane::{send_keys, send_keys_enter};
