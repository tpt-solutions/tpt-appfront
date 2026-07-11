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
            let response = ui.put(rect, egui::Button::new(label.clone()));
            clicked = response.clicked();
            #[cfg(feature = "accesskit")]
            {
                response.set_accessible_name(label.clone());
                if let Some(desc) = &node.ui.meta.ai.description {
                    response.set_accessible_description(desc.clone());
                }
            }
        }
        NodeKind::Input { value } => {
            let mut value = value.clone();
            let response = ui.put(rect, egui::TextEdit::singleline(&mut value));
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
    response.set_accessible_name(name.to_string());
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
