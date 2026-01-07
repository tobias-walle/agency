use std::collections::HashMap;
use std::io::{self, IsTerminal as _};
use std::time::{Duration, Instant};

use anyhow::{Context, Error, Result};
use crossbeam_channel::{Receiver, unbounded};
use crossterm::event::{
  self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

use super::command_log::CommandLogState;
use super::confirm_dialog::{ConfirmAction, ConfirmDialogState, ConfirmOutcome};
use super::file_input_overlay::{FileInputAction, FileInputState};
use super::files_overlay::{FilesOutcome, FilesOverlayState};
use super::help_bar::{
  self, HELP_ITEMS, HELP_ITEMS_FILES, HELP_ITEMS_FILE_INPUT, HELP_ITEMS_INPUT, HELP_ITEMS_LOG,
};
use super::task_input_overlay::{self, InputOverlayState};
use super::select_menu::{MenuOutcome, SelectMenuState};
use super::task_table::{self, TaskTableState};
use crate::commands::new;
use crate::commands::operations::{Operation, execute as execute_operation};
use crate::utils::files::{FileRef, add_file, add_file_from_bytes, files_dir_for_task};
use crate::utils::opener::open_with_default;
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
#[derive(Clone, Debug, Default)]
pub enum Mode {
  #[default]
  List,
  InputSlug,
  FilesOverlay(FilesOverlayState),
  FileInput(FileInputState),
  SelectMenu(SelectMenuState),
  ConfirmDialog(ConfirmDialogState),
}

/// Daemon events for the UI.
enum UiEvent {
  ProjectState,
  Disconnected(Error),
}

/// Connection status for daemon subscription.
enum SubscriptionStatus {
  Connected,
  Disconnected { since: Instant },
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
  subscription_status: SubscriptionStatus,
  events_rx: Option<Receiver<UiEvent>>,
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
      subscription_status: SubscriptionStatus::Connected,
      events_rx: None,
    }
  }
}

impl AppState {
  fn help_items_for_mode(&self) -> &'static [&'static str] {
    match self.mode {
      Mode::InputSlug | Mode::SelectMenu(_) => HELP_ITEMS_INPUT,
      Mode::FilesOverlay(_) => HELP_ITEMS_FILES,
      Mode::FileInput(_) => HELP_ITEMS_FILE_INPUT,
      Mode::List | Mode::ConfirmDialog(_) => match self.focus {
        Focus::Log => HELP_ITEMS_LOG,
        Focus::Tasks => HELP_ITEMS,
      },
    }
  }

  fn process_log_events(&mut self, log_rx: &Receiver<LogEvent>) {
    while let Ok(ev) = log_rx.try_recv() {
      self.command_log.push(ev);
    }
  }

  fn process_interactive_requests(
    &mut self,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ctx: &AppContext,
    irx: &Receiver<InteractiveReq>,
  ) -> Result<()> {
    while let Ok(req) = irx.try_recv() {
      self.handle_interactive_req(terminal, ctx, req)?;
    }
    Ok(())
  }

  fn process_daemon_events(&mut self, ctx: &AppContext) -> Result<()> {
    let Some(rx) = self.events_rx.take() else {
      return Ok(());
    };

    while let Ok(ev) = rx.try_recv() {
      self.handle_daemon_event(ctx, ev)?;
    }

    if self.events_rx.is_none()
      && matches!(self.subscription_status, SubscriptionStatus::Connected)
    {
      self.events_rx = Some(rx);
    }

    Ok(())
  }

  fn attempt_reconnection(&mut self, ctx: &AppContext) {
    let SubscriptionStatus::Disconnected { since } = self.subscription_status else {
      return;
    };

    if since.elapsed() <= Duration::from_secs(2) {
      return;
    }

    if let Ok(rx) = subscribe_events(ctx) {
      self.events_rx = Some(rx);
      self.subscription_status = SubscriptionStatus::Connected;
      self.refresh(ctx).ok();
      log_info!("Reconnected to daemon");
    } else {
      self.subscription_status = SubscriptionStatus::Disconnected { since: Instant::now() };
    }
  }

  fn handle_terminal_events(&mut self, ctx: &AppContext) -> Result<bool> {
    if !event::poll(Duration::from_millis(150))? {
      return Ok(true);
    }

    match event::read()? {
      Event::Key(key) => {
        if key.kind == KeyEventKind::Repeat {
          return Ok(true);
        }

        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
          return Ok(false);
        }

        let mode = self.mode.clone();
        match mode {
          Mode::List => {
            handle_list_mode(self, ctx, key);
          }
          Mode::InputSlug => {
            handle_input_mode(self, ctx, key);
          }
          Mode::FilesOverlay(overlay) => {
            handle_files_mode(self, ctx, overlay, key);
          }
          Mode::FileInput(input) => {
            handle_file_input_mode(self, ctx, input, key);
          }
          Mode::SelectMenu(menu) => {
            handle_menu_mode(self, menu, key);
          }
          Mode::ConfirmDialog(dialog) => {
            handle_confirm_mode(self, ctx, dialog, key);
          }
        }
      }
      Event::Mouse(mouse) => {
        self.command_log.handle_mouse_event(mouse);
      }
      _ => {}
    }

    Ok(true)
  }

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
        self.refresh_with_focus_change(ctx)?;
      }
    }
    Ok(())
  }

  fn handle_daemon_event(&mut self, ctx: &AppContext, ev: UiEvent) -> Result<(), Error> {
    match ev {
      UiEvent::ProjectState => {
        self.refresh_with_focus_change(ctx)?;
      }
      UiEvent::Disconnected(err) => {
        log_info!("Daemon connection lost: {}", err);
        self.subscription_status = SubscriptionStatus::Disconnected { since: Instant::now() };
        self.events_rx = None;
      }
    }
    Ok(())
  }

  fn draw(&mut self, f: &mut ratatui::Frame) {
    let help_items = self.help_items_for_mode();
    let help_lines = help_bar::layout_lines(help_items, f.area().width);
    let help_rows = help_lines.len().try_into().unwrap_or(1_u16).clamp(1, 3);

    let terminal_height = f.area().height;
    let log_height = self.command_log.effective_height(terminal_height, help_rows);

    let rects = ratatui::layout::Layout::vertical([
      Constraint::Fill(1),
      Constraint::Length(log_height),
      Constraint::Length(help_rows),
    ])
    .split(f.area());

    // Store border Y position for mouse hit detection
    self.command_log.set_border_y(rects[1].y);

    self
      .task_table
      .draw(f, rects[0], self.focus == Focus::Tasks);
    if log_height > 0 {
      self.command_log.draw(f, rects[1], self.focus == Focus::Log);
    }
    help_bar::draw_with_items(f, rects[2], help_items);

    if let Some(ref overlay) = self.input_overlay {
      overlay.draw(f, rects[0]);
    }

    if let Mode::FilesOverlay(ref overlay) = self.mode {
      overlay.draw(f, rects[0]);
    }

    if let Mode::FileInput(ref input) = self.mode {
      input.draw(f, rects[0]);
    }

    if let Mode::SelectMenu(ref menu) = self.mode {
      menu.draw(f, rects[0]);
    }

    if let Mode::ConfirmDialog(ref dialog) = self.mode {
      dialog.draw(f, rects[0]);
    }

    // Show disconnected indicator
    if matches!(self.subscription_status, SubscriptionStatus::Disconnected { .. }) {
      let indicator = Paragraph::new(Span::styled(
        " Disconnected ",
        Style::default().fg(Color::Black).bg(Color::Yellow),
      ));
      let area = f.area();
      let width = 14_u16;
      let indicator_rect = Rect::new(area.width.saturating_sub(width + 1), 0, width, 1);
      f.render_widget(indicator, indicator_rect);
    }
  }

  fn dispatch_action(&mut self, ctx: &AppContext, action: &task_table::Action) {
    match action {
      task_table::Action::None => {}
      task_table::Action::SelectionChanged { id } => {
        emit_focus_change(ctx, self.task_table.tui_id, Some(*id));
      }
      task_table::Action::EditOrAttach { id, session } => {
        spawn_edit_or_attach(ctx, *id, *session);
      }
      task_table::Action::NewTask { start_and_attach } => {
        self.input_overlay = Some(InputOverlayState::new(*start_and_attach, ctx));
        self.mode = Mode::InputSlug;
      }
      task_table::Action::StartTask { id } => {
        let id_str = id.to_string();
        let op = Operation::Start {
          task: id_str.clone(),
          attach: false,
        };
        self.command_log.push(LogEvent::Command(op.cli_command()));
        spawn_cmd(ctx, move |ctx| {
          if let Err(err) = execute_operation(&ctx, &op) {
            log_error!("Start failed: {}", err);
          }
        });
      }
      task_table::Action::StopTask { id } => {
        let id_str = id.to_string();
        let op = Operation::Stop {
          task: Some(id_str.clone()),
          session_id: None,
        };
        self.command_log.push(LogEvent::Command(op.cli_command()));
        spawn_cmd(ctx, move |ctx| {
          if let Err(err) = execute_operation(&ctx, &op) {
            log_error!("Stop failed: {}", err);
          }
        });
      }
      task_table::Action::MergeTask { id } => {
        let id_str = id.to_string();
        let op = Operation::Merge {
          task: id_str.clone(),
          base: None,
        };
        self.command_log.push(LogEvent::Command(op.cli_command()));
        spawn_cmd(ctx, move |ctx| {
          if let Err(err) = execute_operation(&ctx, &op) {
            log_error!("Merge failed: {}", err);
          }
        });
      }
      task_table::Action::CompleteTask { id } => {
        let dialog = ConfirmDialogState::new(
          "Complete Task",
          "Merge into base and delete task?",
          ConfirmAction::CompleteTask { id: *id },
        );
        self.mode = Mode::ConfirmDialog(dialog);
      }
      task_table::Action::OpenTask { id } => {
        let op = Operation::Open {
          task: id.to_string(),
        };
        spawn_cmd(ctx, move |ctx| {
          let _ = execute_operation(&ctx, &op);
        });
      }
      task_table::Action::ShellTask { id } => {
        let op = Operation::Shell {
          task: id.to_string(),
        };
        spawn_cmd(ctx, move |ctx| {
          let _ = execute_operation(&ctx, &op);
        });
      }
      task_table::Action::DeleteTask { id } => {
        let id_str = id.to_string();
        let op = Operation::Remove {
          task: id_str.clone(),
        };
        self.command_log.push(LogEvent::Command(op.cli_command()));
        self.task_table.mark_pending_delete(*id);
        spawn_cmd(ctx, move |ctx| {
          let _ = execute_operation(&ctx, &op);
        });
      }
      task_table::Action::ResetTask { id } => {
        let id_str = id.to_string();
        let op = Operation::Reset {
          task: id_str.clone(),
        };
        self.command_log.push(LogEvent::Command(op.cli_command()));
        spawn_cmd(ctx, move |ctx| {
          if let Err(err) = execute_operation(&ctx, &op) {
            log_error!("Reset failed: {}", err);
          }
        });
      }
      task_table::Action::OpenFilesOverlay { task } => {
        let overlay = FilesOverlayState::new(&ctx.paths, task.clone());
        self.mode = Mode::FilesOverlay(overlay);
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

  fn refresh_with_focus_change(&mut self, ctx: &AppContext) -> Result<()> {
    let prev_sel_id = self.task_table.selected_row().map(TaskRow::id);
    self.refresh(ctx).map_err(|err| {
      log_error!("{}", err);
      err
    })?;
    self.task_table.prune_pending_deletes();
    let cur_sel_id = self.task_table.selected_row().map(TaskRow::id);
    if prev_sel_id != cur_sel_id {
      emit_focus_change(ctx, self.task_table.tui_id, cur_sel_id);
    }
    Ok(())
  }
}

/// Entry point for the TUI.
pub(crate) fn run(ctx: &AppContext) -> Result<()> {
  if !io::stdout().is_terminal() {
    log_info!("TUI requires a TTY; try 'agency ps' or a real terminal");
    return Ok(());
  }

  connect_daemon(ctx)?;

  enable_raw_mode().context("enable raw mode")?;
  let mut stdout = io::stdout();
  crossterm::execute!(
    stdout,
    crossterm::terminal::EnterAlternateScreen,
    EnableMouseCapture
  )
  .context("enter alternate screen")?;
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend).context("create terminal")?;

  let res = ui_loop(&mut terminal, ctx);

  let out = terminal.backend_mut();
  crossterm::execute!(
    out,
    DisableMouseCapture,
    crossterm::terminal::LeaveAlternateScreen
  )
  .ok();
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

  // Send initial focus (even if None for empty task list)
  if !state.sent_initial_focus {
    let task_id = state.task_table.selected_row().map(TaskRow::id);
    emit_focus_change(ctx, state.task_table.tui_id, task_id);
    state.sent_initial_focus = true;
  }

  state.events_rx = subscribe_events(ctx)
    .map_err(|err| {
      log_error!("{}", err);
      err
    })
    .ok();

  let (log_tx, log_rx) = unbounded::<LogEvent>();
  set_log_sink(log_tx);

  let (itx, irx) = unbounded::<InteractiveReq>();
  register_interactive_sender(itx);

  loop {
    state.process_log_events(&log_rx);
    state.process_interactive_requests(terminal, ctx, &irx)?;

    if state.paused {
      std::thread::sleep(Duration::from_millis(50));
      continue;
    }

    state.task_table.prune_pending_deletes();
    terminal.draw(|f| state.draw(f))?;

    state.process_daemon_events(ctx)?;
    state.attempt_reconnection(ctx);

    if !state.handle_terminal_events(ctx)? {
      break;
    }
  }

  clear_log_sink();
  Ok(())
}

fn handle_list_mode(state: &mut AppState, ctx: &AppContext, key: crossterm::event::KeyEvent) {
  // Focus switching and global commands
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
    KeyCode::Char('H') => {
      // Toggle command log visibility
      state.command_log.toggle_visibility();
      // If hiding, switch focus to Tasks
      if !state.command_log.is_visible() && state.focus == Focus::Log {
        state.focus = Focus::Tasks;
      }
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
    task_input_overlay::Action::None => {}
    task_input_overlay::Action::Cancel => {
      state.mode = Mode::List;
      state.input_overlay = None;
    }
    task_input_overlay::Action::OpenAgentMenu => {
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
    task_input_overlay::Action::Submit {
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
          move || match new::run(&ctx, &slug, agent.as_deref(), Some(""), false, &[]) {
            Ok(created) => {
              let id_str = created.id.to_string();
              let op = Operation::Start {
                task: id_str.clone(),
                attach: true,
              };
              if let Err(err) = execute_operation(&ctx, &op) {
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
            let _ = new::run(&ctx, &slug, agent.as_deref(), None, false, &[]);
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

fn handle_confirm_mode(
  state: &mut AppState,
  ctx: &AppContext,
  mut dialog: ConfirmDialogState,
  key: crossterm::event::KeyEvent,
) {
  match dialog.handle_key(key) {
    ConfirmOutcome::Continue => {
      state.mode = Mode::ConfirmDialog(dialog);
    }
    ConfirmOutcome::Canceled => {
      state.mode = Mode::List;
    }
    ConfirmOutcome::Confirmed => {
      state
        .command_log
        .push(LogEvent::Command(dialog.action.command_log()));
      state.task_table.mark_pending_delete(dialog.action.task_id());
      execute_confirm_action(ctx, &dialog.action);
      state.mode = Mode::List;
    }
  }
}

fn handle_files_mode(
  state: &mut AppState,
  ctx: &AppContext,
  mut overlay: FilesOverlayState,
  key: crossterm::event::KeyEvent,
) {
  match overlay.handle_key(key) {
    FilesOutcome::Continue => {
      state.mode = Mode::FilesOverlay(overlay);
    }
    FilesOutcome::Canceled => {
      state.mode = Mode::List;
    }
    FilesOutcome::OpenFile(file) => {
      let path = crate::utils::files::file_path(&ctx.paths, &overlay.task, &file);
      if let Err(err) = open_with_default(&path) {
        log_error!("Failed to open file: {}", err);
      }
      state.mode = Mode::FilesOverlay(overlay);
    }
    FilesOutcome::OpenDirectory => {
      let dir = files_dir_for_task(&ctx.paths, &overlay.task);
      if let Err(err) = open_with_default(&dir) {
        log_error!("Failed to open directory: {}", err);
      }
      state.mode = Mode::FilesOverlay(overlay);
    }
    FilesOutcome::PasteClipboard => {
      state.command_log.push(LogEvent::Command(format!(
        "agency files add {} --from-clipboard",
        overlay.task.id
      )));
      spawn_paste_clipboard(ctx, overlay.task.clone());
      state.mode = Mode::FilesOverlay(overlay);
    }
    FilesOutcome::RemoveFile(file) => {
      state.command_log.push(LogEvent::Command(format!(
        "agency files rm {} {}",
        overlay.task.id, file.id
      )));
      if let Err(err) = crate::utils::files::remove_file(&ctx.paths, &overlay.task, &file) {
        log_error!("Remove file failed: {}", err);
      } else {
        log_info!("Removed file {} {}", file.id, file.name);
        let _ = crate::utils::daemon::notify_tasks_changed(ctx);
      }
      overlay.refresh(&ctx.paths);
      state.mode = Mode::FilesOverlay(overlay);
    }
    FilesOutcome::EditFile(file) => {
      let task = overlay.task.clone();
      spawn_edit_file(ctx, task, file);
      state.mode = Mode::FilesOverlay(overlay);
    }
    FilesOutcome::AddFile => {
      let input = FileInputState::new(overlay.task.clone());
      state.mode = Mode::FileInput(input);
    }
  }
}

fn handle_file_input_mode(
  state: &mut AppState,
  ctx: &AppContext,
  mut input: FileInputState,
  key: crossterm::event::KeyEvent,
) {
  match input.handle_key(key) {
    FileInputAction::None => {
      state.mode = Mode::FileInput(input);
    }
    FileInputAction::Cancel => {
      // Return to files overlay
      let overlay = FilesOverlayState::new(&ctx.paths, input.task);
      state.mode = Mode::FilesOverlay(overlay);
    }
    FileInputAction::Submit { path } => {
      state.command_log.push(LogEvent::Command(format!(
        "agency files add {} {}",
        input.task.id,
        path.display()
      )));
      let task = input.task.clone();
      match add_file(&ctx.paths, &task, &path) {
        Ok(file_ref) => {
          log_info!("Added file {} {}", file_ref.id, file_ref.name);
          let _ = crate::utils::daemon::notify_tasks_changed(ctx);
        }
        Err(err) => {
          log_error!("Failed to add file: {}", err);
        }
      }
      // Return to files overlay with refreshed list
      let overlay = FilesOverlayState::new(&ctx.paths, task);
      state.mode = Mode::FilesOverlay(overlay);
    }
  }
}

fn execute_confirm_action(ctx: &AppContext, action: &ConfirmAction) {
  match *action {
    ConfirmAction::CompleteTask { id } => {
      let id_str = id.to_string();
      let op = Operation::Complete {
        task: id_str.clone(),
        base: None,
      };
      spawn_cmd(ctx, move |ctx| {
        if let Err(err) = execute_operation(&ctx, &op) {
          log_error!("Complete failed: {}", err);
        }
      });
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

fn spawn_paste_clipboard(ctx: &AppContext, task: TaskRef) {
  let ctx = ctx.clone();
  std::thread::spawn(move || {
    let data = match crate::utils::clipboard::read_image_from_clipboard() {
      Ok(data) => data,
      Err(err) => {
        log_error!("Clipboard: {}", err);
        return;
      }
    };
    let timestamp = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_millis())
      .unwrap_or(0);
    let name = format!("screenshot-{timestamp}.png");
    match add_file_from_bytes(&ctx.paths, &task, &name, &data) {
      Ok(file_ref) => {
        log_info!("Added file {} {}", file_ref.id, file_ref.name);
        let _ = crate::utils::daemon::notify_tasks_changed(&ctx);
      }
      Err(err) => {
        log_error!("Failed to add file: {}", err);
      }
    }
  });
}

fn spawn_edit_or_attach(ctx: &AppContext, id: u32, session: Option<u64>) {
  let ctx = ctx.clone();
  std::thread::spawn(move || {
    if let Some(sid) = session {
      let op = Operation::Attach { session_id: sid };
      let _ = execute_operation(&ctx, &op);
    } else {
      let op = Operation::Edit {
        task: id.to_string(),
      };
      let _ = execute_operation(&ctx, &op);
    }
  });
}

fn spawn_edit_file(ctx: &AppContext, task: TaskRef, file: FileRef) {
  let ctx = ctx.clone();
  std::thread::spawn(move || {
    let path = crate::utils::files::file_path(&ctx.paths, &task, &file);
    log_info!("Editing file {} {}", file.id, file.name);
    if let Err(err) = crate::utils::editor::open_path(&ctx.config, &path, ctx.paths.root()) {
      log_error!("Edit failed: {}", err);
    }
  });
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
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
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

fn emit_focus_change(ctx: &AppContext, tui_id: Option<u32>, task_id: Option<u32>) {
  let Some(tid) = tui_id else { return };
  let Ok(repo) = open_main_repo(ctx.paths.root()) else {
    return;
  };
  let root = repo_workdir_or(&repo, ctx.paths.root());
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
