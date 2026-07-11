//! Hardware-accelerated canvas backend: `UITree` → `egui` widgets, laid
//! out with `taffy`, drawn via `wgpu`/`winit` through `eframe` (see
//! `spec.txt` section 3.2 and `todo.md` Phase 4). Runs unmodified on
//! desktop (native) and in the browser (WASM canvas).

mod app;
mod auto_optimizer;
mod layout;
mod paint;
mod text;

pub use app::CanvasApp;
pub use auto_optimizer::AutoOptimizer;
pub use text::TextMeasurer;

use appfront_core::UITree;

/// Opens a native window and runs `build_ui` every frame, dispatching any
/// clicked `on_click` message through `dispatch`.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_native<Msg: Clone + 'static>(
    title: &str,
    build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> eframe::Result<()> {
    let app = CanvasApp::new(build_ui, dispatch);
    eframe::run_native(
        title,
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(app))),
    )
}

/// Mounts the canvas app onto an existing `<canvas id="...">` element in
/// the page. Must be awaited from a `wasm_bindgen(start)` entry point (see
/// `examples/counter-canvas`'s wasm target, once wired up).
#[cfg(target_arch = "wasm32")]
pub async fn run_web<Msg: Clone + 'static>(
    canvas_id: &str,
    build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;

    let app = CanvasApp::new(build_ui, dispatch);
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");
    let canvas = document
        .get_element_by_id(canvas_id)
        .expect("canvas element not found")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("element is not a canvas");

    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|_cc| Ok(Box::new(app))),
        )
        .await
}
