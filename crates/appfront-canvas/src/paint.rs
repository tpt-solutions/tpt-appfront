//! Paints a `RenderNode` tree (see `layout.rs`) using `egui` widgets placed
//! at the exact rects `taffy` computed — the "`UITree` → egui widgets"
//! mapping from `spec.txt` (`Container`→plain area, `Button`→`Button`,
//! etc.), kept separate from the layout math itself.

use crate::layout::{self, RenderNode, CELL_PADDING, TEXT_FONT_SIZE};
use appfront_core::NodeKind;
use egui::{Align2, FontId, Pos2, Rect, Sense, Vec2};
use std::rc::Rc;
use taffy::TaffyTree;

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

    *id_seed += 1;
    let id = egui::Id::new(("appfront_canvas_node", *id_seed));

    let mut clicked = false;
    match &node.ui.kind {
        NodeKind::Container { .. } | NodeKind::List { .. } => {}
        NodeKind::Heading { text, level } => {
            paint_text(ui, pos, text, layout::heading_font_size(*level));
            #[cfg(feature = "accesskit")]
            name_accessible_node(ui, rect, id, text, Some(egui::accesskit::Role::Heading));
        }
        NodeKind::Text { text } => {
            paint_text(ui, pos, text, TEXT_FONT_SIZE);
            #[cfg(feature = "accesskit")]
            name_accessible_node(ui, rect, id, text, None);
        }
        NodeKind::Button { label } => {
            let response = ui.put(rect, egui::Button::new(label.as_str()));
            clicked = response.clicked();
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
            let _response = ui.put(rect, egui::TextEdit::singleline(&mut value));
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
                _response.set_accessible_name(name);
            }
        }
        NodeKind::DataGrid { columns, rows } => {
            paint_data_grid(ui, tree, node, pos, columns, rows);
        }
    }

    if !clicked && node.ui.meta.on_click.is_some() {
        let response = ui.interact(rect, id, Sense::click());
        clicked = response.clicked();
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

fn paint_text(ui: &egui::Ui, pos: Pos2, text: &str, font_size: f32) {
    ui.painter().text(
        pos,
        Align2::LEFT_TOP,
        text,
        FontId::proportional(font_size),
        ui.visuals().text_color(),
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
    let row_ids = tree
        .children(node.taffy_id)
        .expect("data grid rows");
    let grid_cells = node
        .grid_cells
        .as_ref()
        .expect("data grid built with grid_cells");

    for (row_idx, row_id) in row_ids.iter().enumerate() {
        let row_layout = tree.layout(*row_id).expect("row layout");
        let row_origin = grid_origin + Vec2::new(row_layout.location.x, row_layout.location.y);
        let cell_ids = &grid_cells[row_idx];
        let texts: &[String] = if row_idx == 0 {
            columns
        } else {
            &rows[row_idx - 1]
        };

        for (cell_idx, cell_id) in cell_ids.iter().enumerate() {
            let Some(text) = texts.get(cell_idx) else {
                continue;
            };
            let cell_layout = tree.layout(*cell_id).expect("cell layout");
            let cell_pos = row_origin
                + Vec2::new(cell_layout.location.x, cell_layout.location.y)
                + Vec2::new(CELL_PADDING, CELL_PADDING / 2.0);
            paint_text(ui, cell_pos, text, TEXT_FONT_SIZE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextMeasurer;
    use appfront_core::UITree;
    use egui::{Event, PointerButton, Pos2 as EguiPos2, RawInput};
    use std::cell::RefCell;

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Clicked,
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
}
