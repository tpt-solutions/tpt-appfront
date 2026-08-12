//! The `eframe::App` that drives one canvas window: each frame it rebuilds
//! the `UITree` (immediate-mode, matching `egui`'s own paradigm), lays it
//! out with `taffy`, and paints it (see `layout.rs` / `paint.rs`).

use std::rc::Rc;

use taffy::TaffyTree;
use tpt_appfront_core::{apply_auto_virtual_scroll, UITree};

#[cfg(test)]
use tpt_appfront_core::{NodeKind, VirtualScroll};

use crate::auto_optimizer::{AutoOptimizer, OptimizerState};
use crate::text::TextMeasurer;
use crate::{layout, paint};

pub struct CanvasApp<Msg: Clone + 'static> {
    build_ui: Box<dyn FnMut() -> UITree<Msg>>,
    dispatch: Rc<dyn Fn(Msg)>,
    measurer: TextMeasurer,
    /// Runtime frame-time profiler (todo.md Phase 11 stretch). Records the
    /// per-frame work duration each `ui()` and exposes recommended
    /// optimizations; reading its `recommendations()` from app code lets a
    /// canvas app auto-toggle virtual scrolling / texture caching.
    optimizer: AutoOptimizer,
    /// When enabled, once `optimizer` recommends it, large unconfigured
    /// `List`/`DataGrid` nodes are auto-windowed via `VirtualScroll` (the
    /// "applied" half of `AutoOptimizer` for the canvas backend).
    auto_optimize_enabled: bool,
}

impl<Msg: Clone + 'static> CanvasApp<Msg> {
    pub fn new(
        build_ui: impl FnMut() -> UITree<Msg> + 'static,
        dispatch: impl Fn(Msg) + 'static,
    ) -> Self {
        CanvasApp {
            build_ui: Box::new(build_ui),
            dispatch: Rc::new(dispatch),
            measurer: TextMeasurer::new(),
            optimizer: AutoOptimizer::default(),
            auto_optimize_enabled: false,
        }
    }

    /// Enables automatic application of `AutoOptimizer`'s `virtual_scrolling`
    /// recommendation: once the frame profiler decides virtual scrolling would
    /// help, large `List`/`DataGrid` nodes that don't already configure it get
    /// windowed automatically (todo.md cross-cutting follow-through).
    pub fn auto_optimize(mut self, enabled: bool) -> Self {
        self.auto_optimize_enabled = enabled;
        self
    }

    /// Smoothed per-frame work duration in milliseconds (for an FPS overlay).
    pub fn frame_ms(&self) -> f64 {
        self.optimizer.smoothed_ms()
    }

    /// Current auto-tuned optimization recommendations.
    pub fn optimizer_state(&self) -> OptimizerState {
        self.optimizer.recommendations()
    }
}

impl<Msg: Clone + 'static> eframe::App for CanvasApp<Msg> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let start = std::time::Instant::now();

        let available = ui.available_size();

        let mut ui_tree = (self.build_ui)();

        // When auto-optimize is on and the profiler has recommended virtual
        // scrolling, window any large unconfigured `List`/`DataGrid` node.
        if self.auto_optimize_enabled && self.optimizer.recommendations().virtual_scrolling {
            apply_auto_virtual_scroll(&mut ui_tree, available.y);
        }

        let mut tree: TaffyTree<()> = TaffyTree::new();
        let root = layout::build(&mut tree, &mut self.measurer, &ui_tree);

        // Degrade instead of crashing the whole window: a transient taffy
        // failure (e.g. zero/NaN available size while minimized) must not take
        // the desktop app down with it. Skip this frame's layout/paint and let
        // the next frame retry — the window stays alive and recovers on its own.
        if let Err(e) = tree.compute_layout(
            root.taffy_id,
            taffy::Size {
                width: taffy::AvailableSpace::Definite(available.x),
                height: taffy::AvailableSpace::Definite(available.y),
            },
        ) {
            eprintln!(
                "tpt-appfront-canvas: taffy compute_layout failed ({e:?}); skipping paint this frame"
            );
            self.optimizer
                .record_frame(start.elapsed().as_secs_f64() * 1000.0);
            return;
        }

        let origin = ui.min_rect().min;
        let mut id_seed = 0u64;
        paint::paint(ui, &tree, &root, origin, &self.dispatch, &mut id_seed);

        let root_layout = tree.layout(root.taffy_id).expect("root layout");
        ui.allocate_space(egui::vec2(root_layout.size.width, root_layout.size.height));

        // Feed the measured per-frame work into the profiler. `record_frame`
        // is a no-op cost (an EMA update) and safe to call unconditionally.
        self.optimizer
            .record_frame(start.elapsed().as_secs_f64() * 1000.0);
    }

    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut *self)
    }
}

#[cfg(test)]
mod tests {
    use tpt_appfront_core::ContainerBuilder;

    use super::*;

    fn big_list(n: usize) -> UITree<()> {
        let mut b = ContainerBuilder::new();
        b.list(|items| {
            for i in 0..n {
                items.text(format!("item {i}"));
            }
        });
        b.into_only_child().unwrap()
    }

    fn as_list(ui: &UITree<()>) -> &UITree<()> {
        match &ui.kind {
            NodeKind::List { .. } => ui,
            other => panic!("expected a List node, got {other:?}"),
        }
    }

    #[test]
    fn auto_virtual_scroll_windowizes_large_lists() {
        let mut ui = big_list(1000);
        apply_auto_virtual_scroll(&mut ui, 200.0);
        assert!(as_list(&ui).meta.virtual_scroll.is_some());
    }

    #[test]
    fn auto_virtual_scroll_skips_small_lists() {
        let mut ui = big_list(5);
        apply_auto_virtual_scroll(&mut ui, 200.0);
        assert!(as_list(&ui).meta.virtual_scroll.is_none());
    }

    #[test]
    fn auto_virtual_scroll_respects_existing_config() {
        let mut ui = big_list(1000);
        ui.meta.virtual_scroll = Some(VirtualScroll::new(10.0, 50.0));
        apply_auto_virtual_scroll(&mut ui, 200.0);
        // Not overwritten by the heuristic default.
        assert_eq!(ui.meta.virtual_scroll.unwrap().item_height, 10.0);
    }

    #[test]
    fn auto_virtual_scroll_does_not_clobber_meta() {
        // Applying virtual scroll must not drop other `NodeMeta` fields.
        let mut ui = big_list(1000);
        ui.meta.class = Some("big-list".to_string());
        apply_auto_virtual_scroll(&mut ui, 200.0);
        assert_eq!(as_list(&ui).meta.class.as_deref(), Some("big-list"));
        assert!(as_list(&ui).meta.virtual_scroll.is_some());
    }
}
