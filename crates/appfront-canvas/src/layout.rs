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
            // Virtual scrolling: when `meta.virtual_scroll` is set, build only
            // the windowed slice of items plus top/bottom spacers, instead of
            // every item — so a list with thousands of rows lays out and paints
            // only the handful in view (mirrors what `appfront-dom` already
            // does; canvas must not render every row every frame).
            if let Some(vs) = ui.meta.virtual_scroll {
                build_virtual_list(tree, measurer, ui, items, vs)
            } else {
                build_flex_container(tree, measurer, ui, items, FlexDirection::Column)
            }
        }
        NodeKind::Heading { text, level } => {
            let fs = (heading_font_size(*level) + canvas_style_for(&ui.meta.class).font_delta).max(8.0);
            build_text_leaf(tree, measurer, ui, text, fs, 0.0, 0.0)
        }
        NodeKind::Text { text } => {
            let fs = (TEXT_FONT_SIZE + canvas_style_for(&ui.meta.class).font_delta).max(8.0);
            build_text_leaf(tree, measurer, ui, text, fs, 0.0, 0.0)
        }
        NodeKind::Button { label } => {
            let fs = (TEXT_FONT_SIZE + canvas_style_for(&ui.meta.class).font_delta).max(8.0);
            build_text_leaf(
                tree,
                measurer,
                ui,
                label,
                fs,
                BUTTON_PAD_X * 2.0,
                BUTTON_PAD_Y * 2.0,
            )
        }
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
        NodeKind::Textarea { value } => {
            let (w, h) = measurer.measure(value, TEXT_FONT_SIZE);
            let width = (w + 16.0).max(INPUT_MIN_WIDTH);
            let height = (h + 16.0).max(INPUT_HEIGHT * 3.0);
            let taffy_id = tree
                .new_leaf(Style {
                    size: Size {
                        width: length(width),
                        height: length(height),
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
        NodeKind::Checkbox { label, .. } => {
            let (w, h) = measurer.measure(label, TEXT_FONT_SIZE);
            let taffy_id = tree
                .new_leaf(Style {
                    size: Size {
                        // +24 for the checkbox glyph + gap that `egui::Checkbox`
                        // draws before the label.
                        width: length(w + 24.0),
                        height: length(h.max(INPUT_HEIGHT)),
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
        NodeKind::Select { options, selected } => {
            let label = options
                .iter()
                .find(|(v, _)| v == selected)
                .map(|(_, l)| l.as_str())
                .unwrap_or(selected.as_str());
            let (w, _) = measurer.measure(label, TEXT_FONT_SIZE);
            let width = (w + 32.0).max(INPUT_MIN_WIDTH);
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
        NodeKind::Radio { options, .. } => {
            let text: String = options
                .iter()
                .map(|(_, l)| l.as_str())
                .collect::<Vec<_>>()
                .join("   ");
            let (w, h) = measurer.measure(&text, TEXT_FONT_SIZE);
            let taffy_id = tree
                .new_leaf(Style {
                    size: Size {
                        width: length(w + 16.0),
                        height: length(h.max(INPUT_HEIGHT)),
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
        // Canvas has no overlay layer; render the portal content inline as a
        // column flex container (its `target` is metadata for hosts that
        // collect portals via `UITree::collect_portals`).
        NodeKind::Portal { content, .. } => {
            build_flex_container(tree, measurer, ui, std::slice::from_ref(content), FlexDirection::Column)
        }
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

    // Honor utility-class padding (e.g. `p-4`) over the hardcoded default when
    // present; otherwise keep the conventional container padding.
    let style = canvas_style_for(&ui.meta.class);
    let pad = if style.padding.left != 0.0
        || style.padding.right != 0.0
        || style.padding.top != 0.0
        || style.padding.bottom != 0.0
    {
        style.padding
    } else {
        Edge {
            left: CONTAINER_PADDING,
            right: CONTAINER_PADDING,
            top: CONTAINER_PADDING,
            bottom: CONTAINER_PADDING,
        }
    };

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
                    left: length(pad.left),
                    right: length(pad.right),
                    top: length(pad.top),
                    bottom: length(pad.bottom),
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

/// A zero-width spacer leaf of a fixed pixel height, used to preserve the
/// scrollable area's total height when a `List` is windowed (so the scrollbar
/// still matches the full, unvirtualized item count).
fn build_spacer(tree: &mut TaffyTree<()>, height: f32) -> NodeId {
    tree.new_leaf(Style {
        size: Size {
            width: length(0.0),
            height: length(height),
        },
        ..Default::default()
    })
    .expect("taffy spacer")
}

/// Windowed `List` layout: renders only the items currently in view (plus
/// overscan) and prepends/appends spacer leaves sized to the skipped items, so
/// the overall list height — and thus scroll behavior — matches the full set.
fn build_virtual_list<'a, Msg>(
    tree: &mut TaffyTree<()>,
    measurer: &mut TextMeasurer,
    ui: &'a UITree<Msg>,
    items: &'a [UITree<Msg>],
    vs: appfront_core::VirtualScroll,
) -> RenderNode<'a, Msg> {
    let range = vs.visible_range(items.len());

    let mut child_nodes: Vec<RenderNode<'a, Msg>> = Vec::new();
    let mut child_ids: Vec<NodeId> = Vec::new();

    if range.top_spacer > 0.0 {
        let id = build_spacer(tree, range.top_spacer);
        child_ids.push(id);
        // No `UITree` to borrow for a spacer; reuse the list node's lifetime via
        // a zero-sized leaf referencing the list node (display only).
        child_nodes.push(RenderNode {
            taffy_id: id,
            ui,
            children: Vec::new(),
            grid_cells: None,
        });
    }

    for item in &items[range.start..range.end] {
        let rn = build(tree, measurer, item);
        child_ids.push(rn.taffy_id);
        child_nodes.push(rn);
    }

    if range.bottom_spacer > 0.0 {
        let id = build_spacer(tree, range.bottom_spacer);
        child_ids.push(id);
        child_nodes.push(RenderNode {
            taffy_id: id,
            ui,
            children: Vec::new(),
            grid_cells: None,
        });
    }

    let taffy_id = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
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

/// Styling adapter: a `CanvasStyle` derived from a node's `meta.class`
/// utility classes (see `appfront_core::styling`). Canvas can't honor every
/// CSS utility, but at minimum padding / font-size / color classes map to
/// `taffy` layout and `egui` paint so `class="..."` is not a silent no-op on
/// canvas (it is for DOM/HTML already). Unrecognized utilities are ignored.
#[derive(Debug, Clone, Default)]
pub struct CanvasStyle {
    /// Per-edge padding in px (replaces `CONTAINER_PADDING` when any padding
    /// utility is present).
    pub padding: Edge<f32>,
    /// Extra text pixel size delta applied on top of the base font size
    /// (from `text-xs`/`text-sm`/`text-lg`/`text-2xl`).
    pub font_delta: f32,
    /// Background color (from `bg-*`), if a utility matched.
    pub background: Option<egui::Color32>,
    /// Foreground/text color (from `text-white`/`text-gray-700`), if matched.
    pub foreground: Option<egui::Color32>,
}

/// Per-edge padding in px, defaults applied when absent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edge<T> {
    pub left: T,
    pub right: T,
    pub top: T,
    pub bottom: T,
}

/// Parses a node's `meta.class` into a [`CanvasStyle`], reading the utility
/// names recognized by `appfront_core::styling::UTILITIES`. Returns the
/// default (no-op) style when there is no class or none of the mapped utilities
/// are present.
pub fn canvas_style_for(class: &Option<String>) -> CanvasStyle {
    let Some(class) = class else {
        return CanvasStyle::default();
    };
    let mut style = CanvasStyle::default();
    let mut have_padding = false;
    let mut pad = Edge {
        left: 0.0,
        right: 0.0,
        top: 0.0,
        bottom: 0.0,
    };
    for util in class.split_whitespace() {
        // The DOM/HTML/SSR path prefixes utilities with `af-u-`; accept both
        // the bare name and the prefixed form so canvas honors the same class
        // strings regardless of which schema emitted them.
        let name = util.strip_prefix("af-u-").unwrap_or(util);
        match name {
            "p-0" => { pad = Edge { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 }; have_padding = true; }
            "p-1" => { pad = uniform(0.25); have_padding = true; }
            "p-2" => { pad = uniform(0.5); have_padding = true; }
            "p-4" => { pad = uniform(1.0); have_padding = true; }
            "p-8" => { pad = uniform(2.0); have_padding = true; }
            "px-4" => { pad.left = 1.0; pad.right = 1.0; have_padding = true; }
            "py-2" => { pad.top = 0.5; pad.bottom = 0.5; have_padding = true; }
            "text-xs" => style.font_delta = -8.0,
            "text-sm" => style.font_delta = -4.0,
            "text-lg" => style.font_delta = 2.0,
            "text-2xl" => style.font_delta = 8.0,
            "text-white" => style.foreground = Some(egui::Color32::WHITE),
            "text-gray-700" => style.foreground = Some(egui::Color32::from_rgb(0x37, 0x41, 0x51)),
            "bg-blue-500" => style.background = Some(egui::Color32::from_rgb(0x3b, 0x82, 0xf6)),
            "bg-gray-100" => style.background = Some(egui::Color32::from_rgb(0xf3, 0xf4, 0xf6)),
            "bg-white" => style.background = Some(egui::Color32::WHITE),
            _ => {}
        }
    }
    if have_padding {
        style.padding = pad;
    }
    style
}

fn uniform(rem: f32) -> Edge<f32> {
    let px = rem * 16.0;
    Edge { left: px, right: px, top: px, bottom: px }
}

#[cfg(test)]
mod tests {
    use super::*;
    use appfront_core::UITree;

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum Msg {
        Clicked,
    }

    #[test]
    fn heading_font_size_shrinks_with_level_and_clamps() {
        assert_eq!(heading_font_size(1), 28.0);
        assert!(heading_font_size(2) < heading_font_size(1));
        // Deeply nested/invalid levels stay readable (clamped at 14.0).
        assert_eq!(heading_font_size(10), 14.0);
        assert_eq!(heading_font_size(0), 28.0);
    }

    #[test]
    fn container_produces_flex_node_with_one_child_per_ui_child() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.text("one");
            c.text("two");
        });
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let mut measurer = TextMeasurer::new();
        let root = build(&mut tree, &mut measurer, &ui);
        assert_eq!(root.children.len(), 2);
        assert_eq!(tree.child_count(root.taffy_id), 2);
    }

    #[test]
    fn button_leaf_includes_padding_beyond_text_size() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.button("Go");
        });
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let mut measurer = TextMeasurer::new();
        let root = build(&mut tree, &mut measurer, &ui);
        let button_node = &root.children[0];
        let style = tree.style(button_node.taffy_id).unwrap();
        let (text_w, text_h) = measurer.measure("Go", TEXT_FONT_SIZE);
        let w = style.size.width.value();
        let h = style.size.height.value();
        assert_eq!(w, text_w + BUTTON_PAD_X * 2.0);
        assert_eq!(h, text_h + BUTTON_PAD_Y * 2.0);
    }

    #[test]
    fn input_width_respects_minimum() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.input("x");
        });
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let mut measurer = TextMeasurer::new();
        let root = build(&mut tree, &mut measurer, &ui);
        let input_node = &root.children[0];
        let style = tree.style(input_node.taffy_id).unwrap();
        let w = style.size.width.value();
        assert!(w >= INPUT_MIN_WIDTH);
    }

    #[test]
    fn data_grid_builds_one_row_per_data_row_plus_header() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.data_grid(["Name", "Age"], [["Alice", "30"], ["Bob", "25"]]);
        });
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let mut measurer = TextMeasurer::new();
        let root = build(&mut tree, &mut measurer, &ui);
        let grid_node = &root.children[0];
        let grid_cells = grid_node.grid_cells.as_ref().expect("data grid has cells");
        // header + 2 data rows
        assert_eq!(grid_cells.len(), 3);
        assert_eq!(grid_cells[0].len(), 2);
        assert_eq!(tree.child_count(grid_node.taffy_id), 3);
    }

    #[test]
    fn data_grid_column_width_matches_widest_cell() {
        let ui: UITree<Msg> = UITree::container(|c| {
            c.data_grid(["N"], [["short"], ["a much longer cell value"]]);
        });
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let mut measurer = TextMeasurer::new();
        let root = build(&mut tree, &mut measurer, &ui);
        let grid_node = &root.children[0];
        let grid_cells = grid_node.grid_cells.as_ref().unwrap();

        let header_cell_style = tree.style(grid_cells[0][0]).unwrap();
        let row2_cell_style = tree.style(grid_cells[2][0]).unwrap();
        // Every cell in a column shares the same (widest-cell) width.
        assert_eq!(header_cell_style.size.width, row2_cell_style.size.width);
    }

    #[test]
    fn virtual_list_windows_items_and_adds_spacers() {
        // 1000 items, item_height ~ 20px => only a small window built.
        let vs = appfront_core::VirtualScroll::new(20.0, 200.0);
        let ui: UITree<Msg> = UITree::container(|c| {
            let b = c.list(|l| {
                for i in 0..1000 {
                    l.text(format!("item {i}"));
                }
            });
            b.virtual_scroll(vs);
        });
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let mut measurer = TextMeasurer::new();
        let root = build(&mut tree, &mut measurer, &ui);
        let list_node = &root.children[0];
        // Should be far fewer than 1000 children (window + 2 spacers).
        assert!(list_node.children.len() < 100, "virtual list must not build every item: got {}", list_node.children.len());
        assert!(list_node.children.len() >= 3, "windowed list should still have top-spacer, items, bottom-spacer");
        // A container without virtual_scroll builds all items.
        let plain: UITree<Msg> = UITree::container(|c| {
            c.list(|l| {
                for i in 0..1000 {
                    l.text(format!("item {i}"));
                }
            });
        });
        let mut tree2: TaffyTree<()> = TaffyTree::new();
        let root2 = build(&mut tree2, &mut measurer, &plain);
        assert_eq!(root2.children[0].children.len(), 1000);
    }

    #[test]
    fn canvas_style_honors_padding_and_color_utilities() {
        let s = canvas_style_for(&Some("p-4 bg-blue-500 text-white".to_string()));
        assert_eq!(s.padding.left, 16.0);
        assert_eq!(s.padding.top, 16.0);
        assert_eq!(s.background, Some(egui::Color32::from_rgb(0x3b, 0x82, 0xf6)));
        assert_eq!(s.foreground, Some(egui::Color32::WHITE));

        // The `af-u-` prefixed form emitted by SSR/DOM is accepted too.
        let s2 = canvas_style_for(&Some("af-u-p-2".to_string()));
        assert_eq!(s2.padding.left, 8.0);

        // No mapped utilities => default no-op style.
        let s3 = canvas_style_for(&Some("my-custom-class".to_string()));
        assert_eq!(s3.padding, Edge { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 });
        assert!(s3.background.is_none());
        assert!(s3.foreground.is_none());
    }

    #[test]
    fn canvas_style_font_delta_from_text_size_utility() {
        assert_eq!(canvas_style_for(&Some("text-2xl".to_string())).font_delta, 8.0);
        assert_eq!(canvas_style_for(&Some("text-sm".to_string())).font_delta, -4.0);
        assert_eq!(canvas_style_for(&None).font_delta, 0.0);
    }
}
