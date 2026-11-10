//! Raw escape-hatch example for TPT AppFront: a pannable, zoomable node graph.
//!
//! See `README.md`: `tpt-appfront-canvas` renders a generic `UITree<Msg>`
//! through a strictly flexbox (taffy) layout and cannot express free-form
//! pan/zoom/absolute world-space transforms or drag-to-reposition. When your
//! UI needs exactly that, drop below the `UITree` model: this example opens
//! the window with `platform::run`, gets the raw framebuffer each frame, and
//! paints it with `raster::Canvas` and `glyphs::Font`.
//!
//! Run with `cargo run` (native). Drag the background to pan, scroll to
//! zoom (cursor-anchored), drag a node body to move it, and drag from a
//! node's right port to another node's left port to create a wire.

use tpt_appfront_canvas::glyphs::Font;
use tpt_appfront_canvas::platform::{self, ControlFlow, Event, MouseButton};
use tpt_appfront_canvas::raster::{rgb, Canvas, Rect, Rgb};

const NODE_W: f32 = 180.0;
const NODE_H: f32 = 80.0;
const PORT_OFFSET_Y: f32 = 40.0;
const PORT_HIT: f32 = 10.0;
const NODE_HIT: f32 = 60.0;

const WIRE: Rgb = rgb(120, 120, 120);
const ACCENT: Rgb = rgb(60, 120, 220);
const NODE_FILL: Rgb = rgb(235, 238, 245);
const NODE_HOVER: Rgb = rgb(210, 225, 245);
const NODE_BORDER: Rgb = rgb(110, 110, 110);

#[derive(Clone)]
struct Node {
    id: usize,
    label: String,
    /// World-space position (before pan and zoom).
    pos: (f32, f32),
}

#[derive(Clone)]
struct Wire {
    from: usize,
    to: usize,
}

struct NodeGraph {
    nodes: Vec<Node>,
    wires: Vec<Wire>,
    pan: (f32, f32),
    zoom: f32,
    /// Node being dragged, with the grab offset in world space.
    drag_node: Option<(usize, (f32, f32))>,
    /// Background pan drag: the pointer offset from the pan at press time.
    pan_drag: Option<(f32, f32)>,
    /// Node whose right port a wire is being dragged from.
    pending_wire: Option<usize>,
    pointer: (f32, f32),
    font: Font,
}

impl Default for NodeGraph {
    fn default() -> Self {
        let nodes = ["Input", "Process", "Output"]
            .iter()
            .enumerate()
            .map(|(i, label)| Node {
                id: i,
                label: label.to_string(),
                pos: (80.0 + i as f32 * 220.0, 160.0),
            })
            .collect();
        Self {
            nodes,
            wires: vec![Wire { from: 0, to: 1 }, Wire { from: 1, to: 2 }],
            pan: (0.0, 0.0),
            zoom: 1.0,
            drag_node: None,
            pan_drag: None,
            pending_wire: None,
            pointer: (0.0, 0.0),
            font: Font::bundled(),
        }
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

impl NodeGraph {
    fn world_to_screen(&self, p: (f32, f32)) -> (f32, f32) {
        (p.0 * self.zoom + self.pan.0, p.1 * self.zoom + self.pan.1)
    }

    fn screen_to_world(&self, p: (f32, f32)) -> (f32, f32) {
        (
            (p.0 - self.pan.0) / self.zoom,
            (p.1 - self.pan.1) / self.zoom,
        )
    }

    /// `side` is `1.0` for the right port and `-1.0` for the left port. Ports
    /// sit on the node's edges, so wires leave and enter outside the body.
    fn port_pos(node: &Node, side: f32) -> (f32, f32) {
        let x = if side > 0.0 {
            node.pos.0 + NODE_W
        } else {
            node.pos.0
        };
        (x, node.pos.1 + PORT_OFFSET_Y)
    }

    fn node_at_screen(&self, p: (f32, f32)) -> Option<usize> {
        self.nodes
            .iter()
            .find(|n| dist(self.world_to_screen(n.pos), p) < NODE_HIT)
            .map(|n| n.id)
    }

    fn node_by_id(&self, id: usize) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Applies one input event to the graph's state.
    fn handle(&mut self, event: &Event) {
        match *event {
            Event::Wheel { x, y, delta } => {
                if delta != 0.0 {
                    let anchor = (x as f32, y as f32);
                    let world_before = self.screen_to_world(anchor);
                    let factor = (1.0 + delta * 0.1).clamp(0.4, 3.0);
                    self.zoom *= factor;
                    let world_after = self.screen_to_world(anchor);
                    self.pan.0 += (world_after.0 - world_before.0) * self.zoom;
                    self.pan.1 += (world_after.1 - world_before.1) * self.zoom;
                }
            }
            Event::PointerMoved { x, y } => {
                self.pointer = (x as f32, y as f32);
                if let Some(start) = self.pan_drag {
                    self.pan = (self.pointer.0 - start.0, self.pointer.1 - start.1);
                }
                if let Some((id, offset)) = self.drag_node {
                    let world = self.screen_to_world(self.pointer);
                    let target = (world.0 - offset.0, world.1 - offset.1);
                    for node in &mut self.nodes {
                        if node.id == id {
                            node.pos = target;
                        }
                    }
                }
            }
            Event::PointerDown {
                x,
                y,
                button: MouseButton::Primary,
            } => {
                self.pointer = (x as f32, y as f32);
                // A press on a right port starts a wire.
                for node in &self.nodes {
                    let port = self.world_to_screen(Self::port_pos(node, 1.0));
                    if dist(port, self.pointer) < PORT_HIT {
                        self.pending_wire = Some(node.id);
                    }
                }
                if self.drag_node.is_none() && self.pan_drag.is_none() {
                    if let Some(id) = self.node_at_screen(self.pointer) {
                        if let Some(node) = self.node_by_id(id) {
                            let world = self.screen_to_world(self.pointer);
                            let offset = (world.0 - node.pos.0, world.1 - node.pos.1);
                            self.drag_node = Some((id, offset));
                        }
                    } else {
                        self.pan_drag =
                            Some((self.pointer.0 - self.pan.0, self.pointer.1 - self.pan.1));
                    }
                }
            }
            Event::PointerUp {
                x,
                y,
                button: MouseButton::Primary,
            } => {
                self.pointer = (x as f32, y as f32);
                self.pan_drag = None;
                self.drag_node = None;
                if let Some(from) = self.pending_wire.take() {
                    if let Some(target) = self.node_at_screen(self.pointer) {
                        if target != from {
                            self.wires.push(Wire { from, to: target });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Paints the graph into the framebuffer.
    fn paint(&mut self, canvas: &mut Canvas<'_>) {
        // Wires are drawn first, beneath the nodes.
        for wire in &self.wires {
            if let (Some(a), Some(b)) = (self.node_by_id(wire.from), self.node_by_id(wire.to)) {
                let p1 = self.world_to_screen(Self::port_pos(a, 1.0));
                let p2 = self.world_to_screen(Self::port_pos(b, -1.0));
                canvas.draw_line(p1.0, p1.1, p2.0, p2.1, 2.0, WIRE);
            }
        }

        for node in &self.nodes {
            let top_left = self.world_to_screen(node.pos);
            let rect = Rect::new(
                top_left.0.round() as i32,
                top_left.1.round() as i32,
                (NODE_W * self.zoom).round() as i32,
                (NODE_H * self.zoom).round() as i32,
            );
            let hovered = self.node_at_screen(self.pointer) == Some(node.id)
                || rect_contains(rect, self.pointer);
            canvas.fill_rect(rect, if hovered { NODE_HOVER } else { NODE_FILL });
            canvas.stroke_rect(rect, NODE_BORDER, 1);

            let size = 16.0 * self.zoom;
            canvas.draw_text(
                &mut self.font,
                rect.x as f32 + 10.0 * self.zoom,
                rect.y as f32 + 30.0 * self.zoom + size * 0.35,
                &node.label,
                size,
                rgb(0, 0, 0),
            );

            for side in [1.0, -1.0] {
                let port = self.world_to_screen(Self::port_pos(node, side));
                canvas.fill_circle(port.0, port.1, 5.0, ACCENT);
            }
        }

        // The in-progress wire trails the pointer.
        if let Some(from) = self.pending_wire {
            if let Some(node) = self.node_by_id(from) {
                let p1 = self.world_to_screen(Self::port_pos(node, 1.0));
                canvas.draw_line(p1.0, p1.1, self.pointer.0, self.pointer.1, 2.0, ACCENT);
            }
        }

        let status = format!(
            "nodes: {}  wires: {}  zoom: {:.2}x",
            self.nodes.len(),
            self.wires.len(),
            self.zoom
        );
        canvas.draw_text(
            &mut self.font,
            8.0,
            20.0,
            &status,
            16.0,
            rgb(0x37, 0x41, 0x51),
        );
    }
}

fn rect_contains(rect: Rect, p: (f32, f32)) -> bool {
    let (x, y) = (p.0 as i32, p.1 as i32);
    x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h
}

fn main() {
    let mut graph = NodeGraph::default();
    let result = platform::run(
        "TPT AppFront — node-graph (raw framebuffer escape hatch)",
        900,
        560,
        move |frame| {
            let mut closing = false;
            for event in frame.events {
                if matches!(event, Event::CloseRequested) {
                    closing = true;
                }
                graph.handle(event);
            }
            let mut canvas = Canvas::new(
                &mut *frame.pixels,
                frame.width as usize,
                frame.height as usize,
            );
            graph.paint(&mut canvas);
            if closing {
                ControlFlow::Exit
            } else {
                ControlFlow::Continue
            }
        },
    );
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
