//! Builds a `taffy` layout tree from a `UITree` (see `spec.txt`: "Shared
//! layout via `taffy`, not GPU compute shaders, for v1"). Layout is kept
//! entirely separate from painting: this module only decides *where*
//! things go; `paint.rs` decides *how* they look.

use crate::text::TextMeasurer;
use appfront_core::{NodeKind, UITree};
use taffy::prelude::*;
use taffy::TaffyTree;

pub const BUTTON_PAD_X: f32 = 12.0;
pub const BUTTON_PAD_Y: f32 = 8.0;
pub const INPUT_MIN_WIDTH: f32 = 160.0;
pub const INPUT_HEIGHT: f32 = 28.0;
pub const CONTAINER_GAP: f32 = 6.0;
pub const CONTAINER_PADDING: f32 = 4.0;
pub const CELL_PADDING: f32 = 6.0;

/// Font size used for a `Heading` at the given level; clamped so deeply
/// nested/invalid levels stay readable.
pub fn heading_font_size(level: u8) -> f32 {
    (28.0 - (level.saturating_sub(1) as f32) * 3.0).max(14.0)
}

pub const TEXT_FONT_SIZE: f32 = 16.0;

/// Mirrors the shape of a `UITree`, pairing each node with the `taffy`
/// node id that holds its computed layout. Built once per frame alongside
/// the `taffy` tree, then walked by `paint.rs` together with
/// `TaffyTree::layout`.
pub struct RenderNode<'a, Msg> {
    pub taffy_id: NodeId,
    pub ui: &'a UITree<Msg>,
    pub children: Vec<RenderNode<'a, Msg>>,
    /// Set only for `DataGrid` nodes: the `taffy` node id of every cell,
    /// laid out as `[header_row, data_row_0, data_row_1, ...]` so
    /// `paint.rs` can look up each cell's computed rect individually.
    pub grid_cells: Option<Vec<Vec<NodeId>>>,
}

/// Builds the `taffy` tree for `ui` and returns its root `RenderNode`.
/// Call `tree.compute_layout(root.taffy_id, ...)` next.
pub fn build<'a, Msg>(
    tree: &mut TaffyTree<()>,
    measurer: &mut TextMeasurer,
    ui: &'a UITree<Msg>,
) -> RenderNode<'a, Msg> {
    match &ui.kind {
        NodeKind::Container { children } => {
            build_flex_container(tree, measurer, ui, children, FlexDirection::Column)
        }
        NodeKind::List { items } => {
            build_flex_container(tree, measurer, ui, items, FlexDirection::Column)
        }
        NodeKind::Heading { text, level } => {
            build_text_leaf(tree, measurer, ui, text, heading_font_size(*level), 0.0, 0.0)
        }
        NodeKind::Text { text } => build_text_leaf(tree, measurer, ui, text, TEXT_FONT_SIZE, 0.0, 0.0),
        NodeKind::Button { label } => build_text_leaf(
            tree,
            measurer,
            ui,
            label,
            TEXT_FONT_SIZE,
            BUTTON_PAD_X * 2.0,
            BUTTON_PAD_Y * 2.0,
        ),
        NodeKind::Input { value } => {
            let (w, _) = measurer.measure(value, TEXT_FONT_SIZE);
            let width = (w + 16.0).max(INPUT_MIN_WIDTH);
            let taffy_id = tree
                .new_leaf(Style {
                    size: Size {
                        width: length(width),
                        height: length(INPUT_HEIGHT),
                    },
                    ..Default::default()
                })
                .expect("taffy leaf");
            RenderNode {
                taffy_id,
                ui,
                children: Vec::new(),
                grid_cells: None,
            }
        }
        NodeKind::DataGrid { columns, rows } => build_data_grid(tree, measurer, ui, columns, rows),
    }
}

fn build_flex_container<'a, Msg>(
    tree: &mut TaffyTree<()>,
    measurer: &mut TextMeasurer,
    ui: &'a UITree<Msg>,
    children: &'a [UITree<Msg>],
    direction: FlexDirection,
) -> RenderNode<'a, Msg> {
    let child_nodes: Vec<RenderNode<'a, Msg>> =
        children.iter().map(|c| build(tree, measurer, c)).collect();
    let child_ids: Vec<NodeId> = child_nodes.iter().map(|c| c.taffy_id).collect();

    let taffy_id = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: direction,
                gap: Size {
                    width: length(CONTAINER_GAP),
                    height: length(CONTAINER_GAP),
                },
                padding: Rect {
                    left: length(CONTAINER_PADDING),
                    right: length(CONTAINER_PADDING),
                    top: length(CONTAINER_PADDING),
                    bottom: length(CONTAINER_PADDING),
                },
                size: Size {
                    width: percent(1.0_f32),
                    height: auto(),
                },
                ..Default::default()
            },
            &child_ids,
        )
        .expect("taffy container");

    RenderNode {
        taffy_id,
        ui,
        children: child_nodes,
        grid_cells: None,
    }
}

fn build_text_leaf<'a, Msg>(
    tree: &mut TaffyTree<()>,
    measurer: &mut TextMeasurer,
    ui: &'a UITree<Msg>,
    text: &str,
    font_size: f32,
    extra_w: f32,
    extra_h: f32,
) -> RenderNode<'a, Msg> {
    let (w, h) = measurer.measure(text, font_size);
    let taffy_id = tree
        .new_leaf(Style {
            size: Size {
                width: length(w + extra_w),
                height: length(h + extra_h),
            },
            ..Default::default()
        })
        .expect("taffy leaf");
    RenderNode {
        taffy_id,
        ui,
        children: Vec::new(),
        grid_cells: None,
    }
}

/// Renders a `DataGrid` as a column of flex rows (header + data rows),
/// with every cell in a column sized to that column's widest cell. `taffy`
/// has a native CSS Grid mode, but this flex-of-flex-rows approach reuses
/// the same leaf-measuring code path and is enough for v1.
fn build_data_grid<'a, Msg>(
    tree: &mut TaffyTree<()>,
    measurer: &mut TextMeasurer,
    ui: &'a UITree<Msg>,
    columns: &[String],
    rows: &[Vec<String>],
) -> RenderNode<'a, Msg> {
    let mut col_widths: Vec<f32> = columns
        .iter()
        .map(|c| measurer.measure(c, TEXT_FONT_SIZE).0)
        .collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            let w = measurer.measure(cell, TEXT_FONT_SIZE).0;
            if let Some(slot) = col_widths.get_mut(i) {
                *slot = slot.max(w);
            }
        }
    }

    let mut row_ids: Vec<NodeId> = Vec::with_capacity(rows.len() + 1);
    let mut grid_cells: Vec<Vec<NodeId>> = Vec::with_capacity(rows.len() + 1);

    let (header_row_id, header_cells) = build_grid_row(tree, columns, &col_widths);
    row_ids.push(header_row_id);
    grid_cells.push(header_cells);
    for row in rows {
        let (row_id, cells) = build_grid_row(tree, row, &col_widths);
        row_ids.push(row_id);
        grid_cells.push(cells);
    }

    let taffy_id = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            &row_ids,
        )
        .expect("taffy grid");

    RenderNode {
        taffy_id,
        ui,
        children: Vec::new(),
        grid_cells: Some(grid_cells),
    }
}

fn build_grid_row(
    tree: &mut TaffyTree<()>,
    cells: &[String],
    col_widths: &[f32],
) -> (NodeId, Vec<NodeId>) {
    let cell_ids: Vec<NodeId> = cells
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let width = col_widths.get(i).copied().unwrap_or(0.0) + CELL_PADDING * 2.0;
            tree.new_leaf(Style {
                size: Size {
                    width: length(width),
                    height: length(TEXT_FONT_SIZE * 1.2 + CELL_PADDING),
                },
                ..Default::default()
            })
            .expect("taffy cell")
        })
        .collect();

    let row_id = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                ..Default::default()
            },
            &cell_ids,
        )
        .expect("taffy row");
    (row_id, cell_ids)
}
