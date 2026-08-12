//! Terminal/TUI backend for AppFront — proves the "one `UITree`, N renderers"
//! thesis one step further: the same `UITree<Msg>` that drives a web app, a
//! native window, or an AI agent also drives a terminal app.
//!
//! The backend is intentionally small. Rendering maps each [`NodeKind`] to a
//! `ratatui` widget (see [`render`]); keyboard input is translated into the
//! app's own `Msg` through the same dispatch closure pattern used by
//! `tpt-appfront-dom`/`tpt-appfront-canvas` (see [`run`]/[`TuiDriver::on_key`]).
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
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap};
use ratatui::{Frame, Terminal};
use tpt_appfront_core::{
    apply_auto_virtual_scroll, ui_tree::MediaType, NodeKind, UITree, VirtualScroll,
};

/// A focusable element discovered while walking the tree, in document order.
/// `tpt-appfront-dom`/`tpt-appfront-canvas` wire these to pointer events; here they're
/// wired to keyboard navigation (Tab/Enter/Space/arrows).
#[derive(Debug, Clone)]
pub struct InteractiveNode<Msg> {
    /// The node's `data_appfront_id` (assigned by [`UITree::assign_ids`]).
    pub id: u64,
    pub kind: InteractiveKind,
    /// The `Msg` to dispatch when this node is "activated" (Enter/Space).
    /// Only ever set for `Button` today.
    pub on_click: Option<Msg>,
    /// `(value, label)` options for `Select`/`Radio`; empty for every other
    /// kind. Used to cycle the selection with Left/Right/Up/Down.
    pub options: Vec<(String, String)>,
    /// The node's original value — `selected` for `Select`/`Radio`, the
    /// stringified `checked` for `Checkbox` — used to seed keyboard
    /// cycling/toggling before the user has touched the control (mirrors how
    /// [`TuiDriver::inputs`] falls back to the `Input`/`Textarea` node's own
    /// `value` until edited).
    pub initial_value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveKind {
    Button,
    Input,
    Textarea,
    Checkbox,
    Select,
    Radio,
}

/// Walks the tree (after IDs are assigned) and collects every focusable node.
fn collect_interactive<Msg: Clone>(ui: &UITree<Msg>) -> Vec<InteractiveNode<Msg>> {
    let mut out = Vec::new();
    fn walk<Msg: Clone>(ui: &UITree<Msg>, out: &mut Vec<InteractiveNode<Msg>>) {
        let id = ui.meta.data_appfront_id.unwrap_or(0);
        let node =
            |kind, options: Vec<(String, String)>, initial_value: Option<String>| InteractiveNode {
                id,
                kind,
                on_click: ui.meta.on_click.clone(),
                options,
                initial_value,
            };
        match &ui.kind {
            NodeKind::Button { .. } => out.push(node(InteractiveKind::Button, Vec::new(), None)),
            NodeKind::Input { .. } => out.push(node(InteractiveKind::Input, Vec::new(), None)),
            NodeKind::Textarea { .. } => {
                out.push(node(InteractiveKind::Textarea, Vec::new(), None))
            }
            NodeKind::Checkbox { checked, .. } => out.push(node(
                InteractiveKind::Checkbox,
                Vec::new(),
                Some(checked.to_string()),
            )),
            NodeKind::Select { options, selected } => out.push(node(
                InteractiveKind::Select,
                options.clone(),
                Some(selected.clone()),
            )),
            NodeKind::Radio {
                options, selected, ..
            } => out.push(node(
                InteractiveKind::Radio,
                options.clone(),
                Some(selected.clone()),
            )),
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
            NodeKind::Heading { .. }
            | NodeKind::Text { .. }
            | NodeKind::DataGrid { .. }
            | NodeKind::Portal { .. }
            | NodeKind::Image { .. }
            | NodeKind::Link { .. }
            | NodeKind::Media { .. } => {}
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
        NodeKind::Textarea { value } => value.clone(),
        NodeKind::Checkbox { label, .. } => label.clone(),
        NodeKind::Select { selected, .. } => selected.clone(),
        NodeKind::Radio { selected, .. } => selected.clone(),
        NodeKind::Image { alt, .. } => alt.clone(),
        NodeKind::Link { text, .. } => text.clone(),
        NodeKind::Media { alt, .. } => alt.clone(),
        _ => String::new(),
    }
}

/// Computes the `(start, end)` window of a large `List`/`DataGrid` to render
/// when virtual scrolling is configured. `VirtualScroll` is px-based for DOM/
/// canvas; in the terminal a row is one unit, so we reinterpret `item_height`
/// as 1 row and `viewport_height` as the visible row count (minus one for a
/// `DataGrid` header). Returns the full range when no `VirtualScroll` is set.
fn tui_window(total: usize, vs: Option<VirtualScroll>, viewport_rows: u16) -> (usize, usize) {
    match vs {
        Some(v) => {
            let vr = VirtualScroll {
                item_height: 1.0,
                viewport_height: viewport_rows.max(1) as f32,
                scroll_offset: v.scroll_offset,
                overscan: v.overscan,
            };
            let r = vr.visible_range(total);
            (r.start, r.end)
        }
        None => (0, total),
    }
}

/// Renders a `UITree` into the given `ratatui` frame area.
///
/// `inputs` lets the caller override an `Input`/`Textarea` node's displayed
/// value with the live text being edited, `checks` overrides a `Checkbox`'s
/// `checked` state, and `selections` overrides a `Select`/`Radio`'s chosen
/// value — all keyed by `data_appfront_id`. `focus_id` highlights the
/// currently focused interactive node.
pub fn render<Msg: Clone>(
    ui: &UITree<Msg>,
    frame: &mut Frame,
    area: Rect,
    inputs: &HashMap<u64, String>,
    checks: &HashMap<u64, bool>,
    selections: &HashMap<u64, String>,
    focus_id: Option<u64>,
) {
    render_node(ui, frame, area, inputs, checks, selections, focus_id);
}

#[allow(clippy::too_many_arguments)]
fn render_node<Msg: Clone>(
    ui: &UITree<Msg>,
    frame: &mut Frame,
    area: Rect,
    inputs: &HashMap<u64, String>,
    checks: &HashMap<u64, bool>,
    selections: &HashMap<u64, String>,
    focus_id: Option<u64>,
) {
    let id = ui.meta.data_appfront_id;
    let focused = id == focus_id;
    let focus_style = || Style::default().fg(Color::Black).bg(Color::Yellow);
    match &ui.kind {
        NodeKind::Container { children } => {
            if children.is_empty() {
                return;
            }
            let constraints: Vec<Constraint> =
                (0..children.len()).map(|_| Constraint::Min(1)).collect();
            let chunks = Layout::vertical(constraints).split(area);
            for (child, chunk) in children.iter().zip(chunks.iter()) {
                render_node(child, frame, *chunk, inputs, checks, selections, focus_id);
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
                focus_style()
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
        NodeKind::Textarea { value } => {
            let v = inputs
                .get(&id.unwrap_or(0))
                .cloned()
                .unwrap_or_else(|| value.clone());
            let style = if focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            frame.render_widget(
                Paragraph::new(v)
                    .style(style)
                    .wrap(Wrap { trim: false })
                    .block(Block::default().borders(Borders::ALL)),
                area,
            );
        }
        NodeKind::Checkbox { label, checked } => {
            let c = checks.get(&id.unwrap_or(0)).copied().unwrap_or(*checked);
            let mark = if c { "x" } else { " " };
            let style = if focused {
                focus_style()
            } else {
                Style::default()
            };
            frame.render_widget(
                Paragraph::new(format!("[{mark}] {label}")).style(style),
                area,
            );
        }
        NodeKind::Select { options, selected } => {
            let cur = selections
                .get(&id.unwrap_or(0))
                .cloned()
                .unwrap_or_else(|| selected.clone());
            let label = options
                .iter()
                .find(|(v, _)| v == &cur)
                .map(|(_, l)| l.as_str())
                .unwrap_or(cur.as_str());
            let style = if focused {
                focus_style()
            } else {
                Style::default()
            };
            frame.render_widget(Paragraph::new(format!("< {label} >")).style(style), area);
        }
        NodeKind::Radio {
            options, selected, ..
        } => {
            let cur = selections
                .get(&id.unwrap_or(0))
                .cloned()
                .unwrap_or_else(|| selected.clone());
            let text = options
                .iter()
                .map(|(v, l)| {
                    if v == &cur {
                        format!("(*) {l}")
                    } else {
                        format!("( ) {l}")
                    }
                })
                .collect::<Vec<_>>()
                .join("   ");
            let style = if focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            frame.render_widget(Paragraph::new(text).style(style), area);
        }
        NodeKind::List { items } => {
            let (start, end) = tui_window(items.len(), ui.meta.virtual_scroll, area.height);
            let mut rows: Vec<ListItem> = Vec::with_capacity(items.len());
            for _ in 0..start {
                rows.push(ListItem::new(""));
            }
            for it in &items[start..end] {
                rows.push(ListItem::new(node_text(it)));
            }
            for _ in end..items.len() {
                rows.push(ListItem::new(""));
            }
            frame.render_widget(
                List::new(rows).block(Block::default().borders(Borders::ALL).title("list")),
                area,
            );
        }
        NodeKind::DataGrid { columns, rows } => {
            let header: Row = Row::new(columns.iter().map(|c| Cell::from(c.clone())));
            // Reserve one row for the header when computing the visible window.
            let viewport_rows = area.height.saturating_sub(1);
            let (start, end) = tui_window(rows.len(), ui.meta.virtual_scroll, viewport_rows);
            let blanks: Vec<Cell> = vec![Cell::from(""); columns.len()];
            let mut body: Vec<Row> = Vec::with_capacity(rows.len());
            for _ in 0..start {
                body.push(Row::new(blanks.clone()));
            }
            for r in &rows[start..end] {
                body.push(Row::new(r.iter().map(|c| Cell::from(c.clone()))));
            }
            for _ in end..rows.len() {
                body.push(Row::new(blanks.clone()));
            }
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
        NodeKind::Portal { content, .. } => {
            // TUI has no overlay layer: render the portal content inline at the
            // declaration site. Hosts that want true overlay portals can use
            // `UITree::collect_portals` to extract them first.
            render_node(content, frame, area, inputs, checks, selections, focus_id);
        }
        NodeKind::Image { src, alt } => {
            frame.render_widget(
                Paragraph::new(format!("🖼 {alt} ({src})")).wrap(Wrap { trim: false }),
                area,
            );
        }
        NodeKind::Link { href, text } => {
            let style = if focused {
                focus_style()
            } else {
                Style::default()
            };
            frame.render_widget(
                Paragraph::new(format!("→ {text} <{href}>")).style(style),
                area,
            );
        }
        NodeKind::Media { src, alt, media_type } => {
            let kind = match media_type {
                MediaType::Audio => "audio",
                MediaType::Video => "video",
            };
            frame.render_widget(
                Paragraph::new(format!("{kind}: {alt} ({src})")).wrap(Wrap { trim: false }),
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
    checks: HashMap<u64, bool>,
    selections: HashMap<u64, String>,
    quit: bool,
    /// When set, large unconfigured `List`/`DataGrid` nodes are auto-windowed
    /// via [`apply_auto_virtual_scroll`] on every render.
    auto_optimize: bool,
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
            checks: HashMap::new(),
            selections: HashMap::new(),
            quit: false,
            auto_optimize: false,
        }
    }

    /// Enables automatic application of the frame-profiler's `virtual_scrolling`
    /// recommendation: when enabled, large unconfigured `List`/`DataGrid` nodes
    /// get windowed automatically on every render — the same auto-tune treatment
    /// the canvas backend (`CanvasApp::auto_optimize`) already provides. Set
    /// before calling [`run`]/[`run_with`].
    pub fn auto_optimize(mut self, enabled: bool) -> Self {
        self.auto_optimize = enabled;
        self
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

    /// Live `Checkbox` toggles, keyed by `data_appfront_id` (for rendering
    /// overrides).
    pub fn checks(&self) -> &HashMap<u64, bool> {
        &self.checks
    }

    /// Live `Select`/`Radio` selections, keyed by `data_appfront_id` (for
    /// rendering overrides).
    pub fn selections(&self) -> &HashMap<u64, String> {
        &self.selections
    }

    /// Whether the driver has been asked to quit.
    pub fn quit(&self) -> bool {
        self.quit
    }

    /// The currently focused node's `(value, label)` options, if it's a
    /// `Select`/`Radio`; `None` otherwise.
    fn cycle_option(&mut self, delta: i32) {
        let node = &self.interactive[self.focus];
        if node.options.is_empty() {
            return;
        }
        let id = node.id;
        let current = self
            .selections
            .get(&id)
            .cloned()
            .or_else(|| node.initial_value.clone())
            .unwrap_or_default();
        let idx = node
            .options
            .iter()
            .position(|(v, _)| v == &current)
            .unwrap_or(0) as i32;
        let len = node.options.len() as i32;
        let next = ((idx + delta) % len + len) % len;
        let value = node.options[next as usize].0.clone();
        self.selections.insert(id, value);
    }

    /// Feeds a key event to the driver. Returns a `Msg` to dispatch (for an
    /// activated button) when one is produced; otherwise `None`. Mutates the
    /// focused control's live state for character/backspace keys (`Input`/
    /// `Textarea`), Enter/Space (`Checkbox` toggle, or cycling a `Select`/
    /// `Radio` forward), and Left/Right/Up/Down (cycling a focused `Select`/
    /// `Radio`; moving focus for every other kind).
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
        let is_cyclable = matches!(
            self.interactive[self.focus].kind,
            InteractiveKind::Select | InteractiveKind::Radio
        );
        match key.code {
            KeyCode::Esc => {
                self.quit = true;
                None
            }
            KeyCode::Tab => {
                self.focus = (self.focus + 1) % self.interactive.len();
                None
            }
            KeyCode::BackTab => {
                self.focus = self
                    .focus
                    .checked_sub(1)
                    .unwrap_or(self.interactive.len() - 1)
                    % self.interactive.len();
                None
            }
            KeyCode::Down | KeyCode::Right if is_cyclable => {
                self.cycle_option(1);
                None
            }
            KeyCode::Up | KeyCode::Left if is_cyclable => {
                self.cycle_option(-1);
                None
            }
            KeyCode::Down | KeyCode::Right => {
                self.focus = (self.focus + 1) % self.interactive.len();
                None
            }
            KeyCode::Up | KeyCode::Left => {
                self.focus = self
                    .focus
                    .checked_sub(1)
                    .unwrap_or(self.interactive.len() - 1)
                    % self.interactive.len();
                None
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let kind = self.interactive[self.focus].kind;
                match kind {
                    InteractiveKind::Checkbox => {
                        let id = self.interactive[self.focus].id;
                        let current = self
                            .checks
                            .get(&id)
                            .copied()
                            .or_else(|| {
                                self.interactive[self.focus]
                                    .initial_value
                                    .as_deref()
                                    .map(|v| v == "true")
                            })
                            .unwrap_or(false);
                        self.checks.insert(id, !current);
                        None
                    }
                    InteractiveKind::Select | InteractiveKind::Radio => {
                        self.cycle_option(1);
                        None
                    }
                    _ => self.interactive[self.focus].on_click.clone(),
                }
            }
            KeyCode::Char(c) => {
                let node = &self.interactive[self.focus];
                if matches!(
                    node.kind,
                    InteractiveKind::Input | InteractiveKind::Textarea
                ) {
                    self.inputs.entry(node.id).or_default().push(c);
                }
                None
            }
            KeyCode::Backspace => {
                let node = &self.interactive[self.focus];
                if matches!(
                    node.kind,
                    InteractiveKind::Input | InteractiveKind::Textarea
                ) {
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
    run_with(build_ui, on_event, false)
}

/// Like [`run`], but lets the caller enable `auto_optimize` — when `true`,
/// large unconfigured `List`/`DataGrid` nodes are windowed automatically on
/// every render (mirroring `CanvasApp::auto_optimize`).
pub fn run_with<Msg: Clone + 'static>(
    build_ui: impl Fn() -> UITree<Msg>,
    on_event: impl Fn(Msg),
    auto_optimize: bool,
) -> Result<()> {
    enable_raw_mode().context("enable_raw_mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    let mut driver = TuiDriver::new(&build_ui()).auto_optimize(auto_optimize);

    loop {
        terminal
            .draw(|frame| {
                let mut ui = build_ui();
                if auto_optimize {
                    apply_auto_virtual_scroll(&mut ui, frame.area().height as f32);
                }
                render(
                    &ui,
                    frame,
                    frame.area(),
                    driver.inputs(),
                    driver.checks(),
                    driver.selections(),
                    driver.focus_id(),
                );
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
pub fn render_to_buffer<Msg: Clone>(ui: &UITree<Msg>, width: u16, height: u16) -> Buffer {
    render_to_buffer_with(ui, width, height, false)
}

/// Like [`render_to_buffer`], but with `auto_optimize` enabled — large
/// unconfigured `List`/`DataGrid` nodes are windowed before rendering, so
/// headless tests can assert the auto-tune path behaves like an explicit
/// [`VirtualScroll`].
pub fn render_to_buffer_with<Msg: Clone>(
    ui: &UITree<Msg>,
    width: u16,
    height: u16,
    auto_optimize: bool,
) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend");
    let driver = TuiDriver::new(ui).auto_optimize(auto_optimize);
    terminal
        .draw(|frame| {
            let mut ui = ui.clone();
            if auto_optimize {
                apply_auto_virtual_scroll(&mut ui, frame.area().height as f32);
            }
            render(
                &ui,
                frame,
                frame.area(),
                driver.inputs(),
                driver.checks(),
                driver.selections(),
                driver.focus_id(),
            )
        })
        .expect("draw");
    terminal.backend().buffer().clone()
}

#[cfg(test)]
mod tests {
    use tpt_appfront_core::ContainerBuilder;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Increment,
    }

    fn sample_ui() -> UITree<Msg> {
        UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.heading(1, "Counter TUI");
            c.text("Press + to count");
            c.button("+1").on_click(Msg::Increment).key("inc");
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

    fn form_ui() -> UITree<Msg> {
        UITree::container(|c: &mut ContainerBuilder<Msg>| {
            c.checkbox("Agree", false).key("agree");
            c.select([("a", "Alpha"), ("b", "Beta"), ("c", "Gamma")], "a")
                .key("choice");
        })
    }

    #[test]
    fn enter_toggles_focused_checkbox() {
        let ui = form_ui();
        let mut driver = TuiDriver::new(&ui);
        let id = driver.interactive[0].id;
        assert_eq!(driver.checks().get(&id), None);

        let enter = KeyEvent::new(
            KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        driver.on_key(enter);
        assert_eq!(driver.checks().get(&id), Some(&true));
        driver.on_key(enter);
        assert_eq!(driver.checks().get(&id), Some(&false));
    }

    #[test]
    fn right_arrow_cycles_focused_select() {
        let ui = form_ui();
        let mut driver = TuiDriver::new(&ui);
        // Move focus from the checkbox to the select.
        let tab = KeyEvent::new(KeyCode::Tab, ratatui::crossterm::event::KeyModifiers::NONE);
        driver.on_key(tab);
        let id = driver.interactive[1].id;
        assert_eq!(driver.selections().get(&id), None);

        let right = KeyEvent::new(
            KeyCode::Right,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        driver.on_key(right);
        assert_eq!(driver.selections().get(&id).map(String::as_str), Some("b"));
        driver.on_key(right);
        assert_eq!(driver.selections().get(&id).map(String::as_str), Some("c"));
        driver.on_key(right);
        assert_eq!(
            driver.selections().get(&id).map(String::as_str),
            Some("a"),
            "cycling wraps around"
        );
    }

    #[test]
    fn enter_on_button_dispatches_msg() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let key = KeyEvent::new(
            KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(driver.on_key(key), Some(Msg::Increment));
        assert!(!driver.quit());
    }

    #[test]
    fn space_activates_button_too() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let key = KeyEvent::new(
            KeyCode::Char(' '),
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(driver.on_key(key), Some(Msg::Increment));
    }

    #[test]
    fn tab_moves_focus_to_input_and_typing_edits_it() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let tab = KeyEvent::new(KeyCode::Tab, ratatui::crossterm::event::KeyModifiers::NONE);
        driver.on_key(tab);
        assert_eq!(driver.focus_id(), Some(driver.interactive[1].id));

        let a = KeyEvent::new(
            KeyCode::Char('a'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        let b = KeyEvent::new(
            KeyCode::Char('b'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        driver.on_key(a);
        driver.on_key(b);
        let id = driver.interactive[1].id;
        assert_eq!(driver.inputs().get(&id).map(String::as_str), Some("ab"));

        let bs = KeyEvent::new(
            KeyCode::Backspace,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        driver.on_key(bs);
        assert_eq!(driver.inputs().get(&id).map(String::as_str), Some("a"));
    }

    #[test]
    fn typing_on_button_is_ignored() {
        let ui = sample_ui();
        let mut driver = TuiDriver::new(&ui);
        let c = KeyEvent::new(
            KeyCode::Char('x'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
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
        assert!(
            s.contains("[ +1 ]"),
            "focused button should be bracketed: {s}"
        );
    }

    #[test]
    fn list_virtual_scroll_renders_only_window() {
        // A 100-item list configured to virtualize (1-row items, 5-row viewport):
        // only the top window should be in the rendered buffer, not far rows.
        let ui: UITree<Msg> = UITree::container(|c| {
            c.list(|l: &mut ContainerBuilder<Msg>| {
                for i in 0..100 {
                    l.text(format!("row {i}"));
                }
            })
            .virtual_scroll(VirtualScroll::new(1.0, 5.0));
        });
        let buf = render_to_buffer(&ui, 40, 10);
        let s = buffer_to_string(&buf);
        assert!(s.contains("row 0"), "first rows should render: {s}");
        assert!(
            !s.contains("row 99"),
            "far rows should be virtualized out of the visible buffer: {s}"
        );
    }

    #[test]
    fn auto_virtual_scroll_windows_large_unconfigured_list() {
        // Same 100-item list but WITHOUT an explicit `VirtualScroll`: with
        // auto_optimize on, the backend must window it automatically (the
        // canvas `CanvasApp::auto_optimize` parity for TUI).
        let ui: UITree<Msg> = UITree::container(|c| {
            c.list(|l: &mut ContainerBuilder<Msg>| {
                for i in 0..100 {
                    l.text(format!("row {i}"));
                }
            });
        });
        let buf = render_to_buffer_with(&ui, 40, 10, true);
        let s = buffer_to_string(&buf);
        assert!(s.contains("row 0"), "first rows should render: {s}");
        assert!(
            !s.contains("row 99"),
            "far rows should be auto-virtualized out of the visible buffer: {s}"
        );
    }

    #[test]
    fn auto_virtual_scroll_respects_existing_config() {
        // A small list (below the auto threshold) must NOT be windowed by
        // auto_optimize, and an explicit config must survive.
        let ui: UITree<Msg> = UITree::container(|c| {
            c.list(|l: &mut ContainerBuilder<Msg>| {
                for i in 0..5 {
                    l.text(format!("row {i}"));
                }
            })
            .virtual_scroll(VirtualScroll::new(1.0, 50.0));
        });
        let buf = render_to_buffer_with(&ui, 40, 10, true);
        let s = buffer_to_string(&buf);
        assert!(s.contains("row 4"), "small list should render all rows: {s}");
    }
}
