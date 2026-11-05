//! Paints a `RenderNode` tree (see `layout.rs`) using `egui` widgets placed
//! at the exact rects `taffy` computed — the "`UITree` → egui widgets"
//! mapping from `spec.txt` (`Container`→plain area, `Button`→`Button`,
//! etc.), kept separate from the layout math itself.

use std::rc::Rc;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Vec2};
use taffy::TaffyTree;
use tpt_appfront_core::{
    KeyEvent as UiKeyEvent, NodeKind, NodeMeta, PointerEvent as UiPointerEvent, ui_tree::MediaType,
};

use crate::layout::{self, CELL_PADDING, GridRowKind, RenderNode, TEXT_FONT_SIZE};

/// Paints `node` (and its subtree) into `ui`, with `origin` being the
/// absolute screen position of `node`'s parent's content box. `id_seed` is
/// bumped per node so every `egui::Id` used for click-interaction is unique
/// within a frame.
pub fn paint<Msg: Clone>(
    ui: &mut egui::Ui,
    tree: &TaffyTree<()>,
    node: &RenderNode<Msg>,
    origin: Pos2,
    dispatch: &Rc<dyn Fn(Msg)>,
    id_seed: &mut u64,
) {
    let layout = tree.layout(node.taffy_id).expect("computed layout");
    let pos = origin + Vec2::new(layout.location.x, layout.location.y);
    let size = Vec2::new(layout.size.width, layout.size.height);
    let rect = Rect::from_min_size(pos, size);

    // Utility-class styling adapter: honor `bg-*`/`text-*` utilities on canvas
    // (they were previously a silent no-op here; DOM/HTML already honored them).
    let style = layout::canvas_style_for(&node.ui.meta.class);

    *id_seed += 1;
    let id = egui::Id::new(("appfront_canvas_node", *id_seed));

    let mut clicked = false;
    match &node.ui.kind {
        NodeKind::Container { .. } | NodeKind::List { .. } | NodeKind::Portal { .. } => {
            if let Some(bg) = style.background {
                ui.painter().rect_filled(rect, 0.0, bg);
            }
        }
        NodeKind::Heading { text, level } => {
            paint_text(
                ui,
                pos,
                text,
                layout::heading_font_size(*level),
                style.foreground,
            );
            #[cfg(feature = "accesskit")]
            name_accessible_node(ui, rect, id, text, Some(egui::accesskit::Role::Heading));
        }
        NodeKind::Text { text } => {
            paint_text(ui, pos, text, TEXT_FONT_SIZE, style.foreground);
            #[cfg(feature = "accesskit")]
            name_accessible_node(ui, rect, id, text, None);
        }
        NodeKind::Button { label } => {
            let mut button = egui::Button::new(label.as_str());
            if let Some(bg) = style.background {
                button = button.fill(bg);
            }
            dispatch_keydown(ui, id, &node.ui.meta, dispatch);
            let response = ui.put(rect, button);
            clicked = response.clicked();
            dispatch_after(ui, id, rect, &response, &node.ui.meta, dispatch);
            #[cfg(feature = "accesskit")]
            {
                response.set_accessible_name(label.as_str());
                if let Some(desc) = &node.ui.meta.ai.description {
                    response.set_accessible_description(desc.as_str());
                }
            }
        }
        NodeKind::Input { value } => {
            let mut value = value.clone();
            // `ui.put` has side effects (places the widget); the returned
            // `Response` is only needed for the AccessKit name below, so bind
            // it as `_response` to avoid an unused-variable warning when the
            // `accesskit` feature is off.
            dispatch_keydown(ui, id, &node.ui.meta, dispatch);
            let response = ui.put(rect, egui::TextEdit::singleline(&mut value).id(id));
            dispatch_after(ui, id, rect, &response, &node.ui.meta, dispatch);
            #[cfg(feature = "accesskit")]
            {
                // Give the field a name so a screen reader announces it; prefer
                // an explicit AI description, then the class, else a generic
                // placeholder derived from the current value.
                let name = node
                    .ui
                    .meta
                    .ai
                    .description
                    .clone()
                    .or_else(|| node.ui.meta.class.clone())
                    .unwrap_or_else(|| format!("Text input: {value}"));
                response.set_accessible_name(name);
            }
        }
        NodeKind::Textarea { value } => {
            let mut value = value.clone();
            dispatch_keydown(ui, id, &node.ui.meta, dispatch);
            let response = ui.put(rect, egui::TextEdit::multiline(&mut value).id(id));
            dispatch_after(ui, id, rect, &response, &node.ui.meta, dispatch);
            #[cfg(feature = "accesskit")]
            {
                let name = node
                    .ui
                    .meta
                    .ai
                    .description
                    .clone()
                    .or_else(|| node.ui.meta.class.clone())
                    .unwrap_or_else(|| format!("Text area: {value}"));
                response.set_accessible_name(name);
            }
        }
        NodeKind::Checkbox { label, checked } => {
            let mut checked = *checked;
            let _response = ui.put(rect, egui::Checkbox::new(&mut checked, label.as_str()));
            #[cfg(feature = "accesskit")]
            _response.set_accessible_name(label.as_str());
        }
        NodeKind::Select { options, selected } => {
            let label = options
                .iter()
                .find(|(v, _)| v == selected)
                .map(|(_, l)| l.as_str())
                .unwrap_or(selected.as_str());
            paint_text(
                ui,
                pos,
                &format!("{label} \u{25be}"),
                TEXT_FONT_SIZE,
                style.foreground,
            );
            #[cfg(feature = "accesskit")]
            name_accessible_node(ui, rect, id, label, None);
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
            paint_text(ui, pos, &text, TEXT_FONT_SIZE, style.foreground);
            #[cfg(feature = "accesskit")]
            name_accessible_node(ui, rect, id, &text, None);
        }
        NodeKind::DataGrid { columns, rows } => {
            paint_data_grid(ui, tree, node, pos, columns, rows);
        }
        NodeKind::Image { src, alt } => {
            let caption = if alt.is_empty() {
                src.as_str()
            } else {
                alt.as_str()
            };
            paint_text(ui, pos, caption, TEXT_FONT_SIZE, style.foreground);
            #[cfg(feature = "accesskit")]
            name_accessible_node(
                ui,
                rect,
                id,
                if alt.is_empty() {
                    src.as_str()
                } else {
                    alt.as_str()
                },
                Some(egui::accesskit::Role::Image),
            );
        }
        NodeKind::Link { href, text } => {
            let label = format!("{text} \u{2192} {href}");
            paint_text(ui, pos, &label, TEXT_FONT_SIZE, style.foreground);
            #[cfg(feature = "accesskit")]
            name_accessible_node(ui, rect, id, text, Some(egui::accesskit::Role::Link));
        }
        NodeKind::Media {
            src,
            alt,
            media_type,
        } => {
            let kind = match media_type {
                MediaType::Audio => "audio",
                MediaType::Video => "video",
            };
            let label = if alt.is_empty() {
                format!("{kind}: {src}")
            } else {
                format!("{kind}: {alt} ({src})")
            };
            paint_text(ui, pos, &label, TEXT_FONT_SIZE, style.foreground);
            #[cfg(feature = "accesskit")]
            name_accessible_node(ui, rect, id, alt, None);
        }
    }

    // Widgets above have their own response, so they only get the click fallback. Other nodes
    // get a focusable interaction when they have any handler, which also carries focus and keys.
    let meta = &node.ui.meta;
    if is_widget(&node.ui.kind) {
        if !clicked && meta.on_click.is_some() {
            let response = ui.interact(rect, id, Sense::click());
            clicked = response.clicked();
        }
    } else if meta.on_click.is_some() || has_input_handlers(meta) {
        dispatch_keydown(ui, id, meta, dispatch);
        let response = ui.interact(rect, id, Sense::click());
        clicked = response.clicked();
        dispatch_after(ui, id, rect, &response, meta, dispatch);
    }

    if clicked {
        if let Some(msg) = node.ui.meta.on_click.clone() {
            dispatch(msg);
        }
    }

    for child in &node.children {
        paint(ui, tree, child, pos, dispatch, id_seed);
    }
}

/// Nodes that paint their own egui widget, and so have their own response.
fn is_widget<Msg>(kind: &NodeKind<Msg>) -> bool {
    matches!(
        kind,
        NodeKind::Button { .. }
            | NodeKind::Input { .. }
            | NodeKind::Textarea { .. }
            | NodeKind::Checkbox { .. }
            | NodeKind::Select { .. }
    )
}

fn has_input_handlers<Msg>(meta: &NodeMeta<Msg>) -> bool {
    meta.on_focus.is_some()
        || meta.on_blur.is_some()
        || meta.on_keydown.is_some()
        || meta.on_pointer_down.is_some()
}

/// The key name the DOM uses for KeyboardEvent.key, from an egui key. Letters follow Shift.
fn key_name(key: egui::Key, shift: bool) -> String {
    let name = key.name();
    match name {
        "Up" => "ArrowUp".to_string(),
        "Down" => "ArrowDown".to_string(),
        "Left" => "ArrowLeft".to_string(),
        "Right" => "ArrowRight".to_string(),
        "Escape" | "Esc" => "Escape".to_string(),
        "Space" => " ".to_string(),
        _ if name.len() == 1 && name.chars().all(|c| c.is_ascii_alphabetic()) => {
            if shift {
                name.to_ascii_uppercase()
            } else {
                name.to_ascii_lowercase()
            }
        }
        _ => name.to_string(),
    }
}

fn ui_key_event(key: egui::Key, modifiers: egui::Modifiers, repeat: bool) -> UiKeyEvent {
    UiKeyEvent {
        key: key_name(key, modifiers.shift),
        code: String::new(),
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        alt: modifiers.alt,
        meta: modifiers.mac_cmd,
        repeat,
    }
}

/// The DOM button numbering: 0 primary, 1 middle, 2 secondary, then the extra buttons.
fn pointer_button_index(button: egui::PointerButton) -> i16 {
    match button {
        egui::PointerButton::Primary => 0,
        egui::PointerButton::Middle => 1,
        egui::PointerButton::Secondary => 2,
        egui::PointerButton::Extra1 => 3,
        egui::PointerButton::Extra2 => 4,
    }
}

/// Runs the focused node's keydown handler before its widget sees the keys. A key the handler
/// takes is consumed, so the widget (for example a button that activates on Enter) does not also
/// act on it.
fn dispatch_keydown<Msg: Clone>(
    ui: &egui::Ui,
    node_id: egui::Id,
    meta: &NodeMeta<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
) {
    let Some(on_keydown) = meta.on_keydown.as_ref() else {
        return;
    };
    // The widget's own id is only known once it has painted, so use the id recorded last frame.
    let Some(widget_id) = ui.data(|d| d.get_temp::<egui::Id>(node_id)) else {
        return;
    };
    if !ui.memory(|m| m.has_focus(widget_id)) {
        return;
    }
    let presses: Vec<(egui::Key, egui::Modifiers, bool)> = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } => Some((*key, *modifiers, *repeat)),
                _ => None,
            })
            .collect()
    });
    for (key, modifiers, repeat) in presses {
        if let Some(msg) = on_keydown(ui_key_event(key, modifiers, repeat)) {
            ui.input_mut(|i| {
                i.consume_key(modifiers, key);
            });
            dispatch(msg);
        }
    }
}

/// Dispatches focus, blur and pointer-down for a node once its widget has been placed. A
/// pointer-down is a press this frame that started inside rect.
fn dispatch_after<Msg: Clone>(
    ui: &egui::Ui,
    node_id: egui::Id,
    rect: Rect,
    response: &egui::Response,
    meta: &NodeMeta<Msg>,
    dispatch: &Rc<dyn Fn(Msg)>,
) {
    // Record the widget's id for the next frame's dispatch_keydown.
    ui.data_mut(|d| d.insert_temp(node_id, response.id));
    if response.gained_focus() {
        if let Some(msg) = meta.on_focus.clone() {
            dispatch(msg);
        }
    }
    if response.lost_focus() {
        if let Some(msg) = meta.on_blur.clone() {
            dispatch(msg);
        }
    }
    let Some(on_pointer_down) = meta.on_pointer_down.as_ref() else {
        return;
    };
    let presses: Vec<(egui::Pos2, egui::PointerButton, egui::Modifiers)> = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                } if rect.contains(*pos) => Some((*pos, *button, *modifiers)),
                _ => None,
            })
            .collect()
    });
    for (pos, button, modifiers) in presses {
        let event = UiPointerEvent {
            x: f64::from(pos.x),
            y: f64::from(pos.y),
            button: pointer_button_index(button),
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            meta: modifiers.mac_cmd,
        };
        if let Some(msg) = on_pointer_down(event) {
            dispatch(msg);
        }
    }
}

fn paint_text(ui: &egui::Ui, pos: Pos2, text: &str, font_size: f32, color: Option<Color32>) {
    let text_color = color.unwrap_or_else(|| ui.visuals().text_color());
    ui.painter().text(
        pos,
        Align2::LEFT_TOP,
        text,
        FontId::proportional(font_size),
        text_color,
    );
}

/// Registers a non-focusable AccessKit node for a painted (non-widget) node
/// — egui's `painter().text` produces no accessible node on its own, so
/// without this a screen reader sees nothing for canvas `Heading`/`Text`.
/// `Sense::hover()` makes the node present to AT without making it
/// keyboard-focusable. Only compiled when the `accesskit` feature is on.
#[cfg(feature = "accesskit")]
fn name_accessible_node(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    name: &str,
    role: Option<egui::accesskit::Role>,
) {
    let response = ui.interact(rect, id, Sense::hover());
    response.set_accessible_name(name);
    if let Some(role) = role {
        response.set_accessible_role(role);
    }
}

/// `DataGrid` cells aren't real `RenderNode`s (see `layout::build_data_grid`)
/// since there's no per-cell `UITree` node to recurse into — their taffy
/// ids are recovered here via `TaffyTree::children` plus the `grid_cells`
/// ids `layout.rs` stashed on the node.
fn paint_data_grid<Msg>(
    ui: &egui::Ui,
    tree: &TaffyTree<()>,
    node: &RenderNode<Msg>,
    grid_origin: Pos2,
    columns: &[String],
    rows: &[Vec<String>],
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
        let row_layout = tree.layout(*row_id).expect("row layout");
        let row_origin = grid_origin + Vec2::new(row_layout.location.x, row_layout.location.y);
        let cell_ids = &grid_row.cells;

        for (cell_idx, cell_id) in cell_ids.iter().enumerate() {
            let Some(text) = texts.get(cell_idx) else {
                continue;
            };
            let cell_layout = tree.layout(*cell_id).expect("cell layout");
            let cell_pos = row_origin
                + Vec2::new(cell_layout.location.x, cell_layout.location.y)
                + Vec2::new(CELL_PADDING, CELL_PADDING / 2.0);
            paint_text(ui, cell_pos, text, TEXT_FONT_SIZE, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use egui::{Event, PointerButton, Pos2 as EguiPos2, RawInput};
    use tpt_appfront_core::UITree;

    use super::*;
    use crate::text::TextMeasurer;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Clicked,
        Focus,
        Key(String),
        Down(i16, i32),
    }

    /// Lays `ui` out with `taffy` and paints it into a fresh headless
    /// `egui::Context`, feeding `input` for that single frame. Returns every
    /// `Msg` the paint pass dispatched, in order.
    fn run_paint_frame(ctx: &egui::Context, ui: &UITree<Msg>, input: RawInput) -> Vec<Msg> {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let mut measurer = TextMeasurer::new();
        let root = layout::build(&mut tree, &mut measurer, ui);
        tree.compute_layout(
            root.taffy_id,
            taffy::Size {
                width: taffy::AvailableSpace::Definite(800.0),
                height: taffy::AvailableSpace::Definite(600.0),
            },
        )
        .expect("compute_layout");

        let dispatched: Rc<RefCell<Vec<Msg>>> = Rc::new(RefCell::new(Vec::new()));
        let dispatched_clone = dispatched.clone();
        let dispatch: Rc<dyn Fn(Msg)> = Rc::new(move |msg| dispatched_clone.borrow_mut().push(msg));

        let _ = ctx.run_ui(input, |ui| {
            let origin = ui.min_rect().min;
            let mut id_seed = 0u64;
            paint(ui, &tree, &root, origin, &dispatch, &mut id_seed);
        });
        drop(dispatch);

        Rc::try_unwrap(dispatched).unwrap().into_inner()
    }

    #[test]
    fn paint_handles_every_node_kind_without_panicking() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.heading(1, "Title");
            c.text("Body text");
            c.button("Go");
            c.input("value");
            c.data_grid(["Name", "Age"], [["Alice", "30"], ["Bob", "25"]]);
            c.list(|l| {
                l.text("item one");
                l.text("item two");
            });
            c.textarea("notes");
            c.checkbox("Agree", true);
            c.select([("a", "Alpha"), ("b", "Beta")], "b");
            c.radio_group("color", [("r", "Red"), ("g", "Green")], "g");
        });
        let ctx = egui::Context::default();
        let dispatched = run_paint_frame(&ctx, &ui, RawInput::default());
        assert!(dispatched.is_empty());
    }

    #[test]
    fn button_without_pointer_input_does_not_dispatch() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Go").on_click(Msg::Clicked);
        });
        let ctx = egui::Context::default();
        let dispatched = run_paint_frame(&ctx, &ui, RawInput::default());
        assert!(dispatched.is_empty());
    }

    #[test]
    fn clicking_a_button_dispatches_its_on_click_msg() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Go").on_click(Msg::Clicked);
        });
        let ctx = egui::Context::default();

        // Button sits at the container's padded top-left corner; well inside
        // its painted rect regardless of exact text measurement.
        let pos = EguiPos2::new(10.0, 10.0);

        // Widget hit-testing is based on the *previous* frame's registered
        // rects, so a warm-up frame is needed before the button's rect is
        // known to egui's interaction state.
        run_paint_frame(&ctx, &ui, RawInput::default());

        let click = RawInput {
            events: vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        };

        assert_eq!(run_paint_frame(&ctx, &ui, click), vec![Msg::Clicked]);
    }

    #[test]
    fn clicking_a_non_button_node_with_on_click_dispatches_via_interact_fallback() {
        // `Text`/`Heading`/etc. aren't real egui widgets, so a click on them
        // goes through the `ui.interact` fallback at the bottom of `paint`
        // rather than a widget's own `Response::clicked()`.
        let ui: UITree<Msg> = UITree::container(|c| {
            c.text("click me").on_click(Msg::Clicked);
        });
        let ctx = egui::Context::default();
        let pos = EguiPos2::new(10.0, 10.0);

        run_paint_frame(&ctx, &ui, RawInput::default());

        let click = RawInput {
            events: vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        };

        assert_eq!(run_paint_frame(&ctx, &ui, click), vec![Msg::Clicked]);
    }

    fn key_press(key: egui::Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn key_names_follow_the_dom() {
        assert_eq!(key_name(egui::Key::ArrowUp, false), "ArrowUp");
        assert_eq!(key_name(egui::Key::Escape, false), "Escape");
        assert_eq!(key_name(egui::Key::A, false), "a");
        assert_eq!(key_name(egui::Key::A, true), "A");
    }

    #[test]
    fn tab_focuses_a_button_and_dispatches_its_focus_message() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Go")
                .on_click(Msg::Clicked)
                .on_focus(Msg::Focus)
                .on_keydown(|k| (k.key == "Enter").then_some(Msg::Key(k.key)));
        });
        let ctx = egui::Context::default();
        // A warm-up frame registers the widget, so that Tab can reach it.
        run_paint_frame(&ctx, &ui, RawInput::default());
        let tab = RawInput {
            events: vec![key_press(egui::Key::Tab)],
            ..Default::default()
        };
        assert_eq!(run_paint_frame(&ctx, &ui, tab), vec![Msg::Focus]);
    }

    #[test]
    fn a_handled_enter_reaches_the_keydown_handler_and_not_the_button_click() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Go")
                .on_click(Msg::Clicked)
                .on_focus(Msg::Focus)
                .on_keydown(|k| (k.key == "Enter").then_some(Msg::Key(k.key)));
        });
        let ctx = egui::Context::default();
        run_paint_frame(&ctx, &ui, RawInput::default());
        let tab = RawInput {
            events: vec![key_press(egui::Key::Tab)],
            ..Default::default()
        };
        run_paint_frame(&ctx, &ui, tab);
        let enter = RawInput {
            events: vec![key_press(egui::Key::Enter)],
            ..Default::default()
        };
        assert_eq!(
            run_paint_frame(&ctx, &ui, enter),
            vec![Msg::Key("Enter".into())]
        );
    }

    #[test]
    fn a_declined_enter_still_clicks_the_focused_button() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Go").on_click(Msg::Clicked).on_keydown(|_| None);
        });
        let ctx = egui::Context::default();
        run_paint_frame(&ctx, &ui, RawInput::default());
        let tab = RawInput {
            events: vec![key_press(egui::Key::Tab)],
            ..Default::default()
        };
        run_paint_frame(&ctx, &ui, tab);
        let enter = RawInput {
            events: vec![key_press(egui::Key::Enter)],
            ..Default::default()
        };
        assert_eq!(run_paint_frame(&ctx, &ui, enter), vec![Msg::Clicked]);
    }

    #[test]
    fn a_press_on_a_node_dispatches_its_pointer_down_with_the_position() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.text("press me")
                .on_pointer_down(|p| Some(Msg::Down(p.button, p.x as i32)));
        });
        let ctx = egui::Context::default();
        let pos = EguiPos2::new(10.0, 10.0);
        run_paint_frame(&ctx, &ui, RawInput::default());
        let press = RawInput {
            events: vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(run_paint_frame(&ctx, &ui, press), vec![Msg::Down(0, 10)]);
    }
}
