//! Text measurement for layout sizing (see `spec.txt`'s canvas backend:
//! `cosmic-text` for shaping). Layout only needs the shaped extent of each
//! text run, not glyph painting — `egui`'s own text painter draws the
//! characters once `taffy` has decided how much space to give them.
//!
//! Real shaping via `cosmic-text` is opt-in behind the `full-text-shaping`
//! feature (native only) for apps that need CJK/Arabic/ligature-accurate
//! measurement. It's off by default: the heuristic width estimator below
//! covers the common Latin-text case without pulling in a font-shaping
//! stack, and is the only option on `wasm32` today (no bundled fallback
//! font wired up for `fontdb` on web yet — TODO).

pub struct TextMeasurer {
    #[cfg(all(not(target_arch = "wasm32"), feature = "full-text-shaping"))]
    font_system: cosmic_text::FontSystem,
}

impl TextMeasurer {
    pub fn new() -> Self {
        TextMeasurer {
            #[cfg(all(not(target_arch = "wasm32"), feature = "full-text-shaping"))]
            font_system: cosmic_text::FontSystem::new(),
        }
    }

    /// Returns the `(width, height)` in logical pixels that `text` occupies
    /// when shaped at `font_size`, unconstrained by wrapping.
    pub fn measure(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        #[cfg(all(not(target_arch = "wasm32"), feature = "full-text-shaping"))]
        {
            self.measure_shaped(text, font_size)
        }
        #[cfg(not(all(not(target_arch = "wasm32"), feature = "full-text-shaping")))]
        {
            self.measure_heuristic(text, font_size)
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "full-text-shaping"))]
    fn measure_shaped(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        use cosmic_text::{Attrs, Buffer, Metrics, Shaping};

        let line_height = font_size * 1.2;
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        buffer.set_size(None, None);
        let text = if text.is_empty() { " " } else { text };
        buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut width = 0.0f32;
        let mut lines = 0usize;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            lines += 1;
        }
        let height = (lines.max(1) as f32) * line_height;
        (width, height)
    }

    #[cfg(not(all(not(target_arch = "wasm32"), feature = "full-text-shaping")))]
    fn measure_heuristic(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        let char_count = text.chars().count().max(1) as f32;
        (char_count * font_size * 0.55, font_size * 1.2)
    }
}

impl Default for TextMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_text_measures_wider() {
        let mut m = TextMeasurer::new();
        let (short_w, _) = m.measure("hi", 16.0);
        let (long_w, _) = m.measure("hello world", 16.0);
        assert!(long_w > short_w);
    }

    #[test]
    fn larger_font_size_measures_wider_and_taller() {
        let mut m = TextMeasurer::new();
        let (w_small, h_small) = m.measure("hello", 16.0);
        let (w_large, h_large) = m.measure("hello", 32.0);
        assert!(w_large > w_small);
        assert!(h_large > h_small);
    }

    #[test]
    fn empty_text_measures_as_nonzero() {
        let mut m = TextMeasurer::new();
        let (w, h) = m.measure("", 16.0);
        assert!(w > 0.0);
        assert!(h > 0.0);
    }

    #[test]
    fn same_text_and_size_measures_identically() {
        let mut m = TextMeasurer::new();
        let (w1, h1) = m.measure("consistent", 16.0);
        let (w2, h2) = m.measure("consistent", 16.0);
        assert_eq!(w1, w2);
        assert_eq!(h1, h2);
    }
}
