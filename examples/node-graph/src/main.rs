//! Proof-of-concept node-graph / visual workflow builder demo.
//!
//! This intentionally bypasses `appfront_canvas::run_native`/`CanvasApp`: that
//! pipeline renders a generic `UITree<Msg>` through a strictly flexbox (taffy)
//! layout with no pan/zoom/absolute-transform primitive and no drag support,
//! so it can't express an infinite pannable node-graph canvas. This example
//! is a raw `egui`/`eframe` sibling demonstrating what such an editor would
//! need, not an extension of `appfront-canvas`.

use eframe::egui;
use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Vec2};

const NODE_SIZE: Vec2 = Vec2::new(140.0, 60.0);
const PORT_RADIUS: f32 = 6.0;
const BASE_FONT_SIZE: f32 = 14.0;
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 4.0;
const ZOOM_SENSITIVITY: f32 = 0.001;
const MIN_WIRE_OFFSET: f32 = 40.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct NodeId(u32);

struct Node {
    id: NodeId,
    label: String,
    /// World-space top-left position.
    pos: Pos2,
    size: Vec2,
}

struct Wire {
    from: NodeId,
    to: NodeId,
}

struct GraphState {
    nodes: Vec<Node>,
    wires: Vec<Wire>,
    next_id: u32,
}

impl GraphState {
    fn demo() -> Self {
        let mut state = GraphState {
            nodes: Vec::new(),
            wires: Vec::new(),
            next_id: 0,
        };
        let input = state.add_node("Input", Pos2::new(40.0, 200.0));
        let transform_a = state.add_node("Transform A", Pos2::new(280.0, 60.0));
        let transform_b = state.add_node("Transform B", Pos2::new(280.0, 340.0));
        let output = state.add_node("Output", Pos2::new(560.0, 200.0));

        state.wires.push(Wire {
            from: input,
            to: transform_a,
        });
        state.wires.push(Wire {
            from: input,
            to: transform_b,
        });
        state.wires.push(Wire {
            from: transform_a,
            to: output,
        });
        state
    }

    fn add_node(&mut self, label: &str, pos: Pos2) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        self.nodes.push(Node {
            id,
            label: label.to_string(),
            pos,
            size: NODE_SIZE,
        });
        id
    }

    fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
}

/// World-space <-> screen-space camera transform. Nothing in
/// `appfront-canvas`'s taffy layout offers this; it's hand-rolled here since
/// it's the core primitive an infinite pannable canvas needs.
struct Camera {
    pan: Vec2,
    zoom: f32,
}

impl Camera {
    fn new() -> Self {
        Camera {
            pan: Vec2::ZERO,
            zoom: 1.0,
        }
    }

    fn world_to_screen(&self, world: Pos2, viewport_origin: Pos2) -> Pos2 {
        viewport_origin + (world.to_vec2() + self.pan) * self.zoom
    }

    fn screen_to_world(&self, screen: Pos2, viewport_origin: Pos2) -> Pos2 {
        (((screen - viewport_origin) / self.zoom) - self.pan).to_pos2()
    }

    /// Zoom by `factor`, keeping the world point currently under
    /// `pivot_screen` fixed on screen.
    fn zoom_at(&mut self, pivot_screen: Pos2, viewport_origin: Pos2, factor: f32) {
        let world_before = self.screen_to_world(pivot_screen, viewport_origin);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let screen_after = self.world_to_screen(world_before, viewport_origin);
        // Nudge pan so world_before maps back onto pivot_screen post-zoom.
        self.pan += (pivot_screen - screen_after) / self.zoom;
    }
}

struct NodeGraphApp {
    graph: GraphState,
    camera: Camera,
    dragging_node: Option<NodeId>,
    pending_wire_from: Option<NodeId>,
}

impl NodeGraphApp {
    fn new() -> Self {
        NodeGraphApp {
            graph: GraphState::demo(),
            camera: Camera::new(),
            dragging_node: None,
            pending_wire_from: None,
        }
    }

    fn output_port_pos(&self, node: &Node, viewport_origin: Pos2) -> Pos2 {
        let rect = self.node_screen_rect(node, viewport_origin);
        Pos2::new(rect.right(), rect.center().y)
    }

    fn input_port_pos(&self, node: &Node, viewport_origin: Pos2) -> Pos2 {
        let rect = self.node_screen_rect(node, viewport_origin);
        Pos2::new(rect.left(), rect.center().y)
    }

    fn node_screen_rect(&self, node: &Node, viewport_origin: Pos2) -> Rect {
        let min = self.camera.world_to_screen(node.pos, viewport_origin);
        Rect::from_min_size(min, node.size * self.camera.zoom)
    }
}

impl eframe::App for NodeGraphApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            let (canvas_rect, canvas_response) =
                ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
            let viewport_origin = canvas_rect.min;

            // Pan: dragging empty canvas background, but only when no node
            // is currently being dragged (node-drag and canvas-pan must not
            // fight over the same pointer motion).
            if self.dragging_node.is_none() && canvas_response.dragged() {
                self.camera.pan += canvas_response.drag_delta() / self.camera.zoom;
            }

            // Zoom: scroll while hovering the canvas, keeping the point
            // under the cursor visually fixed.
            if let Some(hover_pos) = canvas_response.hover_pos() {
                let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll_y != 0.0 {
                    let factor = (1.0 + scroll_y * ZOOM_SENSITIVITY).clamp(0.5, 2.0);
                    self.camera.zoom_at(hover_pos, viewport_origin, factor);
                }
            }

            if canvas_response.clicked() {
                // Clicking empty background cancels a pending wire.
                self.pending_wire_from = None;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.pending_wire_from = None;
            }

            let painter = ui.painter_at(canvas_rect);

            // Pass 1: precompute screen rects.
            let screen_rects: Vec<Rect> = self
                .graph
                .nodes
                .iter()
                .map(|n| self.node_screen_rect(n, viewport_origin))
                .collect();

            // Pass 2: wires, drawn first so ports paint on top of them.
            for wire in &self.graph.wires {
                let (Some(from), Some(to)) =
                    (self.graph.node(wire.from), self.graph.node(wire.to))
                else {
                    continue;
                };
                let p_out = self.output_port_pos(from, viewport_origin);
                let p_in = self.input_port_pos(to, viewport_origin);
                paint_wire(&painter, p_out, p_in, self.camera.zoom);
            }
            if let Some(from_id) = self.pending_wire_from {
                if let Some(from) = self.graph.node(from_id) {
                    let p_out = self.output_port_pos(from, viewport_origin);
                    if let Some(cursor) = canvas_response.hover_pos() {
                        paint_wire(&painter, p_out, cursor, self.camera.zoom);
                    }
                }
            }

            // Pass 3: node boxes + interaction.
            for (idx, screen_rect) in screen_rects.iter().enumerate() {
                let node_id = self.graph.nodes[idx].id;
                let node_response = ui.interact(
                    *screen_rect,
                    Id::new(("node-graph-node", node_id.0)),
                    Sense::click_and_drag(),
                );

                if node_response.dragged() {
                    self.dragging_node = Some(node_id);
                    let delta = node_response.drag_delta() / self.camera.zoom;
                    if let Some(node) = self.graph.node_mut(node_id) {
                        node.pos += delta;
                    }
                }
                if node_response.drag_stopped() {
                    self.dragging_node = None;
                }

                let highlighted =
                    self.dragging_node == Some(node_id) || node_response.hovered();
                let fill = if highlighted {
                    Color32::from_rgb(70, 90, 120)
                } else {
                    Color32::from_rgb(50, 60, 80)
                };
                painter.rect_filled(*screen_rect, 6.0, fill);
                painter.rect_stroke(
                    *screen_rect,
                    6.0,
                    Stroke::new(1.5, Color32::from_rgb(180, 190, 210)),
                    egui::StrokeKind::Outside,
                );
                painter.text(
                    screen_rect.center(),
                    Align2::CENTER_CENTER,
                    &self.graph.nodes[idx].label,
                    egui::FontId::proportional(BASE_FONT_SIZE * self.camera.zoom),
                    Color32::WHITE,
                );

                // Port dots + click-based wire creation.
                let out_pos = Pos2::new(screen_rect.right(), screen_rect.center().y);
                let in_pos = Pos2::new(screen_rect.left(), screen_rect.center().y);

                let out_highlight = self.pending_wire_from == Some(node_id);
                painter.circle_filled(
                    out_pos,
                    PORT_RADIUS,
                    if out_highlight {
                        Color32::from_rgb(255, 210, 80)
                    } else {
                        Color32::from_rgb(120, 200, 255)
                    },
                );
                painter.circle_filled(in_pos, PORT_RADIUS, Color32::from_rgb(120, 255, 160));

                let out_port_rect =
                    Rect::from_center_size(out_pos, Vec2::splat(PORT_RADIUS * 3.0));
                let in_port_rect =
                    Rect::from_center_size(in_pos, Vec2::splat(PORT_RADIUS * 3.0));

                let out_response = ui.interact(
                    out_port_rect,
                    Id::new(("node-graph-out-port", node_id.0)),
                    Sense::click(),
                );
                if out_response.clicked() {
                    self.pending_wire_from = Some(node_id);
                }

                let in_response = ui.interact(
                    in_port_rect,
                    Id::new(("node-graph-in-port", node_id.0)),
                    Sense::click(),
                );
                if in_response.clicked() {
                    if let Some(from_id) = self.pending_wire_from {
                        if from_id != node_id {
                            self.graph.wires.push(Wire {
                                from: from_id,
                                to: node_id,
                            });
                        }
                        self.pending_wire_from = None;
                    }
                }
            }
        });
    }
}

fn paint_wire(painter: &egui::Painter, p_out: Pos2, p_in: Pos2, zoom: f32) {
    let horizontal_offset = ((p_in.x - p_out.x).abs() * 0.5).max(MIN_WIRE_OFFSET) * zoom.max(0.1);
    let c1 = p_out + Vec2::new(horizontal_offset, 0.0);
    let c2 = p_in - Vec2::new(horizontal_offset, 0.0);
    let stroke = Stroke::new(2.0, Color32::from_rgb(200, 200, 210));
    let bezier = egui::epaint::CubicBezierShape::from_points_stroke(
        [p_out, c1, c2, p_in],
        false,
        Color32::TRANSPARENT,
        stroke,
    );
    painter.add(bezier);
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Node Graph",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(NodeGraphApp::new()))),
    )
}
