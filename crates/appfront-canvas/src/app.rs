//! The `eframe::App` that drives one canvas window: each frame it rebuilds
//! the `UITree` (immediate-mode, matching `egui`'s own paradigm), lays it
//! out with `taffy`, and paints it (see `layout.rs` / `paint.rs`).

use crate::text::TextMeasurer;
use crate::{layout, paint};
use appfront_core::UITree;
use std::rc::Rc;
use taffy::TaffyTree;

pub struct CanvasApp<Msg: Clone + 'static> {
    build_ui: Box<dyn FnMut() -> UITree<Msg>>,
    dispatch: Rc<dyn Fn(Msg)>,
    measurer: TextMeasurer,
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
        }
    }
}

impl<Msg: Clone + 'static> eframe::App for CanvasApp<Msg> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ui_tree = (self.build_ui)();

        let mut tree: TaffyTree<()> = TaffyTree::new();
        let root = layout::build(&mut tree, &mut self.measurer, &ui_tree);

        let available = ui.available_size();
        tree.compute_layout(
            root.taffy_id,
            taffy::Size {
                width: taffy::AvailableSpace::Definite(available.x),
                height: taffy::AvailableSpace::Definite(available.y),
            },
        )
        .expect("taffy compute_layout");

        let origin = ui.min_rect().min;
        let mut id_seed = 0u64;
        paint::paint(ui, &tree, &root, origin, &self.dispatch, &mut id_seed);

        let root_layout = tree.layout(root.taffy_id).expect("root layout");
        ui.allocate_space(egui::vec2(root_layout.size.width, root_layout.size.height));
    }

    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut *self)
    }
}
