use std::collections::HashMap;
use std::io::{self, IsTerminal as _};
use std::time::Duration;

use anyhow::{Context, Error, Result};
use crossbeam_channel::{Receiver, unbounded};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;

use super::command_log::CommandLogState;
use super::help_bar::{self, HELP_ITEMS};
use super::input_overlay::{self, InputOverlayState};
use super::select_menu::{MenuOutcome, SelectMenuState};
use super::task_table::{self, TaskTableState};
use crate::commands::{attach, edit, merge, new, open, reset, rm, shell, start, stop};
use crate::config::{AppContext, compute_socket_path};
use crate::daemon_protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, read_frame, write_frame,
};
use crate::utils::daemon::{
  connect_daemon, get_project_state, send_message_to_daemon, tui_register, tui_unregister,
};
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::interactive::{InteractiveReq, register_sender as register_interactive_sender};
use crate::utils::log::{LogEvent, clear_log_sink, set_log_sink};
use crate::utils::task::TaskRef;
use crate::utils::task_columns::{GitMetrics, TaskRow};
use crate::utils::term::restore_terminal_state;
use crate::{log_error, log_info};

/// Which pane is focused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Focus {
  #[default]
  Tasks,
  Log,
}

/// Current UI mode.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Mode {
  #[default]
  List,
  InputSlug,
  SelectMenu(SelectMenuState),
}

/// Daemon events for the UI.
enum UiEvent {
  ProjectState,
  Disconnected(Error),
}

/// Main application state composing all component states.
struct AppState {
  task_table: TaskTableState,
  command_log: CommandLogState,
  input_overlay: Option<InputOverlayState>,
  focus: Focus,
  mode: Mode,
  paused: bool,
  sent_initial_focus: bool,
}

impl Default for AppState {
  fn default() -> Self {
    Self {
      task_table: TaskTableState::new(),
      command_log: CommandLogState::new(),
      input_overlay: None,
      focus: Focus::Tasks,
      mode: Mode::List,
      paused: false,
      sent_initial_focus: false,
    }
  }
}

impl AppState {
  fn handle_interactive_req(
    &mut self,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ctx: &AppContext,
    req: InteractiveReq,
  ) -> Result<()> {
    match req {
      InteractiveReq::Begin { ack } => {
        restore_terminal(terminal)?;
        self.paused = true;
        let _ = ack.send(());
      }
      InteractiveReq::End { ack } => {
        reinit_terminal(terminal)?;
        self.paused = false;
        let _ = ack.send(());
        let prev_sel_id = self.task_table.selected_row().map(TaskRow::id);
        self.refresh(ctx).map_err(|err| {
          log_error!("{}", err);
          err
        })?;
        self.task_table.prune_pending_deletes();
        let cur_sel_id = self.task_table.selected_row().map(TaskRow::id);
        if let (Some(prev), Some(cur)) = (prev_sel_id, cur_sel_id)
          && prev != cur
        {
          emit_focus_change(ctx, self.task_table.tui_id, cur);
        }
      }
    }
    Ok(())
  }

  fn handle_daemon_event(&mut self, ctx: &AppContext, ev: UiEvent) -> Result<(), Error> {
    match ev {
      UiEvent::ProjectState => {
        let prev_sel_id = self.task_table.selected_row().map(TaskRow::id);
        self.refresh(ctx).map_err(|err| {
          log_error!("{}", err);
          err
        })?;
        self.task_table.prune_pending_deletes();
        let cur_sel_id = self.task_table.selected_row().map(TaskRow::id);
        if let (Some(prev), Some(cur)) = (prev_sel_id, cur_sel_id)
          && prev != cur
        {
          emit_focus_change(ctx, self.task_table.tui_id, cur);
        }
      }
      UiEvent::Disconnected(err) => {
        log_error!("{}", err);
        return Err(err);
      }
    }
    Ok(())
  }

  fn draw(&self, f: &mut ratatui::Frame) {
    let help_lines = help_bar::layout_lines(HELP_ITEMS, f.area().width);
    let help_rows = help_lines.len().try_into().unwrap_or(1_u16).clamp(1, 3);

    let rects = ratatui::layout::Layout::vertical([
      Constraint::Fill(1),
      Constraint::Length(7),
      Constraint::Length(help_rows),
    ])
    .split(f.area());

    self
      .task_table
      .draw(f, rects[0], self.focus == Focus::Tasks);
    self.command_log.draw(f, rects[1], self.focus == Focus::Log);
    help_bar::draw(f, rects[2]);

    if let Some(ref overlay) = self.input_overlay {
      overlay.draw(f, rects[0]);
    }

    if let Mode::SelectMenu(ref menu) = self.mode {
      menu.draw(f, rects[0]);
    }
  }

  fn dispatch_action(&mut self, ctx: &AppContext, action: &task_table::Action) {
    match action {
      task_table::Action::None => {}
      task_table::Action::SelectionChanged { id } => {
        emit_focus_change(ctx, self.task_table.tui_id, *id);
      }
      task_table::Action::EditOrAttach { id, session } => {
        let id = *id;
        let session = *session;
        std::thread::spawn({
          let ctx = ctx.clone();
          move || {
            if let Some(sid) = session {
              let _ = attach::run_join_session(&ctx, sid);
            } else {
              let _ = edit::run(&ctx, &id.to_string());
            }
          }
        });
      }
      task_table::Action::NewTask { start_and_attach } => {
        self.input_overlay = Some(InputOverlayState::new(*start_and_attach, ctx));
        self.mode = Mode::InputSlug;
      }
      task_table::Action::StartTask { id } => {
        let id_str = id.to_string();
        self.command_log.push(LogEvent::Command(format!(
          "agency start --no-attach {id_str}"
        )));
        spawn_cmd(ctx, move |ctx| {
          if let Err(err) = start::run_with_attach(&ctx, &id_str, false) {
            log_error!("Start failed: {}", err);
          }
        });
      }
      task_table::Action::StopTask { id } => {
        let id = *id;
        self
          .command_log
          .push(LogEvent::Command(format!("agency stop --task {id}")));
        spawn_cmd(ctx, move |ctx| {
          if let Err(err) = stop::run(&ctx, Some(&id.to_string()), None) {
            log_error!("Stop failed: {}", err);
          }
        });
      }
      task_table::Action::MergeTask { id } => {
        let id_str = id.to_string();
        self
          .command_log
          .push(LogEvent::Command(format!("agency merge {id_str}")));
        spawn_cmd(ctx, move |ctx| {
          if let Err(err) = merge::run(&ctx, &id_str, None) {
            log_error!("Merge failed: {}", err);
          }
        });
      }
      task_table::Action::OpenTask { id } => {
        let id = *id;
        spawn_cmd(ctx, move |ctx| {
          let _ = open::run(&ctx, &id.to_string());
        });
      }
      task_table::Action::ShellTask { id } => {
        let id = *id;
        spawn_cmd(ctx, move |ctx| {
          let _ = shell::run(&ctx, &id.to_string());
        });
      }
      task_table::Action::DeleteTask { id } => {
        let id = *id;
        self
          .command_log
          .push(LogEvent::Command(format!("agency rm {id}")));
        self.task_table.mark_pending_delete(id);
        spawn_cmd(ctx, move |ctx| {
          let _ = rm::run_force(&ctx, &id.to_string());
        });
      }
      task_table::Action::ResetTask { id } => {
        let id = *id;
        self
          .command_log
          .push(LogEvent::Command(format!("agency reset {id}")));
        spawn_cmd(ctx, move |ctx| {
          if let Err(err) = reset::run(&ctx, &id.to_string()) {
            log_error!("Reset failed: {}", err);
          }
        });
      }
    }
  }

  fn refresh(&mut self, ctx: &AppContext) -> Result<()> {
    let snap = get_project_state(ctx)?;
    let git_metrics: HashMap<TaskRef, GitMetrics> = snap
      .metrics
      .into_iter()
      .map(|m| {
        (
          TaskRef::from(m.task),
          GitMetrics {
            uncommitted_add: m.uncommitted_add,
            uncommitted_del: m.uncommitted_del,
            commits_ahead: m.commits_ahead,
          },
        )
      })
      .collect();
    self.task_table.refresh(ctx, &snap.sessions, &git_metrics)?;
    Ok(())
  }
}

/// Entry point for the TUI.
pub fn run(ctx: &AppContext) -> Result<()> {
  if !io::stdout().is_terminal() {
    log_info!("TUI requires a TTY; try 'agency ps' or a real terminal");
    return Ok(());
  }

  connect_daemon(ctx)?;

  enable_raw_mode().context("enable raw mode")?;
  let mut stdout = io::stdout();
  crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)
    .context("enter alternate screen")?;
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend).context("create terminal")?;

  let res = ui_loop(&mut terminal, ctx);

  let out = terminal.backend_mut();
  crossterm::execute!(out, crossterm::terminal::LeaveAlternateScreen).ok();
  disable_raw_mode().ok();
  restore_terminal_state();
  let _ = tui_unregister(ctx, std::process::id());

  res
}

fn ui_loop(
  terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
  ctx: &AppContext,
) -> Result<()> {
  let mut state = AppState::default();
  state.refresh(ctx).map_err(|err| {
    log_error!("{}", err);
    err
  })?;

  // Register TUI with daemon
  if let Ok(id) = tui_register(ctx, std::process::id()) {
    state.task_table.tui_id = Some(id);
  }

  // Send initial focus
  if !state.sent_initial_focus
    && let Some(cur) = state.task_table.selected_row()
  {
    emit_focus_change(ctx, state.task_table.tui_id, cur.id());
    state.sent_initial_focus = true;
  }

  let events_rx = subscribe_events(ctx).map_err(|err| {
    log_error!("{}", err);
    err
  })?;

  let (log_tx, log_rx) = unbounded::<LogEvent>();
  set_log_sink(log_tx);

  let (itx, irx) = unbounded::<InteractiveReq>();
  register_interactive_sender(itx);

  loop {
    // Drain routed logs
    while let Ok(ev) = log_rx.try_recv() {
      state.command_log.push(ev);
    }

    while let Ok(req) = irx.try_recv() {
      state.handle_interactive_req(terminal, ctx, req)?;
    }

    if state.paused {
      std::thread::sleep(Duration::from_millis(50));
      continue;
    }

    state.task_table.prune_pending_deletes();

    terminal.draw(|f| state.draw(f))?;

    while let Ok(ev) = events_rx.try_recv() {
      state.handle_daemon_event(ctx, ev)?;
    }

    // Handle key events
    if event::poll(Duration::from_millis(150))?
      && let Event::Key(key) = event::read()?
    {
      if key.kind == KeyEventKind::Repeat {
        continue;
      }

      // Global quit
      if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        break;
      }

      let mode = state.mode.clone();
      match mode {
        Mode::List => {
          handle_list_mode(&mut state, ctx, key);
        }
        Mode::InputSlug => {
          handle_input_mode(&mut state, ctx, key);
        }
        Mode::SelectMenu(menu) => {
          handle_menu_mode(&mut state, menu, key);
        }
      }
    }
  }

  clear_log_sink();
  Ok(())
}

fn handle_list_mode(state: &mut AppState, ctx: &AppContext, key: crossterm::event::KeyEvent) {
  // Focus switching
  match key.code {
    KeyCode::Char('1') => {
      state.focus = Focus::Tasks;
      state.command_log.reset_scroll();
      return;
    }
    KeyCode::Char('2') => {
      state.focus = Focus::Log;
      return;
    }
    _ => {}
  }

  match state.focus {
    Focus::Tasks => {
      let action = state.task_table.handle_key(key);
      state.dispatch_action(ctx, &action);
    }
    Focus::Log => {
      state.command_log.handle_key(key);
    }
  }
}

fn handle_input_mode(state: &mut AppState, ctx: &AppContext, key: crossterm::event::KeyEvent) {
  let Some(ref mut overlay) = state.input_overlay else {
    state.mode = Mode::List;
    return;
  };

  let action = overlay.handle_key(key);
  match action {
    input_overlay::Action::None => {}
    input_overlay::Action::Cancel => {
      state.mode = Mode::List;
      state.input_overlay = None;
    }
    input_overlay::Action::OpenAgentMenu => {
      let mut items: Vec<String> = ctx.config.agents.keys().cloned().collect();
      items.sort();
      let pre = overlay
        .selected_agent
        .as_ref()
        .and_then(|cur| items.iter().position(|s| s == cur))
        .unwrap_or(0);
      let menu = SelectMenuState::new("Select agent", items, pre);
      state.mode = Mode::SelectMenu(menu);
    }
    input_overlay::Action::Submit {
      slug,
      agent,
      start_and_attach,
    } => {
      if start_and_attach {
        state
          .command_log
          .push(LogEvent::Command(format!("agency new {slug} + start")));
        std::thread::spawn({
          let ctx = ctx.clone();
          move || match new::run(&ctx, &slug, agent.as_deref(), Some(""), false) {
            Ok(created) => {
              let id_str = created.id.to_string();
              if let Err(err) = start::run_with_attach(&ctx, &id_str, true) {
                log_error!("Start+attach failed: {}", err);
              }
            }
            Err(err) => {
              log_error!("New failed: {}", err);
            }
          }
        });
      } else {
        std::thread::spawn({
          let ctx = ctx.clone();
          move || {
            let _ = new::run(&ctx, &slug, agent.as_deref(), None, false);
          }
        });
      }
      state.mode = Mode::List;
      state.input_overlay = None;
      let _ = state.refresh(ctx);
    }
  }
}

fn handle_menu_mode(
  state: &mut AppState,
  mut menu: SelectMenuState,
  key: crossterm::event::KeyEvent,
) {
  match menu.handle_key(key) {
    MenuOutcome::Continue => {
      state.mode = Mode::SelectMenu(menu);
    }
    MenuOutcome::Canceled => {
      state.mode = Mode::InputSlug;
    }
    MenuOutcome::Selected(idx) => {
      if idx < menu.items.len()
        && let Some(ref mut overlay) = state.input_overlay
      {
        overlay.set_agent(menu.items[idx].clone());
      }
      state.mode = Mode::InputSlug;
    }
  }
}

fn spawn_cmd<F>(ctx: &AppContext, f: F)
where
  F: FnOnce(AppContext) + Send + 'static,
{
  let ctx = ctx.clone();
  std::thread::spawn(move || f(ctx));
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
  let out = terminal.backend_mut();
  crossterm::execute!(out, crossterm::terminal::LeaveAlternateScreen)
    .context("leave alternate screen")?;
  disable_raw_mode().context("disable raw mode")?;
  Ok(())
}

fn reinit_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
  let out = terminal.backend_mut();
  crossterm::execute!(out, crossterm::terminal::EnterAlternateScreen)
    .context("re-enter alternate screen")?;
  enable_raw_mode().context("re-enable raw mode")?;
  terminal
    .clear()
    .context("clear terminal after interactive end")?;
  Ok(())
}

fn subscribe_events(ctx: &AppContext) -> Result<Receiver<UiEvent>> {
  let (tx, rx) = unbounded::<UiEvent>();
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  let stream = connect_daemon(ctx)?;
  std::thread::Builder::new()
    .name("tui-subscribe".to_string())
    .spawn(move || {
      let tx_events = tx;
      let mut stream = stream;
      if let Err(err) = write_frame(
        &mut stream,
        &C2D::Control(C2DControl::SubscribeEvents { project }),
      ) {
        let _ = tx_events.send(UiEvent::Disconnected(err));
        return;
      }
      loop {
        let msg: Result<D2C> = read_frame(&mut stream);
        match msg {
          Ok(D2C::Control(D2CControl::ProjectState { .. })) => {
            let _ = tx_events.send(UiEvent::ProjectState);
          }
          Ok(D2C::Control(_)) => {}
          Err(err) => {
            let _ = tx_events.send(UiEvent::Disconnected(err));
            break;
          }
        }
      }
    })?;
  Ok(rx)
}

fn emit_focus_change(ctx: &AppContext, tui_id: Option<u32>, task_id: u32) {
  let Some(tid) = tui_id else { return };
  let Ok(repo) = open_main_repo(ctx.paths.cwd()) else {
    return;
  };
  let root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: root.display().to_string(),
  };
  let socket = compute_socket_path(&ctx.config);
  let _ = send_message_to_daemon(
    &socket,
    C2DControl::TuiFocusTaskChange {
      project,
      tui_id: tid,
      task_id,
    },
  );
}
