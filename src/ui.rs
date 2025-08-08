use crate::app::{AppState, ConfirmContext, Mode};
use crate::ssh_config::SshHostEntry;
use anyhow::Result;
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use std::time::Duration;

#[derive(Debug)]
pub enum Event {
    Action(UiAction),
    Tick,
}

#[derive(Debug, Copy, Clone)]
pub enum UiAction {
    MoveUp,
    MoveDown,
    PageUp,
    PageDown,
    BeginFilter,
    InputChar(char),
    BackspaceFilter,
    ClearFilter,
    EditSelected,
    NewHost,
    DeleteSelected,
    LaunchSelected,
    Quit,
    Noop,
}

pub fn draw_ui(f: &mut Frame<'_>, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled("ssh-picker", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  [j/k] move  [Enter] ssh  [/] filter  [e] edit  [a] add  [d] delete  [q] quit"),
    ]));
    f.render_widget(header, chunks[0]);

    // List of hosts
    let items: Vec<ListItem> = state
        .filtered_hosts
        .iter()
        .map(|&idx| host_to_item(&state.hosts[idx]))
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Hosts"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .highlight_symbol("› ");
    let mut ls = build_list_state(state);
    f.render_stateful_widget(list, chunks[1], &mut ls);

    // Footer / filter
    let filter = match state.mode {
        Mode::Filter => format!("/{}", state.filter_text),
        _ => String::new(),
    };
    let footer = Paragraph::new(filter)
        .block(Block::default().borders(Borders::ALL).title("Filter"))
        .wrap(Wrap { trim: true });
    f.render_widget(footer, chunks[2]);

    // Modal overlay(s)
    if let Mode::Confirm(ctx) = &state.mode {
        let area = centered_rect(60, 30, f.area());
        let block = Block::default().borders(Borders::ALL).title("Confirm");
        let message = match ctx {
            ConfirmContext::Delete { pattern } => format!("Delete host '{}' ?", pattern),
        };
        let text = vec![
            Line::from(Span::raw(message)),
            Span::raw("").into(),
            Line::from(Span::styled(
                "y: Yes    n/Esc: No",
                Style::default().fg(Color::Yellow),
            )),
        ];
        let para = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
        f.render_widget(Clear, area); // clear background
        f.render_widget(para, area);
    }
}

fn host_to_item(entry: &SshHostEntry) -> ListItem<'_> {
    let line = Line::from(vec![
        Span::styled(&entry.pattern, Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled(
            entry.hostname.as_deref().unwrap_or(""),
            Style::default().fg(Color::Gray),
        ),
        Span::raw("  "),
        Span::styled(
            entry.user.as_deref().unwrap_or(""),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    ListItem::new(line)
}

fn build_list_state(state: &AppState) -> ratatui::widgets::ListState {
    let mut ls = ratatui::widgets::ListState::default();
    if !state.filtered_hosts.is_empty() {
        ls.select(Some(state.selected_index));
    }
    ls
}

pub fn read_event() -> Result<Event> {
    if event::poll(Duration::from_millis(200))? {
        if let CEvent::Key(key) = event::read()? {
            return Ok(Event::Action(map_key(key)));
        }
    }
    Ok(Event::Tick)
}

fn map_key(key: KeyEvent) -> UiAction {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => UiAction::Quit,
        (KeyCode::Enter, _) => UiAction::LaunchSelected,
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => UiAction::MoveDown,
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => UiAction::MoveUp,
        (KeyCode::PageDown, _) | (KeyCode::Char('f'), KeyModifiers::CONTROL) => UiAction::PageDown,
        (KeyCode::PageUp, _) | (KeyCode::Char('b'), KeyModifiers::CONTROL) => UiAction::PageUp,
        (KeyCode::Char('/'), _) => UiAction::BeginFilter,
        (KeyCode::Esc, _) => UiAction::ClearFilter,
        (KeyCode::Backspace, _) => UiAction::BackspaceFilter,
        (KeyCode::Char('e'), _) => UiAction::EditSelected,
        (KeyCode::Char('a'), _) => UiAction::NewHost,
        (KeyCode::Char('d'), _) => UiAction::DeleteSelected,
        (KeyCode::Char(c), _) => UiAction::InputChar(c),
        _ => UiAction::Noop,
    }
}

// Simple pop-up dialogs in the alternate screen are implemented here as blocking in-line forms.
// For a first pass, we collect fields in stdin/outside of raw mode.
pub fn edit_host_dialog(existing: &SshHostEntry) -> Result<SshHostEntry> {
    form_dialog(Some(existing))
}

pub fn new_host_dialog() -> Result<SshHostEntry> {
    form_dialog(None)
}

// legacy non-TUI confirm removed; confirm handled via modal overlay and key events

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let ver = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let hor = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(ver[1]);
    hor[1]
}

fn form_dialog(existing: Option<&SshHostEntry>) -> Result<SshHostEntry> {
    let mut pattern = existing.map(|e| e.pattern.clone()).unwrap_or_default();
    let mut hostname = existing.and_then(|e| e.hostname.clone()).unwrap_or_default();
    let mut user = existing.and_then(|e| e.user.clone()).unwrap_or_default();
    let mut port = existing.and_then(|e| e.port.map(|p| p.to_string())).unwrap_or_default();

    // Drop raw mode for input
    crossterm::terminal::disable_raw_mode()?;
    eprintln!("\nEnter host fields. Leave blank to keep current value.");
    read_field("Host (pattern)", &mut pattern)?;
    read_field("HostName", &mut hostname)?;
    read_field("User", &mut user)?;
    read_field("Port", &mut port)?;
    crossterm::terminal::enable_raw_mode()?;

    let port_num = if port.trim().is_empty() { None } else { port.trim().parse().ok() };
    Ok(SshHostEntry { pattern, hostname: nonempty(hostname), user: nonempty(user), port: port_num, other: vec![] })
}

fn read_field(label: &str, buf: &mut String) -> Result<()> {
    eprint!("{} [{}]: ", label, buf.trim());
    std::io::Write::flush(&mut std::io::stderr())?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let v = line.trim().to_string();
    if !v.is_empty() {
        *buf = v;
    }
    Ok(())
}

fn nonempty(s: String) -> Option<String> {
    if s.trim().is_empty() { None } else { Some(s) }
}


