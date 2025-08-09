use crate::ssh_config::{SshConfigFile, SshHostEntry};
use crate::ui::UiAction;
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
        terminal.draw(|f| crate::ui::draw_ui(f, &state))?;

        match ui::read_event(&state.mode)? {
            crate::ui::Event::Action(action) => match handle_action(action, &mut state, &mut ssh_cfg)? {
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
            crate::ui::Event::Tick => {}
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
    execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
    terminal.clear()?;
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
    EditForm(FormData),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfirmContext {
    Delete { pattern: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormData {
    pub is_editing: bool,  // true for edit, false for new
    pub pattern: String,
    pub hostname: String,
    pub user: String,
    pub port: String,
    pub current_field: usize,  // 0=pattern, 1=hostname, 2=user, 3=port
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
            match &mut state.mode {
                Mode::Filter => {
                    state.filter_text.push(ch);
                    state.apply_filter();
                }
                Mode::Confirm(ctx) => {
                    match ch {
                        'y' | 'Y' => {
                            let ConfirmContext::Delete { pattern } = ctx.clone();
                            ssh_cfg.delete_host(&pattern)?;
                            state.hosts = ssh_cfg.list_hosts();
                            state.apply_filter();
                            state.mode = Mode::Normal;
                            state.needs_full_redraw = true;
                        }
                        'n' | 'N' => {
                            state.mode = Mode::Normal;
                            state.needs_full_redraw = true;
                        }
                        _ => {}
                    }
                }
                Mode::EditForm(form) => {
                    let field = match form.current_field {
                        0 => &mut form.pattern,
                        1 => &mut form.hostname,
                        2 => &mut form.user,
                        3 => &mut form.port,
                        _ => return Ok(LoopControl::Continue),
                    };
                    field.push(ch);
                }
                _ => {}
            }
        }
        BackspaceFilter => {
            match &mut state.mode {
                Mode::Filter => {
                    state.filter_text.pop();
                    state.apply_filter();
                }
                Mode::EditForm(form) => {
                    let field = match form.current_field {
                        0 => &mut form.pattern,
                        1 => &mut form.hostname,
                        2 => &mut form.user,
                        3 => &mut form.port,
                        _ => return Ok(LoopControl::Continue),
                    };
                    field.pop();
                }
                _ => {}
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
                state.mode = Mode::EditForm(FormData {
                    is_editing: true,
                    pattern: entry.pattern,
                    hostname: entry.hostname.unwrap_or_default(),
                    user: entry.user.unwrap_or_default(),
                    port: entry.port.map(|p| p.to_string()).unwrap_or_default(),
                    current_field: 0,
                });
                state.needs_full_redraw = true;
            }
        }
        NewHost => {
            state.mode = Mode::EditForm(FormData {
                is_editing: false,
                pattern: String::new(),
                hostname: String::new(),
                user: String::new(),
                port: String::new(),
                current_field: 0,
            });
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
        FormNextField => {
            if let Mode::EditForm(form) = &mut state.mode {
                form.current_field = (form.current_field + 1) % 4;
            }
        }
        FormPrevField => {
            if let Mode::EditForm(form) = &mut state.mode {
                form.current_field = if form.current_field == 0 { 3 } else { form.current_field - 1 };
            }
        }
        FormSubmit => {
            if let Mode::EditForm(form) = &state.mode {
                let port_num = if form.port.trim().is_empty() { 
                    None 
                } else { 
                    match form.port.trim().parse::<u16>() {
                        Ok(p) if p > 0 => Some(p),
                        _ => return Err(anyhow::anyhow!("Invalid port number")),
                    }
                };
                
                let entry = SshHostEntry {
                    pattern: form.pattern.trim().to_string(),
                    hostname: if form.hostname.trim().is_empty() { None } else { Some(form.hostname.trim().to_string()) },
                    user: if form.user.trim().is_empty() { None } else { Some(form.user.trim().to_string()) },
                    port: port_num,
                    other: vec![],
                };
                
                // Validate entry before saving
                entry.validate()?;
                
                ssh_cfg.upsert_host(&entry)?;
                state.hosts = ssh_cfg.list_hosts();
                state.apply_filter();
                state.mode = Mode::Normal;
                state.needs_full_redraw = true;
            }
        }
        FormCancel => {
            if matches!(state.mode, Mode::EditForm(_)) {
                state.mode = Mode::Normal;
                state.needs_full_redraw = true;
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
    pub use crate::ui::read_event;
}


