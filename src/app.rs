use crate::ssh_config::{SshConfigFile, SshHostEntry};
use crate::ui::{draw_ui, Event, UiAction};
use anyhow::{Context, Result};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::process::Command;

pub fn run() -> Result<()> {
    let mut ssh_cfg = SshConfigFile::load_default()?;
    let mut state = AppState::new(ssh_cfg.list_hosts());

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    loop {
        if state.needs_full_redraw {
            terminal.clear()?;
            state.needs_full_redraw = false;
        }
        terminal.draw(|f| draw_ui(f, &state))?;

        match ui::read_event()? {
            Event::Action(action) => match handle_action(action, &mut state, &mut ssh_cfg)? {
                LoopControl::Continue => {}
                LoopControl::Exit => break,
                LoopControl::Launch(host) => {
                    // Tear down TUI before launching ssh
                    teardown_terminal(&mut terminal)?;
                    launch_ssh(&host)?;
                    // Re-init terminal to return to app after ssh exits
                    reinit_terminal(&mut terminal)?;
                }
            },
            Event::Tick => {}
        }
    }

    teardown_terminal(&mut terminal)?;
    Ok(())
}

fn teardown_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

fn reinit_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    *terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct AppState {
    pub hosts: Vec<SshHostEntry>,
    pub filtered_hosts: Vec<usize>,
    pub selected_index: usize,
    pub filter_text: String,
    pub mode: Mode,
    pub needs_full_redraw: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Filter,
    Confirm(ConfirmContext),
    Editing,
    Help,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfirmContext {
    Delete { pattern: String },
}

impl AppState {
    pub fn new(hosts: Vec<SshHostEntry>) -> Self {
        let filtered_hosts = (0..hosts.len()).collect();
        Self {
            hosts,
            filtered_hosts,
            selected_index: 0,
            filter_text: String::new(),
            mode: Mode::Normal,
            needs_full_redraw: false,
        }
    }

    pub fn selected_host(&self) -> Option<&SshHostEntry> {
        self.filtered_hosts
            .get(self.selected_index)
            .and_then(|&idx| self.hosts.get(idx))
    }

    pub fn apply_filter(&mut self) {
        if self.filter_text.is_empty() {
            self.filtered_hosts = (0..self.hosts.len()).collect();
        } else {
            let query = self.filter_text.to_lowercase();
            self.filtered_hosts = self
                .hosts
                .iter()
                .enumerate()
                .filter(|(_, h)| h.matches(&query))
                .map(|(i, _)| i)
                .collect();
        }
        if self.selected_index >= self.filtered_hosts.len() {
            self.selected_index = self.filtered_hosts.len().saturating_sub(1);
        }
    }
}

pub enum LoopControl {
    Continue,
    Exit,
    Launch(String),
}

fn handle_action(action: UiAction, state: &mut AppState, ssh_cfg: &mut SshConfigFile) -> Result<LoopControl> {
    use UiAction::*;
    match action {
        MoveUp => {
            state.selected_index = state.selected_index.saturating_sub(1);
        }
        MoveDown => {
            if state.selected_index + 1 < state.filtered_hosts.len() {
                state.selected_index += 1;
            }
        }
        PageUp => {
            state.selected_index = state.selected_index.saturating_sub(10);
        }
        PageDown => {
            state.selected_index = (state.selected_index + 10).min(state.filtered_hosts.len().saturating_sub(1));
        }
        BeginFilter => {
            state.mode = Mode::Filter;
        }
        InputChar(ch) => {
            match &state.mode {
                Mode::Filter => {
                    state.filter_text.push(ch);
                    state.apply_filter();
                }
                Mode::Confirm(ctx) => {
                    match ch {
                        'y' | 'Y' => {
                            if let ConfirmContext::Delete { pattern } = ctx.clone() {
                                ssh_cfg.delete_host(&pattern)?;
                                state.hosts = ssh_cfg.list_hosts();
                                state.apply_filter();
                                state.mode = Mode::Normal;
                                state.needs_full_redraw = true;
                            }
                        }
                        'n' | 'N' => {
                            state.mode = Mode::Normal;
                            state.needs_full_redraw = true;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        BackspaceFilter => {
            if matches!(state.mode, Mode::Filter) {
                state.filter_text.pop();
                state.apply_filter();
            }
        }
        ClearFilter => {
            match &state.mode {
                Mode::Filter => {
                    state.filter_text.clear();
                    state.apply_filter();
                    state.mode = Mode::Normal;
                }
                Mode::Confirm(_) => {
                    state.mode = Mode::Normal;
                    state.needs_full_redraw = true;
                }
                _ => {}
            }
        }
        EditSelected => {
            if let Some(entry) = state.selected_host().cloned() {
                let updated = ui::edit_host_dialog(&entry)?;
                ssh_cfg.upsert_host(&updated)?;
                state.hosts = ssh_cfg.list_hosts();
                state.apply_filter();
                state.needs_full_redraw = true;
            }
        }
        NewHost => {
            let new_entry = ui::new_host_dialog()?;
            ssh_cfg.upsert_host(&new_entry)?;
            state.hosts = ssh_cfg.list_hosts();
            state.apply_filter();
            state.needs_full_redraw = true;
        }
        DeleteSelected => {
            if let Some(entry) = state.selected_host().cloned() {
                state.mode = Mode::Confirm(ConfirmContext::Delete { pattern: entry.pattern });
                state.needs_full_redraw = true;
            }
        }
        LaunchSelected => {
            if matches!(state.mode, Mode::Confirm(_)) {
                // ignore Enter while confirming
            } else if let Some(entry) = state.selected_host() {
                return Ok(LoopControl::Launch(entry.pattern.clone()));
            }
        }
        Quit => return Ok(LoopControl::Exit),
        Noop => {}
    }
    Ok(LoopControl::Continue)
}

fn launch_ssh(host_pattern: &str) -> Result<()> {
    // Let user's ssh config resolve the final host; rely on external ssh binary
    let status = Command::new("ssh").arg(host_pattern).status().context("failed to spawn ssh")?;
    if !status.success() {
        eprintln!("ssh exited with status: {}", status);
    }
    Ok(())
}

mod ui {
    pub use crate::ui::{draw_ui, edit_host_dialog, new_host_dialog, read_event, Event, UiAction};
}


