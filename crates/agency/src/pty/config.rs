//! Shared configuration constants for the PTY demo.
//!
//! The tests run the daemon and client in a temporary working directory and rely
//! on this relative path so the client can discover the daemon socket without
//! requiring global file system state.

/// Default Unix socket path used by both client and daemon.
///
/// This is intentionally a relative path so tests can `chdir` into a temp
/// directory and isolate state.
pub const DEFAULT_SOCKET_PATH: &str = "./tmp/daemon.sock";
