//! Canvas backend: `UITree` → taffy layout → software painter, shown in a
//! native window through the internal, Rust-only window layer (see
//! [`platform`]). No `egui`, `eframe`, or `winit`, so the crate stays within
//! the license policy in `deny.toml`.
//!
//! Pieces: [`layout`] (taffy), [`scene`] (painting and input), [`raster`]
//! (the painter), [`glyphs`] (text from the bundled Noto Sans), and
//! [`platform`] (windowing). [`platform`] and [`raster`] are public so custom
//! drawing can drop below the `UITree` model; see `examples/node-graph`.

mod auto_optimizer;
pub mod glyphs;
mod layout;
pub mod platform;
pub mod raster;
mod scene;
mod text;

pub use auto_optimizer::AutoOptimizer;
pub use scene::Scene;
pub use text::TextMeasurer;
use tpt_appfront_core::UITree;

/// Opens an 800 x 600 native window and runs `build_ui` every frame,
/// dispatching any message raised by a click, keypress, or edit through
/// `dispatch`. See [`run_window`] for a custom size.
///
/// Blocks until the window is closed. Only Windows has a window backend so
/// far; other targets return [`platform::PlatformError::Unsupported`].
pub fn run_native<Msg: Clone + 'static>(
    title: &str,
    build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> Result<(), platform::PlatformError> {
    run_window(title, 800, 600, build_ui, dispatch)
}

/// Like [`run_native`], with a client area of `width` x `height` pixels.
pub fn run_window<Msg: Clone + 'static>(
    title: &str,
    width: u32,
    height: u32,
    mut build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> Result<(), platform::PlatformError> {
    let mut scene = Scene::new();
    platform::run(title, width, height, move |frame| {
        let mut canvas = raster::Canvas::new(
            &mut *frame.pixels,
            frame.width as usize,
            frame.height as usize,
        );
        let closing = frame
            .events
            .iter()
            .any(|e| matches!(e, platform::Event::CloseRequested));
        let ui = build_ui();
        scene.frame(&mut canvas, &ui, frame.events, &dispatch);
        if closing {
            platform::ControlFlow::Exit
        } else {
            platform::ControlFlow::Continue
        }
    })
}

/// Mounts the canvas app onto an existing `<canvas id="...">` element and
/// starts its animation-frame loop. Returns once the loop is scheduled; the
/// app keeps running for the life of the page.
///
/// Input (mouse, keyboard, wheel) comes from DOM events on the canvas, which
/// is made focusable. Fails with an error when there is no window or document,
/// when no element has `canvas_id`, or when that element is not a `<canvas>`.
#[cfg(target_arch = "wasm32")]
pub async fn run_web<Msg: Clone + 'static>(
    canvas_id: &str,
    mut build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> Result<(), wasm_bindgen::JsValue> {
    let mut scene = Scene::new();
    platform::web::start(canvas_id, move |frame| {
        let mut canvas = raster::Canvas::new(
            &mut *frame.pixels,
            frame.width as usize,
            frame.height as usize,
        );
        let ui = build_ui();
        scene.frame(&mut canvas, &ui, frame.events, &dispatch);
        platform::ControlFlow::Continue
    })
}
