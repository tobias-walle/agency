# PLN-7: Global multi-session, multi-client daemon

Date: 2025-11-06

Introduce a global daemon that can host many sessions in parallel, allow multiple clients to attach to the same session concurrently, and run each session in the task worktree with task-scoped env and argv substitution. Prefer a single merged welcome handshake and reusable structs for clean routing and project filtering.

## Goals

- Single global daemon process that manages many sessions at once (no persistence across restarts).
- Sessions are addressable by an internal `session_id` and filterable by project.
- `agency attach {id|slug}` creates a new session for the task and attaches.
- Support `--session <id>` to join an existing session instead of creating one.
- Multiple clients per session, full duplex stdin and stdout.
- Resize semantics: latest win. On focus, client sends another resize.
- Each session inherits cwd and env from the task, not the daemon.
- Replace variables in argv using a helper and pass env map to the child.
- On agent exit, broadcast exit and prompt clients to press enter to restart (pause raw mode; colored output).
- `agency stop {id|slug}` stops all sessions for a task. `--session <id>` stops just one.
- Remove sessions if a task is deleted by `agency rm`.
 - Add `agency sessions` to list running sessions.

## Process Diagram (ASCII)

Open new session (attach <task>)

Client                          Daemon
  |  Connect                      |
  |------------------------------>|
  |  OpenSession{meta,rows,cols}  |
  |------------------------------>| create session, spawn cmd
  |<------------------------------| Welcome{session_id,rows,cols,ansi}
  |  render snapshot              |
  |  stream phase                 |
  |  Input{bytes}                -> PTY stdin
  |  Resize{rows,cols}           -> latest wins
  |<------------------------------| Output{bytes}
  |<------------------------------| Exited{code,stats} on agent exit
  |  prompt restart, on Enter     |
  |  RestartSession{session_id}  -> restart shell
  |<------------------------------| Welcome{...}

Join existing session

Client                          Daemon
  |  Connect                      |
  |------------------------------>|
  |  JoinSession{session_id,...}  |
  |------------------------------>|
  |<------------------------------| Welcome{session_id,rows,cols,ansi}
  |  stream phase (multi-client)  |

Stop session / task

Client                          Daemon
  |  StopSession{session_id}      |
  |------------------------------>|
  |<------------------------------| Exited{...} to all clients
  |<------------------------------| Goodbye to each attachment

List sessions

Client                          Daemon
  |  ListSessions{project?}       |
  |------------------------------>|
  |<------------------------------| Sessions{entries}
  |  render table                 |

Shutdown daemon

Client                          Daemon
  |  Shutdown                     |
  |------------------------------>|
  |  close socket, stop sessions  |

## Non Goals

- Sharing a single transport for multiple sessions per TCP connection. We keep the model of one connection bound to one session.
- Windows support.

## Current Behavior

- Daemon is global but supports a single session and a single attached client.
- Protocol carries only `Attach` with an optional task string and `Shutdown`.
- Client always creates one session, cannot join an existing one, and errors when daemon is busy.
- Session runs with daemon cwd and env, not the task worktree env.

## Solution

Design a global daemon with a session registry and a richer protocol. Use reusable structs for project, task, and command metadata so code stays clear and filterable by project.

### Planned Architecture & File Layout

- Protocol (wire): `crates/agency/src/pty/protocol.rs`
  - Add `OpenSession`, `JoinSession`, `RestartSession`, `StopSession`, `StopTask`, `ListSessions`, `Welcome`, `Sessions` and serde structs (`ProjectKey`, `TaskMeta`, `SessionOpenMeta`, `SessionInfo`).
- Daemon
  - Registry: `crates/agency/src/pty/registry.rs` (new) with `SessionRegistry`, `SessionEntry`, server-side `SessionMeta`.
  - Main loop: `crates/agency/src/pty/daemon.rs` uses registry; handles first-frame routing; multi-client attach; stop/restart/list.
- Session: `crates/agency/src/pty/session.rs`
  - Multi-sink output; stopped state; `restart_shell(rows, cols, cmd)`.
- Client: `crates/agency/src/pty/client.rs`
  - Send `OpenSession`/`JoinSession`; expect `Welcome`; focus-resize inside the client’s resize thread; exit prompt-and-restart.
  - `pty/client/tty.rs` remains raw mode guards only.
- CLI commands
  - `attach`, `stop` updates; add `sessions` at `crates/agency/src/commands/sessions.rs`.
  - Wire flags in `crates/agency/src/lib.rs`.
- Helpers: `crates/agency/src/utils/command.rs`
  - Extend `Command` with `env` and serde; add `expand_vars_in_argv`.

### Protocol

- C2DControl
  - `OpenSession { meta: SessionOpenMeta, rows: u16, cols: u16 }`
  - `JoinSession { session_id: u64, rows: u16, cols: u16 }`
  - `Resize { rows: u16, cols: u16 }`
  - `Detach`
  - `RestartSession { session_id: u64 }`
  - `StopSession { session_id: u64 }`
  - `StopTask { project: ProjectKey, task_id: u32, slug: String }`
  - `ListSessions { project: Option<ProjectKey> }`
  - `Ping { nonce: u64 }`
  - `Shutdown`
- D2CControl
  - `Welcome { session_id: u64, rows: u16, cols: u16, ansi: Vec<u8> }`
  - `Exited { code: Option<i32>, signal: Option<i32>, stats: SessionStatsLite }`
  - `Sessions { entries: Vec<SessionInfo> }`
  - `Goodbye`, `Error { message: String }`, `Pong { nonce: u64 }`
- Reusable structs
  - `ProjectKey { repo_root: String }` (canonical repo root path)
  - `TaskMeta { id: u32, slug: String }`
  - `Command` (existing utils struct), extended with `env: Vec<(String,String)>` and serde derives
  - `SessionOpenMeta { project: ProjectKey, task: TaskMeta, worktree_dir: String, cmd: Command }`
  - `SessionInfo { session_id, project, task, cwd, status, clients, created_at, stats }`

### Daemon

- `SessionRegistry`
  - Holds `next_id: u64` and `HashMap<u64, SessionEntry>`.
  - `SessionEntry { session: Session, meta: SessionMeta, clients: HashMap<u64, ClientAttachment> }`.
  - API: `create_session(meta, size) -> session_id`, `join_session(session_id, stream, rows, cols)`, `broadcast(session_id, control)`, `stop_session(session_id)`, `restart_session(session_id)`, `stop_task(project, id, slug)`.
- `Session` updates
  - Support multiple output sinks: keep a list of lossy output channels and forward bytes to all. Remove disconnected sinks on error.
  - Remove auto-restart on child exit. Instead set a `Stopped` state and broadcast `Exited` to all clients.
  - Add `restart_shell(rows, cols, cmd)` to restart with the same per-task launch settings.
- Accept loop
  - First frame must be `OpenSession`, `JoinSession`, or `Shutdown`.
  - Reader threads for each client route `Input`, `Resize`, `Detach`, `RestartSession`, `StopSession`.
  - Resize policy: accept any resize, apply to session, latest wins.
- Project filtering
  - Compute `ProjectKey` as canonical repo root. Store in `SessionMeta` to filter and manage by project easily.

### Client

- `agency attach {id|slug}`
  - Resolve task and worktree dir. Compute `ProjectKey` via git repo root.
  - Build env map from current process, add `AGENCY_TASK`, and collect as `Vec<(String,String)>`.
  - Choose agent argv template from config. Expand vars in argv and build `Command` with `cwd = worktree_dir` and `env`.
  - Send `OpenSession(meta, rows, cols)`. Expect `Welcome` and render snapshot. Remember `session_id` from `Welcome`.
  - Spawn threads: stdin -> input channel (Ctrl-Q still detaches), resize monitoring including focus events, output reader.
- Join existing session
  - `agency attach --session <id>` sends `JoinSession` and follows normal attach life-cycle.
- Focus-resize
  - On focus gained, detect current size and send `Resize`. Latest wins.
- Agent exit UX
  - On `Exited`, pause raw mode, print `Agent exited with status code {code}. Press enter to restart.` with colors, wait for Enter, then send `RestartSession { session_id }` and resume.

### CLI and Commands

- `attach {id|slug}`: create a session and attach.
- `attach --session <id>`: join existing session.
- `stop {id|slug}`: stop all sessions for that task in the current project.
- `stop --session <id>`: stop only that session.
- `sessions`: query daemon and render a table of running sessions.
- `rm {id|slug}`: after removing files and git refs, send `StopTask` to the daemon for cleanup.

### Helpers

- Env and argv substitution helper
  - Implement `expand_vars_in_argv(argv_template: &[String], env: &HashMap<String,String>) -> Vec<String>`.
  - Build `Command` from expanded argv with task-scoped env and cwd.
  - Pass env map to the child process when spawning.
- Session list rendering
  - Keep row formatting and table rendering inside the `sessions` CLI command; protocol only carries wire structs (`SessionInfo`).

