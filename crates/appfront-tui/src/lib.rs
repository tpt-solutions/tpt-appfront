//! Terminal/TUI backend for AppFront — proves the "one `UITree`, N renderers"
//! thesis one step further: the same `UITree<Msg>` that drives a web app, a
//! native window, or an AI agent also drives a terminal app.
//!
//! The backend is intentionally small. Rendering maps each [`NodeKind`] to a
//! `ratatui` widget (see [`render`]); keyboard input is translated into the
//! app's own `Msg` through the same dispatch closure pattern used by
//! `appfront-dom`/`appfront-canvas` (see [`run`]/[`TuiDriver::on_key`]).
//!
//! All rendering logic is pure and headless-testable via `ratatui`'s
//! `TestBackend`; only [`run`] touches a real terminal.

use std::collections::HashMap;
use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table};
use ratatui::{Frame, Terminal};

use appfront_core::{NodeKind, UITree};

/// A focusable element discovered while walking the tree, in document order.
/// `appfront-dom`/`appfront-canvas` wire these to pointer events; here they're
/// wired to keyboard navigation (Tab/Enter/Space/arrows).
#[derive(Debug, Clone)]
pub struct InteractiveNode<Msg> {
    /// The node's `data_appfront_id` (assigned by [`UITree::assign_ids`]).
    pub id: u64,
    pub kind: InteractiveKind,
    /// The `Msg` to dispatch when this node is "activated" (Enter/Space).
    pub on_click: Option<Msg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveKind {
    Button,
    Input,
}

/// Walks the tree (after IDs are assigned) and collects every focusable node.
fn collect_interactive<Msg: Clone>(ui: &UITree<Msg>) -> Vec<InteractiveNode<Msg>> {
    let mut out = Vec::new();
    fn walk<Msg: Clone>(ui: &UITree<Msg>, out: &mut Vec<InteractiveNode<Msg>>) {
        let id = ui.meta.data_appfront_id.unwrap_or(0);
        match &ui.kind {
            NodeKind::Button { .. } => out.push(InteractiveNode {
                id,
                kind: InteractiveKind::Button,
                on_click: ui.meta.on_click.clone(),
            }),
            NodeKind::Input { .. } => out.push(InteractiveNode {
                id,
                kind: InteractiveKind::Input,
                on_click: ui.meta.on_click.clone(),
            }),
            NodeKind::Container { children } => {
                for child in children {
                    walk(child, out);
                }
            }
            NodeKind::List { items } => {
                for item in items {
                    walk(item, out);
                }
            }
            NodeKind::Heading { .. } | NodeKind::Text { .. } | NodeKind::DataGrid { .. } => {}
        }
    }
    walk(ui, &mut out);
    out
}

/// Best-effort single-line text for a node (used by `List` item rendering).
fn node_text<Msg>(ui: &UITree<Msg>) -> String {
    match &ui.kind {
        NodeKind::Text { text } => text.clone(),
        NodeKind::Heading { text, .. } => text.clone(),
        NodeKind::Button { label } => label.clone(),
        NodeKind::Input { value } => value.clone(),
        _ => String::new(),
    }
}

/// Renders a `UITree` into the given `ratatui` frame area.
///
/// `inputs` lets the caller override an `Input` node's displayed value with
/// the live text being edited (keyed by `data_appfront_id`); `focus_id`
/// highlights the currently focused interactive node.
pub fn render<Msg: Clone>(
    ui: &UITree<Msg>,
    frame: &mut Frame,
    area: Rect,
    inputs: &HashMap<u64, String>,
    focus_id: Option<u64>,
) {
    render_node(ui, frame, area, inputs, focus_id);
}

fn render_node<Msg: Clone>(
    ui: &UITree<Msg>,
    frame: &mut Frame,
    area: Rect,
    inputs: &HashMap<u64, String>,
    focus_id: Option<u64>,
) {
    let id = ui.meta.data_appfront_id;
    let focused = id == focus_id;
    match &ui.kind {
        NodeKind::Container { children } => {
            if children.is_empty() {
                return;
            }
            let constraints: Vec<Constraint> = (0..children.len()).map(|_| Constraint::Min(1)).collect();
            let chunks = Layout::vertical(constraints).split(area);
            for (child, chunk) in children.iter().zip(chunks.iter()) {
                render_node(child, frame, *chunk, inputs, focus_id);
            }
        }
        NodeKind::Heading { text, .. } => {
            frame.render_widget(Paragraph::new(text.clone()).bold(), area);
        }
        NodeKind::Text { text } => {
            frame.render_widget(Paragraph::new(text.clone()), area);
        }
        NodeKind::Button { label } => {
            let style = if focused {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default()
            };
            frame.render_widget(Paragraph::new(format!("[ {label} ]")).style(style), area);
        }
        NodeKind::Input { value } => {
            let v = inputs
                .get(&id.unwrap_or(0))
                .cloned()
                .unwrap_or_else(|| value.clone());
            let style = if focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            frame.render_widget(Paragraph::new(format!("> {v}")).style(style), area);
        }
        NodeKind::List { items } => {
            let rows: Vec<ListItem> = items
                .iter()
                .map(|it| ListItem::new(node_text(it)))
                .collect();
            frame.render_widget(
                List::new(rows).block(Block::default().borders(Borders::ALL).title("list")),
                area,
            );
        }
        NodeKind::DataGrid { columns, rows } => {
            let header: Row = Row::new(columns.iter().map(|c| Cell::from(c.clone())));
            let body: Vec<Row> = rows
                .iter()
                .map(|r| Row::new(r.iter().map(|c| Cell::from(c.clone()))))
                .collect();
            let widths: Vec<Constraint> = if columns.is_empty() {
                vec![Constraint::Percentage(100)]
            } else {
                vec![Constraint::Percentage(100 / columns.len() as u16); columns.len()]
            };
            frame.render_widget(
                Table::new(body, widths)
                    .header(header)
                    .block(Block::default().borders(Borders::ALL)),
                area,
            );
        }
    }
}

/// Keyboard/state driver for a `UITree`. Holds the set of focusable nodes, the
/// current focus, and the live text of any `Input` nodes. The terminal loop
/// feeds it [`KeyEvent`]s via [`TuiDriver::on_key`] and re-renders; the pure
/// parts are unit-testable without a TTY.
pub struct TuiDriver<Msg: Clone> {
    interactive: Vec<InteractiveNode<Msg>>,
    focus: usize,
    inputs: HashMap<u64, String>,
    quit: bool,
}

impl<Msg: Clone> TuiDriver<Msg> {
    /// Builds the driver from a tree snapshot. Assigns IDs first so focus
    /// lookups have stable identities.
    pub fn new(ui: &UITree<Msg>) -> Self {
        let mut ui = ui.clone();
        ui.assign_ids();
        let interactive = collect_interactive(&ui);
        TuiDriver {
            interactive,
            focus: 0,
            inputs: HashMap::new(),
            quit: false,
        }
    }

    /// The number of focusable nodes (used to wrap focus navigation).
    pub fn len(&self) -> usize {
        self.interactive.len()
    }

    pub fn is_empty(&self) -> bool {
        self.interactive.is_empty()
    }

    /// ID of the currently focused node, if any.
    pub fn focus_id(&self) -> Option<u64> {
        self.interactive.get(self.focus).map(|n| n.id)
    }

    /// Live input text, keyed by `data_appfront_id` (for rendering overrides).
    pub fn inputs(&self) -> &HashMap<u64, String> {
        &self.inputs
    }

    /// Whether the driver has been asked to quit.
    pub fn quit(&self) -> bool {
        self.quit
    }

    /// Feeds a key event to the driver. Returns a `Msg` to dispatch (for an
    /// activated button) when one is produced; otherwise `None`. Mutates the
    /// focused `Input` buffer for character/backspace keys.
    pub fn on_key(&mut self, key: KeyEvent) -> Option<Msg> {
        if key.kind != KeyEventKind::Press {
            return None;
        }
        if self.interactive.is_empty() {
            // No interactive nodes: only Esc quits.
            if key.code == KeyCode::Esc {
                self.quit = true;
            }
            return None;
        }
        match key.code {
            KeyCode::Esc => {
                self.quit = true;
                None
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Right => {
                self.focus = (self.focus + 1) % self.interactive.len();
                None
            }
            KeyCode::BackTab | KeyCode::Up | KeyCode::Left => {
                self.focus = self
                    .focus
                    .checked_sub(1)
                    .unwrap_or(self.interactive.len() - 1)
                    % self.interactive.len();
                None
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.interactive[self.focus].on_click.clone()
            }
            KeyCode::Char(c) => {
                let node = &self.interactive[self.focus];
                if node.kind == InteractiveKind::Input {
                    self.inputs.entry(node.id).or_default().push(c);
                }
                None
            }
            KeyCode::Backspace => {
                let node = &self.interactive[self.focus];
                if node.kind == InteractiveKind::Input {
                    if let Some(buf) = self.inputs.get_mut(&node.id) {
                        buf.pop();
                    }
                }
                None
            }
            _ => None,
        }
    }
}

/// Runs the full terminal event loop: alternate screen, raw mode, polling for
/// key events, dispatching `Msg`s via `on_event`. `build_ui` is called on every
/// frame so the rendered values reflect current app state (the driver's focus
/// set is captured once at startup — sufficient for static layouts like the
/// counter example; dynamic add/remove of interactive nodes is a future
/// enhancement).
pub fn run<Msg: Clone + 'static>(
    build_ui: impl Fn() -> UITree<Msg>,
    on_event: impl Fn(Msg),
) -> Result<()> {
    enable_raw_mode().context("enable_raw_mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    let mut driver = TuiDriver::new(&build_ui());

    loop {
        terminal
            .draw(|frame| {
                let ui = build_ui();
                render(&ui, frame, frame.area(), driver.inputs(), driver.focus_id());
            })
            .context("draw")?;

        if event::poll(Duration::from_millis(100)).context("poll")? {
            if let Event::Key(key) = event::read().context("read event")? {
                if let Some(msg) = driver.on_key(key) {
                    on_event(msg);
                }
                if driver.quit() {
                    break;
                }
            }
        }
    }

    disable_raw_mode().context("disable_raw_mode")?;
    execute!(io::stdout(), LeaveAlternateScreen).context("leave alternate screen")?;
    Ok(())
}

/// Flattens a `ratatui` buffer into a single string (one line per row) for
/// snapshot assertions in tests.
pub fn buffer_to_string(buf: &Buffer) -> String {
    let area = buf.area;
    let mut s = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

/// Renders `ui` to an off-screen `TestBackend` of `width` x `height` and
/// returns the resulting buffer. Used by the crate's own headless tests and
/// handy for app-level snapshot tests.
pub fn render_to_buffer<Msg: Clone>(
    ui: &UITree<Msg>,
    width: u16,
    height: u16,
) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend");
    let driver = TuiDriver::new(ui);
    terminal
        .draw(|frame| render(ui, frame, frame.area(), driver.inputs(), driver.focus_id()))
        .expect("draw");
    terminal.backend().buffer().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use appfront_core::ContainerBuilder;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Increment,
    }

    fn sample_ui() -> UITree<Msg> {
        UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Counter TUI");
            c.text("Press + to count");
            c.button("+1")
                .on_click(Msg::Increment)
                .key("inc");
            c.input("type here").key("name");
        })
    }

    #[test]
    fn collects_interactive_in_document_order() {
        let ui = sample_ui();
        let driver = TuiDriver::new(&ui);
        assert_eq!(driver.len(), 2);
        assert_eq!(driver.interactive[0].kind, InteractiveKind::Button);
        assert_eq!(driver.interactive[1].kind, InteractiveKind::Input);
        // Focus starts on the button.
        assert_eq!(driver.focus_id(), Some(driver.interactive[0].id));
    }

    #[test]
    fn enter_on_button_dispatches_msg() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let key = KeyEvent::new(KeyCode::Enter, ratatui::crossterm::event::KeyModifiers::NONE);
        assert_eq!(driver.on_key(key), Some(Msg::Increment));
        assert!(!driver.quit());
    }

    #[test]
    fn space_activates_button_too() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let key = KeyEvent::new(KeyCode::Char(' '), ratatui::crossterm::event::KeyModifiers::NONE);
        assert_eq!(driver.on_key(key), Some(Msg::Increment));
    }

    #[test]
    fn tab_moves_focus_to_input_and_typing_edits_it() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let tab = KeyEvent::new(KeyCode::Tab, ratatui::crossterm::event::KeyModifiers::NONE);
        driver.on_key(tab);
        assert_eq!(driver.focus_id(), Some(driver.interactive[1].id));

        let a = KeyEvent::new(KeyCode::Char('a'), ratatui::crossterm::event::KeyModifiers::NONE);
        let b = KeyEvent::new(KeyCode::Char('b'), ratatui::crossterm::event::KeyModifiers::NONE);
        driver.on_key(a);
        driver.on_key(b);
        let id = driver.interactive[1].id;
        assert_eq!(driver.inputs().get(&id).map(String::as_str), Some("ab"));

        let bs =
            KeyEvent::new(KeyCode::Backspace, ratatui::crossterm::event::KeyModifiers::NONE);
        driver.on_key(bs);
        assert_eq!(driver.inputs().get(&id).map(String::as_str), Some("a"));
    }

    #[test]
    fn typing_on_button_is_ignored() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let c = KeyEvent::new(KeyCode::Char('x'), ratatui::crossterm::event::KeyModifiers::NONE);
        assert_eq!(driver.on_key(c), None);
        assert!(driver.inputs().values().all(|v| v.is_empty()));
    }

    #[test]
    fn esc_requests_quit() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let esc = KeyEvent::new(KeyCode::Esc, ratatui::crossterm::event::KeyModifiers::NONE);
        driver.on_key(esc);
        assert!(driver.quit());
    }

    #[test]
    fn render_contains_heading_and_button_label() {
        let ui = sample_ui();
        let buf = render_to_buffer(&ui, 40, 10);
        let s = buffer_to_string(&buf);
        assert!(s.contains("Counter TUI"), "heading should render: {s}");
        assert!(s.contains("+1"), "button label should render: {s}");
    }

    #[test]
    fn render_highlights_focused_button() {
        let ui = sample_ui();
        let buf = render_to_buffer(&ui, 40, 10);
        let s = buffer_to_string(&buf);
        // Focused button renders inside brackets; the bracketed form only
        // appears for the focused node.
        assert!(s.contains("[ +1 ]"), "focused button should be bracketed: {s}");
    }
}
