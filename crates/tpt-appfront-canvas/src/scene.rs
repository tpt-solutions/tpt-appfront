//! Paints a `UITree` with the internal painter and handles its input.
//!
//! This replaces the egui widget layer. Each frame the tree is laid out
//! with `taffy` (see `layout.rs`), window input is applied, and the result is
//! painted with [`Canvas`]. The widget behaviour that egui used to provide lives
//! here: pointer capture, click, focus and Tab order, keyboard routing, and
//! text editing.
//!
//! Text fields are controlled. Each edit is sent through `on_input`, and the
//! application's next tree supplies the value. A local draft keeps typing
//! responsive while the application catches up, and is dropped once the tree
//! no longer shows the value the draft was based on.
//!
//! Nodes are identified across frames by their path of child indices, so
//! focus and drafts follow a node only while the tree keeps its shape.

use std::collections::HashMap;

use taffy::TaffyTree;
use tpt_appfront_core::{KeyEvent, NodeKind, PointerEvent, UITree};

use crate::glyphs::Font;
use crate::layout::{self, GridRowKind, RenderNode, CELL_PADDING, TEXT_FONT_SIZE};
use crate::platform::{Event, MouseButton};
use crate::raster::{rgb, Canvas, Rect, Rgb};
use crate::text::TextMeasurer;

const TEXT_COLOR: Rgb = rgb(0x1F, 0x29, 0x37);
const BORDER_COLOR: Rgb = rgb(0x9C, 0xA3, 0xAF);
const FOCUS_COLOR: Rgb = rgb(0x3B, 0x82, 0xF6);
const BUTTON_BG: Rgb = rgb(0xE5, 0xE7, 0xEB);
const FIELD_BG: Rgb = rgb(0xFF, 0xFF, 0xFF);
const CHECK_COLOR: Rgb = rgb(0x3B, 0x82, 0xF6);
const FIELD_PAD: f32 = 8.0;
const CHECKBOX_SIZE: f32 = 14.0;
const CHECKBOX_GAP: f32 = 10.0;

/// Position of a node in the tree: the child index at each level.
pub type Path = Vec<usize>;

/// Absolute position and size of a laid-out node, in framebuffer pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bounds {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Bounds {
    fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    fn to_rect(self) -> Rect {
        Rect::new(
            self.x.round() as i32,
            self.y.round() as i32,
            self.w.round() as i32,
            self.h.round() as i32,
        )
    }
}

/// One laid-out node, in paint order (parents before their children).
struct Hit<'a, Msg> {
    path: Path,
    ui: &'a UITree<Msg>,
    bounds: Bounds,
    focusable: bool,
    interactive: bool,
}

/// Interaction state that persists between frames.
pub struct Scene {
    font: Font,
    measurer: TextMeasurer,
    focus: Option<Path>,
    /// The node that received a press and holds the pointer until release.
    holder: Option<Path>,
    /// The node that received the last primary press, for click detection.
    pressed: Option<Path>,
    /// Text typed into a field: `(value the tree had, text shown)`.
    drafts: HashMap<Path, (String, String)>,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            font: Font::bundled(),
            measurer: TextMeasurer::new(),
            focus: None,
            holder: None,
            pressed: None,
            drafts: HashMap::new(),
        }
    }

    /// The path of the focused node, if any.
    pub fn focused(&self) -> Option<&Path> {
        self.focus.as_ref()
    }

    /// Lays out `ui`, applies this frame's `events` (dispatching any messages
    /// through `dispatch`), and paints the result into `canvas`.
    pub fn frame<Msg: Clone>(
        &mut self,
        canvas: &mut Canvas<'_>,
        ui: &UITree<Msg>,
        events: &[Event],
        dispatch: &dyn Fn(Msg),
    ) {
        let (w, h) = canvas.size();
        let mut taffy: TaffyTree<()> = TaffyTree::new();
        let root = layout::build(&mut taffy, &mut self.measurer, ui);
        let available = taffy::Size {
            width: taffy::AvailableSpace::Definite(w as f32),
            height: taffy::AvailableSpace::Definite(h as f32),
        };
        // A failed layout skips this frame
        // rather than taking the window down.
        if let Err(e) = taffy.compute_layout(root.taffy_id, available) {
            eprintln!("tpt-appfront-canvas: taffy compute_layout failed ({e:?}); skipping frame");
            return;
        }

        let mut hits = Vec::new();
        collect_hits(&root, &taffy, (0.0, 0.0), &mut Vec::new(), &mut hits);

        for event in events {
            self.handle_event(event, &hits, dispatch);
        }

        paint_node(self, canvas, &taffy, &root, (0.0, 0.0), &mut Vec::new());
    }

    fn handle_event<Msg: Clone>(
        &mut self,
        event: &Event,
        hits: &[Hit<'_, Msg>],
        dispatch: &dyn Fn(Msg),
    ) {
        match *event {
            Event::PointerMoved { x, y } => {
                let Some(holder) = self.holder.clone() else {
                    return;
                };
                if let Some(hit) = find(hits, &holder) {
                    if let Some(on_move) = &hit.ui.meta.on_pointer_move {
                        if let Some(msg) = on_move(pointer_event(hit.bounds, x, y, 0)) {
                            dispatch(msg);
                        }
                    }
                }
            }
            Event::PointerDown { x, y, button } => {
                let target = hit_at(hits, x, y);
                match target {
                    Some(hit) if hit.focusable => {
                        self.set_focus(Some(hit.path.clone()), hits, dispatch)
                    }
                    _ => self.set_focus(None, hits, dispatch),
                }
                self.pressed = target.map(|h| h.path.clone());
                if let Some(hit) = target {
                    if let Some(on_down) = &hit.ui.meta.on_pointer_down {
                        if let Some(msg) = on_down(pointer_event(hit.bounds, x, y, button.index()))
                        {
                            dispatch(msg);
                        }
                    }
                    let holds = hit.ui.meta.on_pointer_down.is_some()
                        || hit.ui.meta.on_pointer_move.is_some()
                        || hit.ui.meta.on_pointer_up.is_some();
                    if holds {
                        self.holder = Some(hit.path.clone());
                    }
                }
            }
            Event::PointerUp { x, y, button } => {
                if let Some(holder) = self.holder.take() {
                    if let Some(hit) = find(hits, &holder) {
                        if let Some(on_up) = &hit.ui.meta.on_pointer_up {
                            if let Some(msg) =
                                on_up(pointer_event(hit.bounds, x, y, button.index()))
                            {
                                dispatch(msg);
                            }
                        }
                    }
                }
                let released_on = hit_at(hits, x, y).map(|h| h.path.clone());
                let pressed = self.pressed.take();
                if button == MouseButton::Primary {
                    if let (Some(path), Some(pressed)) = (released_on, pressed) {
                        if path == pressed {
                            self.click(&path, hits, dispatch);
                        }
                    }
                }
            }
            Event::KeyDown {
                code,
                shift,
                ctrl,
                alt,
            } => {
                if let Some(name) = key_name(code) {
                    self.key(name.to_string(), None, shift, ctrl, alt, hits, dispatch);
                }
            }
            Event::Text(ch) => {
                self.key(
                    ch.to_string(),
                    Some(ch),
                    false,
                    false,
                    false,
                    hits,
                    dispatch,
                );
            }
            Event::Wheel { .. } | Event::Resized { .. } | Event::CloseRequested => {}
        }
    }

    /// Routes one key press to the focused node: its `on_keydown` first, then
    /// the default action (Tab moves focus, Enter/Space activate, typing edits).
    #[allow(clippy::too_many_arguments)]
    fn key<Msg: Clone>(
        &mut self,
        name: String,
        text: Option<char>,
        shift: bool,
        ctrl: bool,
        alt: bool,
        hits: &[Hit<'_, Msg>],
        dispatch: &dyn Fn(Msg),
    ) {
        if let Some(path) = self.focus.clone() {
            if let Some(hit) = find(hits, &path) {
                if let Some(on_key) = &hit.ui.meta.on_keydown {
                    let event = KeyEvent {
                        key: name.clone(),
                        code: String::new(),
                        ctrl,
                        shift,
                        alt,
                        meta: false,
                        repeat: false,
                    };
                    if let Some(msg) = on_key(event) {
                        dispatch(msg);
                        return;
                    }
                }
            }
        }

        if name == "Tab" {
            self.move_focus(shift, hits, dispatch);
            return;
        }

        let Some(path) = self.focus.clone() else {
            return;
        };
        let Some(hit) = find(hits, &path) else {
            return;
        };
        match (&hit.ui.kind, text, name.as_str()) {
            (NodeKind::Button { .. }, Some(' '), _)
            | (NodeKind::Button { .. }, None, "Enter")
            | (NodeKind::Checkbox { .. }, Some(' '), _) => self.click(&path, hits, dispatch),
            (NodeKind::Input { .. } | NodeKind::Textarea { .. }, Some(ch), _) => {
                self.edit(hit, dispatch, |v| format!("{v}{ch}"));
            }
            (NodeKind::Input { .. } | NodeKind::Textarea { .. }, None, "Backspace") => {
                self.edit(hit, dispatch, |v| {
                    let mut s = v.to_string();
                    s.pop();
                    s
                });
            }
            (NodeKind::Textarea { .. }, None, "Enter") => {
                self.edit(hit, dispatch, |v| format!("{v}\n"));
            }
            _ => {}
        }
    }

    fn set_focus<Msg: Clone>(
        &mut self,
        next: Option<Path>,
        hits: &[Hit<'_, Msg>],
        dispatch: &dyn Fn(Msg),
    ) {
        if self.focus == next {
            return;
        }
        if let Some(old) = self.focus.take() {
            if let Some(hit) = find(hits, &old) {
                if let Some(msg) = hit.ui.meta.on_blur.clone() {
                    dispatch(msg);
                }
            }
        }
        if let Some(new) = &next {
            if let Some(hit) = find(hits, new) {
                if let Some(msg) = hit.ui.meta.on_focus.clone() {
                    dispatch(msg);
                }
            }
        }
        self.focus = next;
    }

    /// Moves focus to the next (or, with `backward`, previous) focusable node,
    /// wrapping at the ends.
    fn move_focus<Msg: Clone>(
        &mut self,
        backward: bool,
        hits: &[Hit<'_, Msg>],
        dispatch: &dyn Fn(Msg),
    ) {
        let order: Vec<&Hit<'_, Msg>> = hits.iter().filter(|h| h.focusable).collect();
        if order.is_empty() {
            return;
        }
        let n = order.len();
        let current = self
            .focus
            .as_ref()
            .and_then(|f| order.iter().position(|h| &h.path == f));
        let next = match (current, backward) {
            (None, false) => 0,
            (None, true) => n - 1,
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
        };
        let path = order[next].path.clone();
        self.set_focus(Some(path), hits, dispatch);
    }

    /// Activates a node: a checkbox toggles, then `on_click` fires.
    fn click<Msg: Clone>(&self, path: &Path, hits: &[Hit<'_, Msg>], dispatch: &dyn Fn(Msg)) {
        let Some(hit) = find(hits, path) else {
            return;
        };
        if let NodeKind::Checkbox { checked, .. } = &hit.ui.kind {
            if let Some(on_toggle) = &hit.ui.meta.on_toggle {
                dispatch(on_toggle(!*checked));
            }
        }
        if let Some(msg) = hit.ui.meta.on_click.clone() {
            dispatch(msg);
        }
    }

    /// Applies a text change to a field and reports the new value through
    /// `on_input`.
    fn edit<Msg: Clone>(
        &mut self,
        hit: &Hit<'_, Msg>,
        dispatch: &dyn Fn(Msg),
        change: impl FnOnce(&str) -> String,
    ) {
        let base = value_of(hit.ui).to_string();
        let current = self.visible_value(&hit.path, &base);
        let next = change(&current);
        self.drafts.insert(hit.path.clone(), (base, next.clone()));
        if let Some(on_input) = &hit.ui.meta.on_input {
            dispatch(on_input(next));
        }
    }

    /// The text a field should show: the draft while the tree still shows the
    /// value the draft was based on, otherwise the tree's value.
    fn visible_value(&mut self, path: &Path, ui_value: &str) -> String {
        let stale = match self.drafts.get(path) {
            Some((base, text)) => {
                if base == ui_value {
                    return text.clone();
                }
                true
            }
            None => false,
        };
        if stale {
            self.drafts.remove(path);
        }
        ui_value.to_string()
    }
}

fn is_focusable<Msg>(ui: &UITree<Msg>) -> bool {
    matches!(
        ui.kind,
        NodeKind::Button { .. }
            | NodeKind::Input { .. }
            | NodeKind::Textarea { .. }
            | NodeKind::Checkbox { .. }
            | NodeKind::Select { .. }
    ) || ui.meta.on_focus.is_some()
        || ui.meta.on_keydown.is_some()
}

fn is_interactive<Msg>(ui: &UITree<Msg>) -> bool {
    is_focusable(ui)
        || ui.meta.on_click.is_some()
        || ui.meta.on_pointer_down.is_some()
        || ui.meta.on_pointer_move.is_some()
        || ui.meta.on_pointer_up.is_some()
}

fn value_of<Msg>(ui: &UITree<Msg>) -> &str {
    match &ui.kind {
        NodeKind::Input { value } | NodeKind::Textarea { value } => value,
        _ => "",
    }
}

fn node_bounds(tree: &TaffyTree<()>, id: taffy::NodeId, origin: (f32, f32)) -> Bounds {
    let layout = tree.layout(id).expect("computed layout");
    Bounds {
        x: origin.0 + layout.location.x,
        y: origin.1 + layout.location.y,
        w: layout.size.width,
        h: layout.size.height,
    }
}

fn collect_hits<'a, Msg>(
    node: &RenderNode<'a, Msg>,
    tree: &TaffyTree<()>,
    origin: (f32, f32),
    path: &mut Path,
    hits: &mut Vec<Hit<'a, Msg>>,
) {
    let bounds = node_bounds(tree, node.taffy_id, origin);
    hits.push(Hit {
        path: path.clone(),
        ui: node.ui,
        bounds,
        focusable: is_focusable(node.ui),
        interactive: is_interactive(node.ui),
    });
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        collect_hits(child, tree, (bounds.x, bounds.y), path, hits);
        path.pop();
    }
}

/// The topmost interactive node under `(x, y)`.
fn hit_at<'h, 'a, Msg>(hits: &'h [Hit<'a, Msg>], x: i32, y: i32) -> Option<&'h Hit<'a, Msg>> {
    let (px, py) = (x as f32, y as f32);
    hits.iter()
        .rev()
        .find(|h| h.interactive && h.bounds.contains(px, py))
}

fn find<'h, 'a, Msg>(hits: &'h [Hit<'a, Msg>], path: &Path) -> Option<&'h Hit<'a, Msg>> {
    hits.iter().find(|h| &h.path == path)
}

fn pointer_event(bounds: Bounds, x: i32, y: i32, button: i16) -> PointerEvent {
    PointerEvent {
        x: f64::from(x),
        y: f64::from(y),
        button,
        target_width: f64::from(bounds.w),
        target_height: f64::from(bounds.h),
        ..PointerEvent::default()
    }
}

/// The DOM `KeyboardEvent.key` name for a virtual-key code. Printable
/// characters arrive as [`Event::Text`] instead, so they are not listed here.
fn key_name(code: u32) -> Option<&'static str> {
    Some(match code {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x1B => "Escape",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x2E => "Delete",
        _ => return None,
    })
}

/// Vertical baseline that centres a line of `size` in `bounds`.
fn centred_baseline(bounds: Bounds, size: f32) -> f32 {
    bounds.y + bounds.h / 2.0 + size * 0.35
}

/// Draws one line of text with its top at `top`.
fn text_top(
    canvas: &mut Canvas<'_>,
    font: &mut Font,
    x: f32,
    top: f32,
    text: &str,
    size: f32,
    color: Rgb,
) {
    canvas.draw_text(font, x, top + size * 0.9, text, size, color);
}

fn paint_node<Msg: Clone>(
    scene: &mut Scene,
    canvas: &mut Canvas<'_>,
    tree: &TaffyTree<()>,
    node: &RenderNode<'_, Msg>,
    origin: (f32, f32),
    path: &mut Path,
) {
    let bounds = node_bounds(tree, node.taffy_id, origin);
    let style = layout::canvas_style_for(&node.ui.meta.class);
    let fg = style.foreground.unwrap_or(TEXT_COLOR);
    let size = (TEXT_FONT_SIZE + style.font_delta).max(8.0);
    let focused = scene.focus.as_ref() == Some(&*path);
    let rect = bounds.to_rect();

    match &node.ui.kind {
        NodeKind::Container { .. } | NodeKind::List { .. } | NodeKind::Portal { .. } => {
            if let Some(bg) = style.background {
                canvas.fill_rect(rect, bg);
            }
        }
        NodeKind::Heading { text, level } => {
            let heading = (layout::heading_font_size(*level) + style.font_delta).max(8.0);
            text_top(
                canvas,
                &mut scene.font,
                bounds.x,
                bounds.y,
                text,
                heading,
                fg,
            );
        }
        NodeKind::Text { text } => {
            text_top(canvas, &mut scene.font, bounds.x, bounds.y, text, size, fg);
        }
        NodeKind::Button { label } => {
            canvas.fill_rect(rect, style.background.unwrap_or(BUTTON_BG));
            let width = scene.font.measure(label, size);
            let x = bounds.x + (bounds.w - width) / 2.0;
            canvas.draw_text(
                &mut scene.font,
                x,
                centred_baseline(bounds, size),
                label,
                size,
                fg,
            );
            draw_frame(canvas, rect, focused);
        }
        NodeKind::Input { .. } => {
            let value = scene.visible_value(path, value_of(node.ui));
            canvas.fill_rect(rect, FIELD_BG);
            canvas.with_clip(rect, |canvas| {
                let advance = canvas.draw_text(
                    &mut scene.font,
                    bounds.x + FIELD_PAD,
                    centred_baseline(bounds, size),
                    &value,
                    size,
                    fg,
                );
                if focused {
                    let caret_x = (bounds.x + FIELD_PAD + advance).round() as i32;
                    let caret_h = (size * 1.1).round() as i32;
                    let caret_y = (bounds.y + bounds.h / 2.0 - caret_h as f32 / 2.0).round() as i32;
                    canvas.fill_rect(Rect::new(caret_x, caret_y, 1, caret_h), fg);
                }
            });
            draw_frame(canvas, rect, focused);
        }
        NodeKind::Textarea { .. } => {
            let value = scene.visible_value(path, value_of(node.ui));
            canvas.fill_rect(rect, FIELD_BG);
            let line_h = size * 1.4;
            canvas.with_clip(rect, |canvas| {
                let mut last = (bounds.x + FIELD_PAD, bounds.y + FIELD_PAD);
                for (i, line) in value.split('\n').enumerate() {
                    let top = bounds.y + FIELD_PAD + i as f32 * line_h;
                    let advance = canvas.draw_text(
                        &mut scene.font,
                        bounds.x + FIELD_PAD,
                        top + size * 0.9,
                        line,
                        size,
                        fg,
                    );
                    last = (bounds.x + FIELD_PAD + advance, top);
                }
                if focused {
                    let caret_x = last.0.round() as i32;
                    let caret_h = (size * 1.1).round() as i32;
                    canvas.fill_rect(Rect::new(caret_x, last.1.round() as i32, 1, caret_h), fg);
                }
            });
            draw_frame(canvas, rect, focused);
        }
        NodeKind::Checkbox { label, checked } => {
            let box_top = (bounds.y + (bounds.h - CHECKBOX_SIZE) / 2.0).round() as i32;
            let box_rect = Rect::new(
                bounds.x.round() as i32,
                box_top,
                CHECKBOX_SIZE as i32,
                CHECKBOX_SIZE as i32,
            );
            canvas.fill_rect(box_rect, FIELD_BG);
            draw_frame(canvas, box_rect, focused);
            if *checked {
                let inset = 3;
                canvas.fill_rect(
                    Rect::new(
                        box_rect.x + inset,
                        box_rect.y + inset,
                        box_rect.w - 2 * inset,
                        box_rect.h - 2 * inset,
                    ),
                    CHECK_COLOR,
                );
            }
            let label_x = bounds.x + CHECKBOX_SIZE + CHECKBOX_GAP;
            canvas.draw_text(
                &mut scene.font,
                label_x,
                centred_baseline(bounds, size),
                label,
                size,
                fg,
            );
        }
        NodeKind::Select { options, selected } => {
            let label = options
                .iter()
                .find(|(v, _)| v == selected)
                .map(|(_, l)| l.as_str())
                .unwrap_or(selected.as_str());
            canvas.fill_rect(rect, FIELD_BG);
            canvas.draw_text(
                &mut scene.font,
                bounds.x + FIELD_PAD,
                centred_baseline(bounds, size),
                label,
                size,
                fg,
            );
            let chevron_w = scene.font.measure("v", size);
            canvas.draw_text(
                &mut scene.font,
                bounds.x + bounds.w - FIELD_PAD - chevron_w,
                centred_baseline(bounds, size),
                "v",
                size,
                fg,
            );
            draw_frame(canvas, rect, focused);
        }
        NodeKind::Radio {
            options, selected, ..
        } => {
            let text: String = options
                .iter()
                .map(|(v, l)| {
                    if v == selected {
                        format!("(*) {l}")
                    } else {
                        format!("( ) {l}")
                    }
                })
                .collect::<Vec<_>>()
                .join("   ");
            text_top(canvas, &mut scene.font, bounds.x, bounds.y, &text, size, fg);
        }
        NodeKind::DataGrid { columns, rows } => {
            paint_data_grid(
                canvas,
                &mut scene.font,
                tree,
                node,
                bounds,
                columns,
                rows,
                size,
                fg,
            );
        }
        NodeKind::Image { src, alt } => {
            let caption = if alt.is_empty() {
                src.as_str()
            } else {
                alt.as_str()
            };
            text_top(
                canvas,
                &mut scene.font,
                bounds.x,
                bounds.y,
                caption,
                size,
                fg,
            );
        }
        NodeKind::Link { href, text } => {
            let label = format!("{text} \u{2192} {href}");
            text_top(
                canvas,
                &mut scene.font,
                bounds.x,
                bounds.y,
                &label,
                size,
                fg,
            );
        }
        NodeKind::Media {
            src,
            alt,
            media_type,
        } => {
            let kind = match media_type {
                tpt_appfront_core::ui_tree::MediaType::Audio => "audio",
                tpt_appfront_core::ui_tree::MediaType::Video => "video",
            };
            let label = if alt.is_empty() {
                format!("{kind}: {src}")
            } else {
                format!("{kind}: {alt} ({src})")
            };
            text_top(
                canvas,
                &mut scene.font,
                bounds.x,
                bounds.y,
                &label,
                size,
                fg,
            );
        }
    }

    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        paint_node(scene, canvas, tree, child, (bounds.x, bounds.y), path);
        path.pop();
    }
}

/// Border for a widget: grey normally, blue and thicker when focused.
fn draw_frame(canvas: &mut Canvas<'_>, rect: Rect, focused: bool) {
    if focused {
        canvas.stroke_rect(rect, FOCUS_COLOR, 2);
    } else {
        canvas.stroke_rect(rect, BORDER_COLOR, 1);
    }
}

/// Paints a `DataGrid`'s cells using the rows and cell ids that `layout.rs`
/// recorded, so spacer rows from virtualised grids are skipped correctly.
#[allow(clippy::too_many_arguments)]
fn paint_data_grid<Msg>(
    canvas: &mut Canvas<'_>,
    font: &mut Font,
    tree: &TaffyTree<()>,
    node: &RenderNode<'_, Msg>,
    bounds: Bounds,
    columns: &[String],
    rows: &[Vec<String>],
    size: f32,
    fg: Rgb,
) {
    let row_ids = tree.children(node.taffy_id).expect("data grid rows");
    let grid_cells = node
        .grid_cells
        .as_ref()
        .expect("data grid built with grid_cells");
    for (row_idx, row_id) in row_ids.iter().enumerate() {
        let grid_row = &grid_cells[row_idx];
        let texts: &[String] = match grid_row.kind {
            GridRowKind::Header => columns,
            GridRowKind::Spacer => continue,
            GridRowKind::Data(source_idx) => &rows[source_idx],
        };
        let row = node_bounds(tree, *row_id, (bounds.x, bounds.y));
        for (cell_idx, cell_id) in grid_row.cells.iter().enumerate() {
            let Some(text) = texts.get(cell_idx) else {
                continue;
            };
            let cell = node_bounds(tree, *cell_id, (row.x, row.y));
            text_top(
                canvas,
                font,
                cell.x + CELL_PADDING,
                cell.y + CELL_PADDING / 2.0,
                text,
                size,
                fg,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tpt_appfront_core::{ContainerBuilder, UITree};

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Clicked(&'static str),
        Focus(&'static str),
        Blur(&'static str),
        Typed(String),
        Toggled(bool),
        Key(&'static str),
        Down(i32, i32),
        Up(i32, i32),
        Moved(i32, i32),
    }

    const WIDTH: usize = 320;
    const HEIGHT: usize = 200;

    /// Paints one frame of `ui` with `events` into a fresh buffer and returns
    /// the messages dispatched, in order.
    fn frame(scene: &mut Scene, ui: &UITree<Msg>, events: &[Event]) -> (Vec<Msg>, Vec<u32>) {
        let log: RefCell<Vec<Msg>> = RefCell::new(Vec::new());
        let dispatch = |m: Msg| log.borrow_mut().push(m);
        let mut pixels = vec![0xFFFFFFu32; WIDTH * HEIGHT];
        {
            let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT);
            scene.frame(&mut canvas, ui, events, &dispatch);
        }
        (log.into_inner(), pixels)
    }

    fn down(x: i32, y: i32) -> Event {
        Event::PointerDown {
            x,
            y,
            button: MouseButton::Primary,
        }
    }

    fn up(x: i32, y: i32) -> Event {
        Event::PointerUp {
            x,
            y,
            button: MouseButton::Primary,
        }
    }

    fn key(code: u32) -> Event {
        Event::KeyDown {
            code,
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    /// A single button as the root, which lays out at (0, 0).
    fn root_button() -> UITree<Msg> {
        let mut b = ContainerBuilder::new();
        b.button("Save").on_click(Msg::Clicked("save"));
        b.into_only_child().unwrap()
    }

    #[test]
    fn clicking_a_button_dispatches_its_click_message() {
        let mut scene = Scene::new();
        let ui = root_button();
        let (msgs, _) = frame(&mut scene, &ui, &[down(4, 4), up(4, 4)]);
        assert_eq!(msgs, vec![Msg::Clicked("save")]);
    }

    #[test]
    fn press_and_release_on_different_nodes_is_not_a_click() {
        let mut scene = Scene::new();
        let ui = root_button();
        let (msgs, _) = frame(&mut scene, &ui, &[down(4, 4), up(400, 4)]);
        assert!(msgs.is_empty());
    }

    #[test]
    fn a_button_paints_its_background_and_label() {
        let mut scene = Scene::new();
        let ui = root_button();
        let (_, pixels) = frame(&mut scene, &ui, &[]);
        // Inside the button, away from the text, is the default button fill.
        assert_eq!(pixels[2 * WIDTH + 2], BUTTON_BG);
        // Something other than the fill is drawn somewhere (the label).
        assert!(pixels.iter().any(|&p| p != BUTTON_BG && p != 0xFFFFFF));
    }

    #[test]
    fn tab_cycles_focus_and_fires_focus_and_blur() {
        let ui = UITree::container(|c| {
            c.button("A")
                .on_focus(Msg::Focus("a"))
                .on_blur(Msg::Blur("a"));
            c.button("B")
                .on_focus(Msg::Focus("b"))
                .on_blur(Msg::Blur("b"));
        });
        let mut scene = Scene::new();

        let (msgs, _) = frame(&mut scene, &ui, &[key(0x09)]);
        assert_eq!(msgs, vec![Msg::Focus("a")]);
        let (msgs, _) = frame(&mut scene, &ui, &[key(0x09)]);
        assert_eq!(msgs, vec![Msg::Blur("a"), Msg::Focus("b")]);
        let (msgs, _) = frame(&mut scene, &ui, &[key(0x09)]);
        assert_eq!(msgs, vec![Msg::Blur("b"), Msg::Focus("a")]);
    }

    #[test]
    fn enter_on_a_focused_button_clicks_it_unless_keydown_consumes_it() {
        let mut b = ContainerBuilder::new();
        b.button("Go").on_click(Msg::Clicked("go"));
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        frame(&mut scene, &ui, &[down(4, 4), up(4, 4)]); // focus it via click
        let (msgs, _) = frame(&mut scene, &ui, &[key(0x0D)]);
        assert_eq!(msgs, vec![Msg::Clicked("go")]);

        let mut b = ContainerBuilder::new();
        b.button("Go")
            .on_click(Msg::Clicked("go"))
            .on_keydown(|e| (e.key == "Enter").then_some(Msg::Key("enter")));
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        frame(&mut scene, &ui, &[down(4, 4), up(4, 4)]);
        let (msgs, _) = frame(&mut scene, &ui, &[key(0x0D)]);
        assert_eq!(msgs, vec![Msg::Key("enter")]);
    }

    #[test]
    fn typing_into_a_field_reports_each_edit_through_on_input() {
        let mut b = ContainerBuilder::new();
        b.input("ab").on_input(Msg::Typed);
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        frame(&mut scene, &ui, &[down(4, 4), up(4, 4)]); // focus

        let (msgs, _) = frame(&mut scene, &ui, &[Event::Text('c')]);
        assert_eq!(msgs, vec![Msg::Typed("abc".into())]);
        // The tree still shows "ab" (the app has not echoed yet), so the draft
        // carries on: a backspace removes the typed "c".
        let (msgs, _) = frame(&mut scene, &ui, &[key(0x08)]);
        assert_eq!(msgs, vec![Msg::Typed("ab".into())]);
    }

    #[test]
    fn a_field_shows_the_value_the_app_sends_back() {
        let mut b = ContainerBuilder::new();
        b.input("ab").on_input(Msg::Typed);
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        frame(&mut scene, &ui, &[down(4, 4), up(4, 4)]);
        frame(&mut scene, &ui, &[Event::Text('c')]);

        // The app now renders "abc"; the draft is dropped and typing continues from it.
        let mut b = ContainerBuilder::new();
        b.input("abc").on_input(Msg::Typed);
        let ui = b.into_only_child().unwrap();
        let (msgs, _) = frame(&mut scene, &ui, &[Event::Text('d')]);
        assert_eq!(msgs, vec![Msg::Typed("abcd".into())]);
    }

    #[test]
    fn enter_adds_a_line_in_a_textarea_but_not_in_an_input() {
        let mut b = ContainerBuilder::new();
        b.textarea("x").on_input(Msg::Typed);
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        frame(&mut scene, &ui, &[down(4, 4), up(4, 4)]);
        let (msgs, _) = frame(&mut scene, &ui, &[key(0x0D)]);
        assert_eq!(msgs, vec![Msg::Typed("x\n".into())]);

        let mut b = ContainerBuilder::new();
        b.input("x").on_input(Msg::Typed);
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        frame(&mut scene, &ui, &[down(4, 4), up(4, 4)]);
        let (msgs, _) = frame(&mut scene, &ui, &[key(0x0D)]);
        assert!(msgs.is_empty());
    }

    #[test]
    fn clicking_a_checkbox_toggles_it() {
        let mut b = ContainerBuilder::new();
        b.checkbox("Agree", false).on_toggle(Msg::Toggled);
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        let (msgs, _) = frame(&mut scene, &ui, &[down(4, 4), up(4, 4)]);
        assert_eq!(msgs, vec![Msg::Toggled(true)]);
    }

    #[test]
    fn a_checked_checkbox_paints_its_tick() {
        let mut b = ContainerBuilder::new();
        b.checkbox("Agree", true);
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        let (_, pixels) = frame(&mut scene, &ui, &[]);
        assert!(pixels.contains(&CHECK_COLOR));
    }

    #[test]
    fn pointer_down_up_and_move_reach_the_holder() {
        let mut b = ContainerBuilder::new();
        b.button("Drag")
            .on_pointer_down(|e| Some(Msg::Down(e.x as i32, e.y as i32)))
            .on_pointer_move(|e| Some(Msg::Moved(e.x as i32, e.y as i32)))
            .on_pointer_up(|e| Some(Msg::Up(e.x as i32, e.y as i32)));
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        let events = [
            down(4, 4),
            // Outside the button, but the holder keeps receiving moves.
            Event::PointerMoved { x: 200, y: 150 },
            up(200, 150),
        ];
        let (msgs, _) = frame(&mut scene, &ui, &events);
        assert_eq!(
            msgs,
            vec![Msg::Down(4, 4), Msg::Moved(200, 150), Msg::Up(200, 150)]
        );
    }

    #[test]
    fn text_fields_paint_their_value_in_the_field() {
        let mut b = ContainerBuilder::new();
        b.input("Hi");
        let ui = b.into_only_child().unwrap();
        let mut scene = Scene::new();
        let (_, pixels) = frame(&mut scene, &ui, &[]);
        // The border is light grey, so any dark pixel can only be text.
        assert!(pixels.iter().any(|&p| p < 0x808080));
    }
}
