use anyhow::Result;

use crate::config::AppContext;
use crate::daemon_protocol::TaskMeta;
use crate::daemon_protocol::TuiListItem;
use crate::daemon_protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, read_frame, write_frame,
};
use crate::utils::daemon as dutil;
use crate::utils::daemon::{get_project_state, send_start_bootstrap};
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::interactive;
use crate::utils::session::{build_session_plan, start_session_for_task};
use crate::utils::task::resolve_id_or_slug;
use crate::utils::tmux;
use crossbeam_channel::unbounded;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
mod overlay;
use overlay::OverlayUI;
use std::process::Child;

pub fn run_with_task(ctx: &AppContext, ident: &str) -> Result<()> {
  if !ctx.tty.is_interactive() {
    anyhow::bail!("attach requires an interactive terminal (TTY). Run this command in an interactive shell or terminal.");
  }

  // Initialize env_logger similar to pty-demo main
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .format_timestamp_secs()
    .try_init();

  // Resolve task
  let task = resolve_id_or_slug(&ctx.paths, ident)?;

  // Query existing sessions and join the latest for this task; error if none
  let entries = get_project_state(ctx)?.sessions;
  let target = entries
    .into_iter()
    .filter(|e| e.task.id == task.id && e.task.slug == task.slug)
    .max_by_key(|e| e.created_at_ms);

  let task_meta = TaskMeta {
    id: task.id,
    slug: task.slug.clone(),
  };
  if target.is_some() {
    return interactive::scope(|| tmux::attach_session(&ctx.config, &task_meta));
  }
  // Auto-start when missing using shared session helpers, then attach
  let plan = build_session_plan(ctx, &task)?;
  let bootstrap_request = plan.bootstrap_request.clone();

  crate::utils::daemon::notify_after_task_change(ctx, || {
    // Send bootstrap request to daemon BEFORE starting session
    // This ensures fast bootstrap commands complete before the agent starts
    if let Some(request) = bootstrap_request {
      send_start_bootstrap(ctx, request);
    }

    // Start session (creates tmux session and sends agent command)
    start_session_for_task(ctx, &plan, false)?;

    // Attach (this blocks until user detaches)
    interactive::scope(|| tmux::attach_session(&ctx.config, &plan.task_meta))
  })
}

pub fn run_join_session(ctx: &AppContext, session_id: u64) -> Result<()> {
  if !ctx.tty.is_interactive() {
    anyhow::bail!("attach requires an interactive terminal (TTY). Run this command in an interactive shell or terminal.");
  }

  let entries = get_project_state(ctx)?.sessions;
  let Some(si) = entries.into_iter().find(|e| e.session_id == session_id) else {
    anyhow::bail!("Session not found: {session_id}");
  };
  interactive::scope(|| tmux::attach_session(&ctx.config, &si.task))
}

pub fn run_follow(ctx: &AppContext, tui_id_opt: Option<u32>) -> Result<()> {
  if !ctx.tty.is_interactive() {
    anyhow::bail!("attach --follow requires an interactive terminal (TTY). Run this command in an interactive shell or terminal.");
  }

  // Resolve project key
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };

  // Pick TUI id: explicit or auto-pick when exactly one exists
  let target_id = pick_tui_id(tui_id_opt, &dutil::tui_list(ctx)?)?;

  // Spawn background reader subscribed to events
  let (tx, rx) = unbounded::<D2C>();
  let events_project = project.clone();
  let ctx_clone = ctx.clone();
  std::thread::Builder::new()
    .name("follow-events".into())
    .spawn(move || {
      if let Ok(mut s) = crate::utils::daemon::connect_daemon(&ctx_clone)
        && write_frame(
          &mut s,
          &C2D::Control(C2DControl::SubscribeEvents {
            project: events_project,
          }),
        )
        .is_ok()
      {
        // Drain initial
        let _ = read_frame::<_, D2C>(&mut s);
        while let Ok(msg) = read_frame::<_, D2C>(&mut s) {
          let _ = tx.send(msg);
        }
      }
    })?;

  // Follow handshake on separate connection
  let mut follow_stream = crate::utils::daemon::connect_daemon(ctx)?;
  write_frame(
    &mut follow_stream,
    &C2D::Control(C2DControl::TuiFollow {
      project: project.clone(),
      tui_id: target_id,
    }),
  )?;
  // Receive success and optional immediate focus
  let mut initial_focus: Option<u32> = None;
  for _ in 0..2 {
    match read_frame::<_, D2C>(&mut follow_stream) {
      Ok(D2C::Control(D2CControl::TuiFocusTaskChanged { task_id, .. })) => {
        initial_focus = Some(task_id);
      }
      Ok(D2C::Control(D2CControl::TuiFollowFailed { message })) => anyhow::bail!(message),
      Ok(D2C::Control(_)) => {}
      Err(_) => break,
    }
  }

  // Fallback: ask daemon for current focus if none was pushed yet
  if initial_focus.is_none()
    && let Ok(items) = dutil::tui_list(ctx)
    && let Some(it) = items.into_iter().find(|i| i.tui_id == target_id)
  {
    initial_focus = it.focused_task_id;
  }
  // Child management and current focus
  // Track a generation counter to avoid races from late exits
  let mut current_child: Option<(Child, u64)> = None; // (tmux attach child, gen)
  let mut child_gen: u64 = 0;
  let mut overlay_active: bool = false; // inline overlay
  let mut overlay_task_id: Option<u32> = None;
  let mut current_task_id: Option<u32> = None;
  if let Some(tid) = initial_focus {
    let attach_target = handle_focus_change(
      ctx,
      &mut current_child,
      &mut overlay_active,
      &mut overlay_task_id,
      &mut current_task_id,
      tid,
    )?;
    if let Some(task) = attach_target {
      child_gen = child_gen.wrapping_add(1);
      current_child = Some((tmux::spawn_attach_session(&ctx.config, &task)?, child_gen));
    }
  }

  // Overlay UI (ratatui) lifecycle
  let overlay_ui: Option<OverlayUI> = None;

  run_follow_loop(
    ctx,
    target_id,
    &rx,
    current_child,
    child_gen,
    overlay_active,
    overlay_task_id,
    current_task_id,
    overlay_ui,
  )
}

#[allow(clippy::too_many_arguments)]
fn run_follow_loop(
  ctx: &AppContext,
  target_id: u32,
  rx: &crossbeam_channel::Receiver<D2C>,
  mut current_child: Option<(Child, u64)>,
  mut child_gen: u64,
  mut overlay_active: bool,
  mut overlay_task_id: Option<u32>,
  mut current_task_id: Option<u32>,
  mut overlay_ui: Option<OverlayUI>,
) -> Result<()> {
  loop {
    // Draw overlay when active and poll for key events
    if overlay_active && handle_overlay(ctx, &mut overlay_ui, &mut overlay_task_id)? {
      return Ok(());
    }

    // Periodically ensure the followed TUI still exists; cancel if it disappeared
    check_tui_still_exists(ctx, target_id, &mut overlay_ui)?;

    match rx.recv_timeout(std::time::Duration::from_millis(200)) {
      Ok(D2C::Control(D2CControl::TuiFocusTaskChanged {
        tui_id, task_id, ..
      }))
        if tui_id == target_id =>
      {
        let attach_target = handle_focus_change(
          ctx,
          &mut current_child,
          &mut overlay_active,
          &mut overlay_task_id,
          &mut current_task_id,
          task_id,
        )?;
        // When switching to attach, ensure overlay UI is torn down first
        if let Some(task) = attach_target {
          if let Some(ui) = overlay_ui.take() {
            ui.restore();
          }
          child_gen = child_gen.wrapping_add(1);
          current_child = Some((tmux::spawn_attach_session(&ctx.config, &task)?, child_gen));
        }
      }
      Ok(_other) => {}
      Err(_timeout) => {}
    }
    // Handle attach child lifecycle: user detach vs. session end
    if let Some((mut ch, cur_gen)) = current_child.take() {
      if let Ok(Some(_status)) = ch.try_wait() {
        match decide_follow_on_child_exit(ctx, child_gen, cur_gen, current_task_id) {
          FollowExitAction::CancelFollow => {
            if let Some(ui) = overlay_ui.take() {
              ui.restore();
            }
            return Ok(());
          }
          FollowExitAction::SwitchToOverlay(tid) => {
            activate_overlay(&mut overlay_active, &mut overlay_task_id, tid);
          }
          FollowExitAction::Ignore => {}
        }
      } else {
        current_child = Some((ch, cur_gen));
      }
    }

    // If overlay is active and a session was started externally, attach now
    if overlay_active
      && let (Some(task_id), Ok(state)) = (overlay_task_id, dutil::get_project_state(ctx))
      && state.sessions.iter().any(|s| s.task.id == task_id)
    {
      let task = TaskMeta {
        id: task_id,
        slug: state
          .tasks
          .iter()
          .find(|t| t.id == task_id)
          .map_or_else(|| format!("task-{task_id}"), |t| t.slug.clone()),
      };
      if let Some(ui) = overlay_ui.take() {
        ui.restore();
      }
      child_gen = child_gen.wrapping_add(1);
      current_child = Some((tmux::spawn_attach_session(&ctx.config, &task)?, child_gen));
      overlay_active = false;
      overlay_task_id = None;
    }
  }
}

fn handle_overlay(
  ctx: &AppContext,
  overlay_ui: &mut Option<OverlayUI>,
  overlay_task_id: &mut Option<u32>,
) -> Result<bool> {
  if overlay_ui.is_none() {
    *overlay_ui = Some(OverlayUI::init()?);
  }
  if let Some(ui) = overlay_ui.as_mut()
    && let Ok(state) = dutil::get_project_state(ctx)
    && let Some(tid) = *overlay_task_id
  {
    let slug = state
      .tasks
      .iter()
      .find(|t| t.id == tid)
      .map_or_else(|| format!("task-{tid}"), |t| t.slug.clone());
    ui.draw(&slug, tid)?;
  }
  while event::poll(std::time::Duration::from_millis(0))? {
    if let Event::Key(k) = event::read()?
      && k.kind != KeyEventKind::Repeat
    {
      if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c')) {
        if let Some(ui) = overlay_ui.take() {
          ui.restore();
        }
        return Ok(true);
      }
      if let KeyCode::Char('s') = k.code
        && let Some(task_id) = *overlay_task_id
      {
        let state = dutil::get_project_state(ctx)?;
        let slug = state
          .tasks
          .iter()
          .find(|t| t.id == task_id)
          .map_or_else(|| format!("task-{task_id}"), |t| t.slug.clone());
        let tref = crate::utils::task::TaskRef { id: task_id, slug };
        let plan = build_session_plan(ctx, &tref)?;
        let bootstrap_request = plan.bootstrap_request.clone();
        // Send bootstrap request BEFORE starting session
        if let Some(request) = bootstrap_request {
          send_start_bootstrap(ctx, request);
        }
        let _ = start_session_for_task(ctx, &plan, false);
      }
    }
  }
  Ok(false)
}

fn check_tui_still_exists(
  ctx: &AppContext,
  target_id: u32,
  overlay_ui: &mut Option<OverlayUI>,
) -> Result<()> {
  if let Ok(items) = dutil::tui_list(ctx)
    && !items.iter().any(|i| i.tui_id == target_id)
  {
    if let Some(ui) = overlay_ui.take() {
      ui.restore();
    }
    anyhow::bail!("Follow canceled: TUI {target_id} disappeared (closed or died)");
  }
  Ok(())
}

fn pick_tui_id(explicit: Option<u32>, items: &[TuiListItem]) -> anyhow::Result<u32> {
  if let Some(id) = explicit {
    return Ok(id);
  }
  match items.len() {
    0 => anyhow::bail!("No TUI instances found. Open Agency TUI first."),
    1 => Ok(items[0].tui_id),
    _ => {
      anyhow::bail!(
        "More than one TUI open. Please provide a TUI ID --follow <tui-id>. You can find the TUI ID in the top right corner."
      )
    }
  }
}

fn handle_focus_change(
  ctx: &AppContext,
  current_child: &mut Option<(Child, u64)>,
  overlay_active: &mut bool,
  overlay_task_id: &mut Option<u32>,
  current_task_id: &mut Option<u32>,
  task_id: u32,
) -> anyhow::Result<Option<TaskMeta>> {
  *current_task_id = Some(task_id);
  // Decide attach vs overlay based on sessions
  let state = dutil::get_project_state(ctx)?;
  let has_session = state.sessions.iter().any(|s| s.task.id == task_id);
  let task = TaskMeta {
    id: task_id,
    slug: state
      .tasks
      .iter()
      .find(|t| t.id == task_id)
      .map_or_else(|| format!("task-{task_id}"), |t| t.slug.clone()),
  };
  if let Some((mut ch, _gen)) = current_child.take() {
    terminate_child(&mut ch);
  }
  if has_session {
    *overlay_active = false;
    *overlay_task_id = None;
    Ok(Some(task))
  } else {
    // Activate overlay mode; ratatui renderer handles display and input
    *overlay_active = true;
    *overlay_task_id = Some(task_id);
    Ok(None)
  }
}

fn terminate_child(child: &mut Child) {
  let pid = child.id();
  let _ = std::process::Command::new("kill")
    .arg("-TERM")
    .arg(pid.to_string())
    .status();
  let _ = child.kill();
  std::thread::sleep(std::time::Duration::from_millis(100));
}

// Helpers for clean decisions when the tmux attach child exits
enum FollowExitAction {
  CancelFollow,
  SwitchToOverlay(u32),
  Ignore,
}

fn has_session_for_task(ctx: &AppContext, task_id: u32) -> bool {
  match crate::utils::daemon::get_project_state(ctx) {
    Ok(state) => state.sessions.iter().any(|s| s.task.id == task_id),
    Err(_) => false,
  }
}

fn decide_follow_on_child_exit(
  ctx: &AppContext,
  child_gen: u64,
  observed_gen: u64,
  current_task_id: Option<u32>,
) -> FollowExitAction {
  if observed_gen != child_gen {
    return FollowExitAction::Ignore;
  }
  let Some(tid) = current_task_id else {
    return FollowExitAction::Ignore;
  };
  if has_session_for_task(ctx, tid) {
    FollowExitAction::CancelFollow
  } else {
    FollowExitAction::SwitchToOverlay(tid)
  }
}

fn activate_overlay(overlay_active: &mut bool, overlay_task_id: &mut Option<u32>, tid: u32) {
  *overlay_active = true;
  *overlay_task_id = Some(tid);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pick_tui_id_auto_and_errors() {
    let items: Vec<TuiListItem> = vec![];
    let err = pick_tui_id(None, &items).unwrap_err().to_string();
    assert!(err.contains("No TUI instances"));

    let one = vec![TuiListItem {
      tui_id: 7,
      pid: 123,
      focused_task_id: None,
    }];
    assert_eq!(pick_tui_id(None, &one).unwrap(), 7);

    let many = vec![
      TuiListItem {
        tui_id: 2,
        pid: 1,
        focused_task_id: None,
      },
      TuiListItem {
        tui_id: 1,
        pid: 2,
        focused_task_id: None,
      },
    ];
    let err2 = pick_tui_id(None, &many).unwrap_err().to_string();
    assert!(err2.contains("More than one TUI open"));
  }

  #[test]
  fn terminate_child_kills_process() {
    // Spawn a long-running process and ensure terminate_child stops it
    let mut ch = std::process::Command::new("bash")
      .arg("-lc")
      .arg("sleep 5")
      .spawn()
      .expect("spawn sleep");
    terminate_child(&mut ch);
    // Allow a short time then ensure process exited
    std::thread::sleep(std::time::Duration::from_millis(150));
    let st = ch.try_wait().expect("status");
    assert!(
      st.is_some(),
      "child should have exited after terminate_child"
    );
  }
}
