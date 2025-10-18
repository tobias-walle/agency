//! Core library for the Agency tool.
//!
//! Provides adapters (fs/git/pty), configuration loading and defaults,
//! domain models for tasks, a JSON-RPC daemon implementation, and DTOs
//! used by the CLI. The daemon exposes methods like `daemon.status`,
//! `task.new`, `task.status`, `task.start`, and PTY control flows.
//!
//! Quick start:
//! - Use `agency-core::daemon::start` to run the JSON-RPC server over a Unix socket.
//! - Load config via `agency-core::config::load(Some(project_root))`.
//! - Spawn agents through the PTY adapter after resolving actions in `agent::runner`.
//!
//! Maintainer note: `agency init` writes a default project config without
//! duplicate `[pty]` tables by placing `pty.detach_keys` documentation as
//! comments rather than introducing a second table header.

pub mod adapters;
pub mod agent;
pub mod config;
pub mod daemon;
pub mod domain;
pub mod logging;
pub mod rpc;
