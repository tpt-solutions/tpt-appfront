//! Text measurement for layout sizing (see `spec.txt`'s canvas backend:
//! `cosmic-text` for shaping). Layout only needs the shaped extent of each
//! text run, not glyph painting — `egui`'s own text painter draws the
//! characters once `taffy` has decided how much space to give them.
//!
//! Real shaping via `cosmic-text` is opt-in behind the `full-text-shaping`
//! feature. On native, `cosmic-text` reads the system font database, so it
//! works out of the box. On `wasm32` there is no system font database, so
//! we bundle a fallback font ([`BUNDLED_FONT`], Noto Sans — OFL 1.1) and
//! load it into `fontdb` at startup; this replaces the heuristic width
//! estimator (which previously was the only option on web — see the old
//! `todo.md` Phase4 item) with real CJK/Arabic/ligature-accurate
//! measurement. The heuristic estimator remains the default (feature off) to
//! keep the WASM payload small.

#[cfg(feature = "full-text-shaping")]
const BUNDLED_FONT: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");

pub struct TextMeasurer {
    #[cfg(feature = "full-text-shaping")]
    font_system: cosmic_text::FontSystem,
}

impl TextMeasurer {
    /// Creates a measurer using the default font source for the current
    /// target. On native that's the system font database; on `wasm32` it's
    /// the bundled Noto Sans fallback (real `cosmic-text` shaping instead of
    /// the heuristic estimator). With the `full-text-shaping` feature off,
    /// this builds the heuristic estimator (no font stack at all).
    pub fn new() -> Self {
        #[cfg(feature = "full-text-shaping")]
        {
            let mut font_system = cosmic_text::FontSystem::new();
            // On wasm there is no system font database, so the bundled font
            // is the *only* source; on native it's an extra face alongside
            // whatever the OS provides. `load_font_data` copies the bytes into
            // `fontdb`, which is fine — it's done once at startup.
            font_system.db_mut().load_font_data(BUNDLED_FONT.to_vec());
            TextMeasurer { font_system }
        }
        #[cfg(not(feature = "full-text-shaping"))]
        {
            TextMeasurer {}
        }
    }

    /// Creates a measurer that uses a caller-supplied font (e.g. a font
    /// fetched at runtime) instead of the bundled one. Only available with the
    /// `full-text-shaping` feature, since the heuristic path needs no font.
    #[cfg(feature = "full-text-shaping")]
    pub fn with_font_data(font_bytes: Vec<u8>) -> Self {
        let mut font_system = cosmic_text::FontSystem::new();
        font_system.db_mut().load_font_data(font_bytes);
        TextMeasurer { font_system }
    }

    /// Returns the `(width, height)` in logical pixels that `text` occupies
    /// when shaped at `font_size`, unconstrained by wrapping.
    pub fn measure(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        #[cfg(feature = "full-text-shaping")]
        {
            self.measure_shaped(text, font_size)
        }
        #[cfg(not(feature = "full-text-shaping"))]
        {
            self.measure_heuristic(text, font_size)
        }
    }

    #[cfg(feature = "full-text-shaping")]
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

    #[cfg(not(feature = "full-text-shaping"))]
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

    // The bundled-font path: only compiled/exercised with
    // `--features full-text-shaping` (native). Verifies that real shaping
    // produces a non-trivial, deterministic width for Noto Sans text.
    #[cfg(all(feature = "full-text-shaping", not(target_arch = "wasm32")))]
    #[test]
    fn bundled_font_shapes_latin_text() {
        let mut m = TextMeasurer::new();
        // Noto Sans should give a width meaningfully larger than the empty
        // placeholder and identical across identical inputs.
        let (w1, h) = m.measure("Hello, AppFront", 16.0);
        let (w2, _) = m.measure("Hello, AppFront", 16.0);
        assert!(w1 > 0.0);
        assert!(h > 0.0);
        assert_eq!(w1, w2);
    }
}
