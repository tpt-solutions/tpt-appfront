//! Text measurement for layout sizing (see `spec.txt`'s canvas backend:
//! `cosmic-text` for shaping). Layout only needs the shaped extent of each
//! text run, not glyph painting — `egui`'s own text painter draws the
//! characters once `taffy` has decided how much space to give them.
//!
//! On native targets, `cosmic-text` performs real shaping against system
//! fonts. On `wasm32`, loading system fonts through `cosmic-text`/`fontdb`
//! is not yet wired up (no bundled fallback font), so measurement falls
//! back to a monospace-ish heuristic — TODO: embed a font for wasm shaping.

pub struct TextMeasurer {
    #[cfg(not(target_arch = "wasm32"))]
    font_system: cosmic_text::FontSystem,
}

impl TextMeasurer {
    pub fn new() -> Self {
        TextMeasurer {
            #[cfg(not(target_arch = "wasm32"))]
            font_system: cosmic_text::FontSystem::new(),
        }
    }

    /// Returns the `(width, height)` in logical pixels that `text` occupies
    /// when shaped at `font_size`, unconstrained by wrapping.
    pub fn measure(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.measure_shaped(text, font_size)
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.measure_heuristic(text, font_size)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn measure_shaped(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        use cosmic_text::{Attrs, Buffer, Metrics, Shaping};

        let line_height = font_size * 1.2;
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        buffer.set_size(&mut self.font_system, None, None);
        let text = if text.is_empty() { " " } else { text };
        buffer.set_text(&mut self.font_system, text, &Attrs::new(), Shaping::Advanced);

        let mut width = 0.0f32;
        let mut lines = 0usize;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            lines += 1;
        }
        let height = (lines.max(1) as f32) * line_height;
        (width, height)
    }

    #[cfg(target_arch = "wasm32")]
    fn measure_heuristic(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        let char_count = text.chars().count().max(1) as f32;
        (char_count * font_size * 0.55, font_size * 1.2)
    }
}
