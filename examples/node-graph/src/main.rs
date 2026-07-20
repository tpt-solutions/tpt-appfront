//! Raw `egui`/`eframe` escape-hatch example for TPT AppFront.
//!
//! See `README.md`: `tpt-appfront-canvas` renders a generic `UITree<Msg>`
//! through a strictly flexbox (taffy) layout and cannot express free-form
//! pan/zoom/absolute world-space transforms or drag-to-reposition. When your
//! UI needs exactly that (a node-graph editor, diagram, canvas), drop down to
//! raw `egui`/`eframe` like this rather than fighting the flexbox layout.
//!
//! Run with `cargo run` (native). Drag the background to pan, scroll to
//! zoom (cursor-anchored), drag a node body to move it, and drag from a
//! node's right port to another node's left port to create a wire.

use eframe::egui;

#[derive(Clone)]
struct Node {
    id: usize,
    label: String,
    pos: egui::Pos2,
}

#[derive(Clone)]
struct Wire {
    from: usize,
    to: usize,
}

struct NodeGraphApp {
    nodes: Vec<Node>,
    wires: Vec<Wire>,
    pan: egui::Vec2,
    zoom: f32,
    drag_node: Option<(usize, egui::Vec2)>,
    pan_drag: Option<egui::Pos2>,
    pending_wire: Option<usize>,
}

impl Default for NodeGraphApp {
    fn default() -> Self {
        let mut nodes = Vec::new();
        for (i, label) in ["Input", "Process", "Output"].iter().enumerate() {
            nodes.push(Node {
                id: i,
                label: label.to_string(),
                pos: egui::pos2(80.0 + i as f32 * 220.0, 160.0),
            });
        }
        Self {
            nodes,
            wires: vec![Wire { from: 0, to: 1 }, Wire { from: 1, to: 2 }],
            pan: egui::Vec2::ZERO,
            zoom: 1.0,
            drag_node: None,
            pan_drag: None,
            pending_wire: None,
        }
    }
}

impl NodeGraphApp {
    fn world_to_screen(&self, p: egui::Pos2) -> egui::Pos2 {
        (egui::vec2(p.x, p.y) * self.zoom + self.pan).to_pos2()
    }

    fn screen_to_world(&self, p: egui::Pos2) -> egui::Pos2 {
        ((egui::vec2(p.x, p.y) - self.pan) / self.zoom).to_pos2()
    }

    fn port_pos(&self, node: &Node, side: f32) -> egui::Pos2 {
        egui::pos2(node.pos.x + side * 90.0, node.pos.y + 40.0)
    }

    fn node_at_screen(&self, p: egui::Pos2) -> Option<usize> {
        let hit = self
            .nodes
            .iter()
            .find(|n| self.world_to_screen(n.pos).distance(p) < 60.0)
            .map(|n| n.id)?;
        Some(hit)
    }
}

impl eframe::App for NodeGraphApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            let screen_rect = ui.clip_rect();
            let painter = ui.painter();
            let pointer = ui.input(|i| i.pointer.interact_pos()).unwrap_or(screen_rect.min);
            let primary_down = ui.input(|i| i.pointer.primary_down());
            let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
            let released = ui.input(|i| i.pointer.any_released());
            let scroll = ui.input(|i| i.smooth_scroll_delta().y);

            // Zoom toward the cursor on scroll.
            if scroll != 0.0 {
                let world_before = self.screen_to_world(pointer);
                let factor = (1.0_f32 + scroll * 0.001).clamp(0.4, 3.0);
                self.zoom *= factor;
                let world_after = self.screen_to_world(pointer);
                self.pan += (world_after - world_before) * self.zoom;
            }

            // Begin a drag: node body (if hovered), else background pan.
            if primary_pressed && self.drag_node.is_none() && self.pan_drag.is_none() {
                if let Some(id) = self.node_at_screen(pointer) {
                    let node = self.nodes.iter().find(|n| n.id == id).unwrap();
                    let world = self.screen_to_world(pointer);
                    self.drag_node = Some((id, world - node.pos));
                } else {
                    self.pan_drag = Some(pointer - self.pan);
                }
            }

            if let Some(start) = self.pan_drag {
                if released || !primary_down {
                    self.pan_drag = None;
                } else {
                    self.pan = pointer - start;
                }
            }

            if let Some((id, offset)) = self.drag_node {
                if released {
                    self.drag_node = None;
                } else {
                    let world = self.screen_to_world(pointer) - offset;
                    for node in &mut self.nodes {
                        if node.id == id {
                            node.pos = world;
                        }
                    }
                }
            }

            // Wires are drawn first, beneath the nodes.
            for wire in &self.wires {
                if let (Some(a), Some(b)) = (
                    self.nodes.iter().find(|n| n.id == wire.from),
                    self.nodes.iter().find(|n| n.id == wire.to),
                ) {
                    let p1 = self.world_to_screen(self.port_pos(a, 1.0));
                    let p2 = self.world_to_screen(self.port_pos(b, -1.0));
                    painter.line_segment(
                        [p1, p2],
                        egui::Stroke::new(2.0, egui::Color32::from_gray(120)),
                    );
                }
            }

            // Finish a pending wire on release over a target node.
            if released {
                if let Some(from) = self.pending_wire.take() {
                    if let Some(target) = self.node_at_screen(pointer) {
                        if target != from {
                            self.wires.push(Wire { from, to: target });
                        }
                    }
                }
            }

            // Draw nodes; start a wire from the right port when pressed there.
            for node in &self.nodes {
                let screen_pos = self.world_to_screen(node.pos);
                let size = egui::vec2(180.0 * self.zoom, 80.0 * self.zoom);
                let r = egui::Rect::from_min_size(screen_pos, size);
                let right_port = self.world_to_screen(self.port_pos(node, 1.0));
                let left_port = self.world_to_screen(self.port_pos(node, -1.0));

                if primary_pressed && right_port.distance(pointer) < 10.0 {
                    self.pending_wire = Some(node.id);
                }

                let fill = if r.contains(pointer) {
                    egui::Color32::from_rgb(210, 225, 245)
                } else {
                    egui::Color32::from_rgb(235, 238, 245)
                };
                painter.rect_filled(r, 6.0 * self.zoom, fill);
                painter.rect_stroke(
                    r,
                    6.0 * self.zoom,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(110)),
                    egui::StrokeKind::Middle,
                );
                painter.text(
                    r.min + egui::vec2(10.0 * self.zoom, 30.0 * self.zoom),
                    egui::Align2::LEFT_CENTER,
                    &node.label,
                    egui::FontId::proportional(16.0 * self.zoom),
                    egui::Color32::BLACK,
                );
                painter.circle_filled(right_port, 5.0, egui::Color32::from_rgb(60, 120, 220));
                painter.circle_filled(left_port, 5.0, egui::Color32::from_rgb(60, 120, 220));
            }

            // The in-progress wire trails the cursor.
            if let Some(from) = self.pending_wire {
                if let Some(node) = self.nodes.iter().find(|n| n.id == from) {
                    let p1 = self.world_to_screen(self.port_pos(node, 1.0));
                    painter.line_segment(
                        [p1, pointer],
                        egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 120, 220)),
                    );
                }
            }

            ui.label(format!(
                "nodes: {}  wires: {}  zoom: {:.2}x",
                self.nodes.len(),
                self.wires.len(),
                self.zoom
            ));
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "TPT AppFront — node-graph (egui escape hatch)",
        options,
        Box::new(|_cc| Ok(Box::new(NodeGraphApp::default()))),
    )
}
