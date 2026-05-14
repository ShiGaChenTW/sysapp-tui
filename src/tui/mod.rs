use crate::model::AppEntry;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Terminal,
};
use std::io;

const PAGE_SIZE: usize = 10;
const COL_HEADERS: &[&str] = &["Name", "Source", "Lang", "Version", "Installed", "Usage", "Path"];

enum Mode {
    Normal,
    Search,
    Detail,
}

pub struct TuiApp {
    entries: Vec<AppEntry>,
    filtered: Vec<usize>,
    table_state: TableState,
    mode: Mode,
    search_query: String,
    sort_col: usize,
    sort_asc: bool,
}

impl TuiApp {
    fn new(entries: Vec<AppEntry>) -> Self {
        let count = entries.len();
        let mut app = Self {
            entries,
            filtered: Vec::with_capacity(count),
            table_state: TableState::default(),
            mode: Mode::Normal,
            search_query: String::new(),
            sort_col: 0, // Name
            sort_asc: true,
        };
        app.filtered = (0..count).collect();
        app.apply_sort();
        app.table_state.select(if count > 0 { Some(0) } else { None });
        app
    }

    fn selected_entry(&self) -> Option<&AppEntry> {
        let sel = self.table_state.selected()?;
        self.filtered.get(sel).map(|&i| &self.entries[i])
    }

    fn move_selection(&mut self, delta: i32) {
        let max = self.filtered.len().saturating_sub(1) as i32;
        if max < 0 {
            return;
        }
        let cur = self.table_state.selected().unwrap_or(0) as i32;
        let new = (cur + delta).clamp(0, max);
        self.table_state.select(Some(new as usize));
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| q.is_empty() || e.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        self.apply_sort();
        self.table_state
            .select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    fn apply_sort(&mut self) {
        if self.filtered.len() < 2 {
            return;
        }
        self.filtered.sort_by(|&a, &b| {
            let ea = &self.entries[a];
            let eb = &self.entries[b];
            let ord = match self.sort_col {
                0 => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),
                1 => ea.source.to_string().cmp(&eb.source.to_string()),
                2 => {
                    let la = ea.language.as_ref().map(|l| l.to_string()).unwrap_or_default();
                    let lb = eb.language.as_ref().map(|l| l.to_string()).unwrap_or_default();
                    la.cmp(&lb)
                }
                3 => {
                    let va = ea.version.as_deref().unwrap_or("");
                    let vb = eb.version.as_deref().unwrap_or("");
                    va.cmp(vb)
                }
                4 => ea.install_date.cmp(&eb.install_date),
                5 => eb.usage_count.cmp(&ea.usage_count), // highest first
                6 => {
                    let pa = ea.path.as_deref().unwrap_or("");
                    let pb = eb.path.as_deref().unwrap_or("");
                    pa.cmp(pb)
                }
                _ => std::cmp::Ordering::Equal,
            };
            if self.sort_asc { ord } else { ord.reverse() }
        });
    }

    fn handle_normal(&mut self, key: KeyEvent) -> io::Result<bool> {
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            return Ok(true);
        }
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('g') | KeyCode::Home => {
                self.table_state.select(Some(0));
            }
            KeyCode::Char('G') | KeyCode::End => {
                let max = self.filtered.len().saturating_sub(1);
                self.table_state.select(Some(max));
            }
            KeyCode::PageDown => self.move_selection(PAGE_SIZE as i32),
            KeyCode::PageUp => self.move_selection(-(PAGE_SIZE as i32)),
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search_query.clear();
            }
            KeyCode::Enter | KeyCode::Char('i') if self.table_state.selected().is_some() => {
                self.mode = Mode::Detail;
            }
            KeyCode::Char(ch) if ch.is_ascii_digit() && ch != '0' => {
                let idx = (ch as u8 - b'1') as usize;
                if idx < COL_HEADERS.len() {
                    if self.sort_col == idx {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_col = idx;
                        self.sort_asc = true;
                    }
                    self.apply_sort();
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search_query.clear();
                self.apply_filter();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.apply_filter();
            }
            KeyCode::Char(ch) if !ch.is_ascii_control() => {
                self.search_query.push(ch);
                self.apply_filter();
            }
            _ => {}
        }
    }

    fn handle_detail(&mut self, key: KeyEvent) -> io::Result<bool> {
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            return Ok(true);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('i') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(false)
    }
}

fn render_header(frame: &mut Frame, area: Rect, count: usize, total: usize) {
    let title = format!(" sysapp-tui — {} app{} found", count, if count == 1 { "" } else { "s" });
    let line = if count < total {
        format!(
            " (filtered from {})   |  1-6:sort  ↑↓:nav  /:search  i:detail  q:quit",
            total
        )
    } else {
        "   |  1-6:sort  ↑↓:nav  /:search  i:detail  q:quit".to_string()
    };
    let text = Line::from(vec![
        Span::styled(title, Style::default().bold().light_cyan()),
        Span::raw(line),
    ]);
    frame.render_widget(Paragraph::new(text).style(Style::default().bg(Color::Black)), area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let (text, style) = match app.mode {
        Mode::Search => {
            let t = format!(" Search: {}█  Esc:cancel  Enter:done", app.search_query);
            (t, Style::default().fg(Color::Black).bg(Color::White))
        }
        _ => {
            let t = if app.filtered.is_empty() {
                " No matches — press / to search, Esc to clear".to_string()
            } else {
                let sel = app.table_state.selected().unwrap_or(0) + 1;
                let total = app.filtered.len();
                let col = COL_HEADERS[app.sort_col];
                let dir = if app.sort_asc { "▲" } else { "▼" };
                format!(
                    " {}/{}  |  sorted by {col} {dir}",
                    sel, total
                )
            };
            (t, Style::default().fg(Color::Black).bg(Color::LightBlue))
        }
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn render_table(app: &mut TuiApp, frame: &mut Frame, area: Rect) {
    if app.filtered.is_empty() {
        frame.render_widget(
            Paragraph::new("  No entries found.").style(Style::default().fg(Color::Gray)),
            area,
        );
        return;
    }

    let _selected = app.table_state.selected();

    let widths = [
        Constraint::Ratio(3, 12), // Name
        Constraint::Ratio(1, 12), // Source
        Constraint::Ratio(1, 12), // Lang
        Constraint::Ratio(1, 12), // Version
        Constraint::Ratio(2, 12), // Installed
        Constraint::Ratio(2, 12), // Usage
        Constraint::Ratio(2, 12), // Path
    ];

    let header_cells: Vec<Cell> = COL_HEADERS
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
            if i == app.sort_col {
                let arrow = if app.sort_asc { " ▲" } else { " ▼" };
                Cell::from(format!("{}{}", name, arrow)).style(style)
            } else {
                Cell::from(*name).style(style)
            }
        })
        .collect();

    let header = Row::new(header_cells).style(Style::default().bg(Color::Black));

    let rows: Vec<Row> = app
        .filtered
        .iter()
        .map(|&idx| {
            let e = &app.entries[idx];
            let version = e.version.as_deref().unwrap_or("—");
            let lang = e
                .language
                .as_ref()
                .map(|l| l.to_string())
                .unwrap_or_else(|| "—".to_string());
            let install = e
                .install_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "—".to_string());
            let usage = if e.usage_count > 0 {
                format!("{}x", e.usage_count)
            } else if let Some(d) = e.last_used {
                d.format("%Y-%m-%d").to_string()
            } else {
                "—".to_string()
            };
            let path = e.path.as_deref().unwrap_or("—");

            Row::new(vec![
                truncate_cell(&e.name, 28),
                Cell::from(e.source.to_string()),
                Cell::from(lang),
                Cell::from(version),
                Cell::from(install),
                Cell::from(usage),
                truncate_cell(path, 24),
            ])
        })
        .collect();

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_detail(frame: &mut Frame, area: Rect, entry: &AppEntry) {
    let lines = vec![
        Line::from(vec![
            Span::styled("Name       ", Style::default().bold()),
            Span::raw(&entry.name),
        ]),
        Line::from(vec![
            Span::styled("Source     ", Style::default().bold()),
            Span::raw(entry.source.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Language   ", Style::default().bold()),
            Span::raw(
                entry
                    .language
                    .as_ref()
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "—".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Version    ", Style::default().bold()),
            Span::raw(entry.version.as_deref().unwrap_or("—")),
        ]),
        Line::from(vec![
            Span::styled("Installed  ", Style::default().bold()),
            Span::raw(
                entry
                    .install_date
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "—".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Last Used  ", Style::default().bold()),
            Span::raw(
                entry
                    .last_used
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "—".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Usage      ", Style::default().bold()),
            Span::raw(format!("{}x", entry.usage_count)),
        ]),
        Line::from(vec![
            Span::styled("Path       ", Style::default().bold()),
            Span::raw(entry.path.as_deref().unwrap_or("—")),
        ]),
    ];

    let content: Vec<Line> = lines
        .into_iter()
        .chain(std::iter::once(Line::from("")))
        .chain(std::iter::once(
            Line::from(Span::styled("  Esc/i: back  q: quit", Style::default().dim())),
        ))
        .collect();

    let overlay_height = content.len() as u16 + 2; // +2 for top/bottom border
    if overlay_height > area.height {
        return; // terminal too small for detail overlay
    }

    let overlay_width = area.width.min(64);
    let x = (area.width - overlay_width) / 2;
    let y = (area.height - overlay_height) / 2;

    let overlay = Rect { x, y, width: overlay_width, height: overlay_height };

    let block = Block::default()
        .title(" Details ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let para = Paragraph::new(Text::from(content)).block(block);
    frame.render_widget(para, overlay);
}

fn truncate_cell(text: &str, max: usize) -> Cell<'_> {
    if text.len() > max {
        let truncated: String = text.chars().take(max.saturating_sub(1)).collect();
        Cell::from(format!("{}…", truncated))
    } else {
        Cell::from(text)
    }
}

pub fn run(entries: Vec<AppEntry>) -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = TuiApp::new(entries);

    // Panic hook to restore terminal
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        prev_hook(info);
    }));

    let result = loop {
        terminal.draw(|f| render(&mut app, f))?;

        let event = event::read()?;
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let quit = match app.mode {
                Mode::Normal => app.handle_normal(key)?,
                Mode::Search => {
                    app.handle_search(key);
                    false
                }
                Mode::Detail => app.handle_detail(key)?,
            };
            if quit {
                break Ok(());
            }
        }
    };

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen)?;

    // Restore original panic hook
    let _ = std::panic::take_hook();

    result
}

fn render(app: &mut TuiApp, frame: &mut Frame) {
    let [header_area, main_area, status_area] = *Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area())
    else {
        return;
    };

    render_header(frame, header_area, app.filtered.len(), app.entries.len());

    if matches!(app.mode, Mode::Detail) {
        if let Some(entry) = app.selected_entry() {
            render_detail(frame, main_area, entry);
        }
    } else {
        render_table(app, frame, main_area);
    }

    render_status_bar(frame, status_area, app);
}
