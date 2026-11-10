//! Text measurement for layout sizing.
//!
//! Layout needs the extent of each text run before painting. This measures
//! with the same bundled font the painter draws with ([`glyphs::Font`]), so a
//! box sized by `taffy` fits the text that is drawn into it. Advances are
//! summed without kerning or shaping, so scripts that need shaping (complex
//! Arabic, Indic, and similar) are not laid out correctly yet.
//!
//! [`glyphs::Font`]: crate::glyphs::Font

use crate::glyphs::Font;

/// Line height as a multiple of the font size.
const LINE_HEIGHT: f32 = 1.2;

pub struct TextMeasurer {
    font: Font,
}

impl TextMeasurer {
    /// Creates a measurer over the bundled Noto Sans face.
    pub fn new() -> Self {
        TextMeasurer {
            font: Font::bundled(),
        }
    }

    /// Returns the `(width, height)` in logical pixels that `text` occupies
    /// at `font_size`, unconstrained by wrapping. Each line of `text` (split
    /// on `\n`) adds one line height.
    pub fn measure(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        let line_height = font_size * LINE_HEIGHT;
        let mut width = 0.0f32;
        let mut lines = 0usize;
        for line in text.split('\n') {
            width = width.max(self.font.measure(line, font_size));
            lines += 1;
        }
        // An empty string still occupies one line, so it measures non-zero.
        let width = if text.is_empty() {
            self.font.measure(" ", font_size)
        } else {
            width
        };
        (width, lines.max(1) as f32 * line_height)
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

    #[test]
    fn width_matches_the_painter_advance() {
        let mut m = TextMeasurer::new();
        let font = Font::bundled();
        let (w, _) = m.measure("Hello, AppFront", 16.0);
        assert!((w - font.measure("Hello, AppFront", 16.0)).abs() < 0.001);
    }

    #[test]
    fn each_newline_adds_a_line() {
        let mut m = TextMeasurer::new();
        let (_, one) = m.measure("a", 16.0);
        let (_, three) = m.measure("a\nb\nc", 16.0);
        assert!((three - one * 3.0).abs() < 0.001);
    }
}
