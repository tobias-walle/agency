//! Terminal User Interface (TUI) components for Agency.
//! Provides an interactive dashboard for managing tasks, sessions, and monitoring project state through a Ratatui-based interface.

mod app;
mod colors;
mod command_log;
mod confirm_dialog;
mod file_input_overlay;
mod files_overlay;
mod help_bar;
mod layout;
mod select_menu;
mod task_input_overlay;
mod task_table;
mod text_input;

pub(crate) use app::run;
