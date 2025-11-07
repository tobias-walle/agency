use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::pty::protocol::{
  D2CControl, D2CControlChannel, D2COutputChannel, ProjectKey, SessionInfo, SessionOpenMeta,
  SessionStatsLite, TaskMeta, WireCommand,
};
use crate::pty::session::Session;
use crate::utils::command::Command;

/// Server-side metadata attached to a session.
pub struct SessionMeta {
  pub project: ProjectKey,
  pub task: TaskMeta,
  pub cwd: String,
  pub cmd: Command,
  pub created_at: SystemTime,
}

/// Attached client tracked by the registry for broadcast.
pub struct ClientAttachment {
  pub client_id: u64,
  pub control: D2CControlChannel,
  pub output: D2COutputChannel,
}

/// One running session entry in the registry.
pub struct SessionEntry {
  pub session_id: u64,
  pub session: Session,
  pub meta: SessionMeta,
  pub clients: HashMap<u64, ClientAttachment>,
  pub exited_notified: bool,
}

impl SessionEntry {
  #[must_use]
  pub fn stats_lite(&self) -> SessionStatsLite {
    self.session.stats_lite()
  }
}

/// Manages all sessions and attached clients in the daemon.
pub struct SessionRegistry {
  next_session_id: u64,
  next_client_id: u64,
  pub sessions: HashMap<u64, SessionEntry>,
}

impl Default for SessionRegistry {
  fn default() -> Self {
    Self::new()
  }
}

impl SessionRegistry {
  #[must_use]
  pub fn new() -> Self {
    Self {
      next_session_id: 1,
      next_client_id: 1,
      sessions: HashMap::new(),
    }
  }

  fn to_command(w: &WireCommand) -> Command {
    Command {
      program: w.program.clone(),
      args: w.args.clone(),
      cwd: std::path::PathBuf::from(&w.cwd),
      env: w.env.clone(),
    }
  }

  /// Find the most recently created session id for the given project/task tuple.
  /// Returns `None` if no session exists.
  #[must_use]
  pub fn find_latest_session_for_task(
    &self,
    project: &ProjectKey,
    task_id: u32,
    slug: &str,
  ) -> Option<u64> {
    let mut best: Option<(u64, std::time::SystemTime)> = None;
    for (sid, entry) in &self.sessions {
      if &entry.meta.project != project
        || entry.meta.task.id != task_id
        || entry.meta.task.slug != slug
      {
        continue;
      }
      let created = entry.meta.created_at;
      match best {
        None => best = Some((*sid, created)),
        Some((_bid, btime)) => {
          if created > btime {
            best = Some((*sid, created));
          }
        }
      }
    }
    best.map(|(sid, _)| sid)
  }

  /// Ensure a session is ready to attach.
  /// If the session has previously exited and has no clients, restart its shell
  /// with the provided size before attaching.
  pub fn ensure_running_for_attach(
    &mut self,
    session_id: u64,
    rows: u16,
    cols: u16,
  ) -> anyhow::Result<()> {
    if let Some(entry) = self.sessions.get_mut(&session_id)
      && entry.exited_notified
      && entry.clients.is_empty()
    {
      entry.session.restart_shell(rows, cols)?;
      entry.exited_notified = false;
    }
    Ok(())
  }

  pub fn create_session(
    &mut self,
    meta: SessionOpenMeta,
    rows: u16,
    cols: u16,
  ) -> anyhow::Result<u64> {
    let id = self.next_session_id;
    self.next_session_id += 1;

    let cmd = Self::to_command(&meta.cmd);
    let mut session = Session::new(rows, cols, cmd.clone())?;
    // Ensure no output sinks configured until a client attaches
    session.clear_all_sinks();

    let entry = SessionEntry {
      session_id: id,
      session,
      meta: SessionMeta {
        project: meta.project,
        task: meta.task,
        cwd: meta.worktree_dir,
        cmd,
        created_at: SystemTime::now(),
      },
      clients: HashMap::new(),
      exited_notified: false,
    };
    self.sessions.insert(id, entry);
    Ok(id)
  }

  pub fn attach_client(
    &mut self,
    session_id: u64,
    control: D2CControlChannel,
    output: D2COutputChannel,
  ) -> anyhow::Result<u64> {
    let entry = self
      .sessions
      .get_mut(&session_id)
      .ok_or_else(|| anyhow::anyhow!("invalid session id"))?;
    let client_id = self.next_client_id;
    self.next_client_id += 1;
    entry.session.add_output_sink(output.clone());
    entry.clients.insert(
      client_id,
      ClientAttachment {
        client_id,
        control,
        output,
      },
    );
    Ok(client_id)
  }

  pub fn detach_client(&mut self, session_id: u64, client_id: u64) {
    if let Some(entry) = self.sessions.get_mut(&session_id)
      && let Some(att) = entry.clients.remove(&client_id)
    {
      entry.session.remove_output_sink(&att.output);
    }
  }

  #[must_use]
  pub fn list_sessions(&self, project: Option<&ProjectKey>) -> Vec<SessionInfo> {
    let mut out = Vec::new();
    for entry in self.sessions.values() {
      if let Some(pk) = project
        && &entry.meta.project != pk
      {
        continue;
      }
      let stats = entry.stats_lite();
      let created_at_ms = entry
        .meta
        .created_at
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::from_millis(0))
        .as_millis() as u64;
      out.push(SessionInfo {
        session_id: entry.session_id,
        project: entry.meta.project.clone(),
        task: entry.meta.task.clone(),
        cwd: entry.meta.cwd.clone(),
        status: "Running".to_string(),
        clients: entry.clients.len() as u32,
        created_at_ms,
        stats,
      });
    }
    out
  }

  pub fn broadcast(&self, session_id: u64, ctrl: D2CControl) {
    if let Some(entry) = self.sessions.get(&session_id) {
      for att in entry.clients.values() {
        let _ = att.control.send(ctrl.clone());
      }
    }
  }

  pub fn apply_resize(&self, session_id: u64, rows: u16, cols: u16) {
    if let Some(entry) = self.sessions.get(&session_id) {
      entry.session.apply_resize(rows, cols);
    }
  }

  pub fn write_input(&self, session_id: u64, bytes: &[u8]) {
    if let Some(entry) = self.sessions.get(&session_id) {
      let _ = entry.session.write_input(bytes);
    }
  }

  pub fn restart_session(&mut self, session_id: u64, rows: u16, cols: u16) -> anyhow::Result<()> {
    if let Some(entry) = self.sessions.get_mut(&session_id) {
      entry.session.restart_shell(rows, cols)?;
      entry.exited_notified = false;
    }
    Ok(())
  }

  pub fn stop_session(&mut self, session_id: u64) -> anyhow::Result<()> {
    if let Some(mut entry) = self.sessions.remove(&session_id) {
      let _ = entry.session.stop();
      // Notify clients
      for (_id, att) in entry.clients.drain() {
        let _ = att.control.send_goodbye();
      }
    }
    Ok(())
  }

  pub fn stop_task(&mut self, project: &ProjectKey, task_id: u32, slug: &str) -> usize {
    let ids: Vec<u64> = self
      .sessions
      .iter()
      .filter(|(_id, e)| {
        &e.meta.project == project && e.meta.task.id == task_id && e.meta.task.slug == slug
      })
      .map(|(id, _)| *id)
      .collect();
    let count = ids.len();
    for sid in ids {
      let _ = self.stop_session(sid);
    }
    count
  }

  #[must_use]
  pub fn snapshot(&self, session_id: u64) -> Option<(Vec<u8>, (u16, u16))> {
    self.sessions.get(&session_id).map(|e| e.session.snapshot())
  }

  /// Scan sessions for exited children and return list to notify.
  pub fn collect_exited(&mut self) -> Vec<(u64, SessionStatsLite)> {
    let mut out = Vec::new();
    for (sid, entry) in &mut self.sessions {
      if entry.exited_notified {
        continue;
      }
      if let Some(_status) = entry.session.try_wait_child() {
        let stats = entry.session.stats_lite();
        entry.exited_notified = true;
        out.push((*sid, stats));
      }
    }
    out
  }
}
