//! Canvas backend: `UITree` → taffy layout → software painter, shown in a
//! native window through the internal, Rust-only window layer (see
//! [`platform`]). No `egui`, `eframe`, or `winit`, so the crate stays within
//! the license policy in `deny.toml`.
//!
//! Pieces: `layout` (taffy), `scene` (painting and input), [`raster`]
//! (the painter), [`glyphs`] (text from the bundled Noto Sans), `image`
//! (PNG, JPEG, GIF and WebP decoding), and [`platform`] (windowing).
//! [`platform`] and [`raster`] are public so custom drawing can drop below the
//! `UITree` model; see `examples/node-graph`.

mod auto_optimizer;
pub mod glyphs;
mod image;
mod layout;
pub mod platform;
pub mod raster;
mod scene;
mod text;

pub use auto_optimizer::AutoOptimizer;
pub use scene::Scene;
pub use text::TextMeasurer;
use tpt_appfront_core::UITree;

/// Builder for a canvas app. Use it when `NodeKind::Image` nodes need image bytes:
/// [`CanvasApp::image_loader`] supplies them. [`run_native`] and [`run_window`]
/// stay as shortcuts for apps without images.
///
/// ```no_run
/// # use tpt_appfront_core::UITree;
/// # fn build() -> UITree<()> { UITree::container(|_| {}) }
/// tpt_appfront_canvas::CanvasApp::new("Gallery")
///     .size(640, 480)
///     .image_loader(|src| std::fs::read(src).ok())
///     .run(build, |_| {})
///     .unwrap();
/// ```
pub struct CanvasApp {
    title: String,
    width: u32,
    height: u32,
    image_loader: Option<image::ImageLoader>,
}

impl CanvasApp {
    /// An app with an 800 x 600 window and no image loader.
    pub fn new(title: &str) -> Self {
        CanvasApp {
            title: title.to_owned(),
            width: 800,
            height: 600,
            image_loader: None,
        }
    }

    /// Sets the client area size in pixels.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Supplies the bytes for an image's `src`. The canvas never fetches anything
    /// itself: `load` maps a `src` to encoded PNG, JPEG, GIF or WebP bytes, or
    /// returns `None` to show the caption. Each `src` is loaded and decoded once,
    /// and the result is kept for the life of the app.
    pub fn image_loader(mut self, load: impl Fn(&str) -> Option<Vec<u8>> + 'static) -> Self {
        self.image_loader = Some(Box::new(load));
        self
    }

    fn scene(&mut self) -> Scene {
        match self.image_loader.take() {
            Some(load) => Scene::with_image_loader(load),
            None => Scene::new(),
        }
    }

    /// Opens the window and runs `build_ui` every frame, dispatching messages as
    /// [`run_window`] does. Blocks until the window closes.
    pub fn run<Msg: Clone + 'static>(
        mut self,
        build_ui: impl FnMut() -> UITree<Msg> + 'static,
        dispatch: impl Fn(Msg) + 'static,
    ) -> Result<(), platform::PlatformError> {
        let scene = self.scene();
        run_scene(
            &self.title,
            self.width,
            self.height,
            scene,
            build_ui,
            dispatch,
        )
    }

    /// Mounts the app onto an existing `<canvas id="...">`, as [`run_web`] does.
    #[cfg(target_arch = "wasm32")]
    pub async fn run_web<Msg: Clone + 'static>(
        mut self,
        canvas_id: &str,
        build_ui: impl FnMut() -> UITree<Msg> + 'static,
        dispatch: impl Fn(Msg) + 'static,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let scene = self.scene();
        run_web_scene(canvas_id, scene, build_ui, dispatch).await
    }
}

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
    build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> Result<(), platform::PlatformError> {
    run_scene(title, width, height, Scene::new(), build_ui, dispatch)
}

fn run_scene<Msg: Clone + 'static>(
    title: &str,
    width: u32,
    height: u32,
    mut scene: Scene,
    mut build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> Result<(), platform::PlatformError> {
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
    build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> Result<(), wasm_bindgen::JsValue> {
    run_web_scene(canvas_id, Scene::new(), build_ui, dispatch).await
}

#[cfg(target_arch = "wasm32")]
async fn run_web_scene<Msg: Clone + 'static>(
    canvas_id: &str,
    mut scene: Scene,
    mut build_ui: impl FnMut() -> UITree<Msg> + 'static,
    dispatch: impl Fn(Msg) + 'static,
) -> Result<(), wasm_bindgen::JsValue> {
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
