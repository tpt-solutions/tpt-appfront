//! Glyph rasterisation for the internal painter (see `raster.rs`).
//!
//! The bundled Noto Sans face is read with `ttf-parser`, its outlines are
//! flattened to line segments, and each glyph is filled with a 4x4
//! supersampled nonzero-winding scan. The resulting 8-bit coverage bitmaps
//! are cached per character and size. Only Latin text is covered by the
//! bundled face; other characters fall back to a blank advance.

use std::collections::HashMap;

use ttf_parser::{Face, OutlineBuilder};

/// Noto Sans Regular (SIL Open Font License 1.1).
const BUNDLED_FONT: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");

/// Horizontal and vertical samples per pixel when computing coverage.
const SUPERSAMPLE: usize = 4;
/// Line segments used to approximate each quadratic / cubic Bézier.
const QUAD_STEPS: usize = 8;
const CUBIC_STEPS: usize = 12;

/// One rasterised glyph. `left`/`top` are the offset of the bitmap's top-left
/// corner from the pen position on the baseline (`top` is negative above the
/// baseline).
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    pub left: i32,
    pub top: i32,
    pub width: usize,
    pub height: usize,
    /// Row-major coverage, `width * height` bytes, 0 = empty, 255 = full.
    pub coverage: Vec<u8>,
    /// Horizontal pen advance in pixels.
    pub advance: f32,
}

impl Glyph {
    fn blank(advance: f32) -> Self {
        Glyph {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            coverage: Vec::new(),
            advance,
        }
    }
}

/// A font face plus a cache of rasterised glyphs.
pub struct Font {
    face: Face<'static>,
    cache: HashMap<(char, u32), Glyph>,
}

impl Font {
    /// The bundled Noto Sans Regular face.
    pub fn bundled() -> Self {
        Font::from_static(BUNDLED_FONT).expect("bundled Noto Sans is a valid font")
    }

    /// Parses the first face in `data`. `data` must outlive the font, which is
    /// why this takes `'static` data; use [`Font::bundled`] for the default.
    pub fn from_static(data: &'static [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        Ok(Font {
            face: Face::parse(data, 0)?,
            cache: HashMap::new(),
        })
    }

    /// Pen advance of `text` at `size` pixels, without kerning.
    pub fn measure(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|ch| self.advance(ch, size)).sum()
    }

    /// Pen advance of one character at `size` pixels.
    pub fn advance(&self, ch: char, size: f32) -> f32 {
        let scale = size / f32::from(self.face.units_per_em());
        match self.face.glyph_index(ch) {
            Some(id) => self
                .face
                .glyph_hor_advance(id)
                .map_or(0.0, |units| f32::from(units) * scale),
            // Characters outside the face get a half-em blank so the text
            // still has visible spacing.
            None => size * 0.5,
        }
    }

    /// Returns the rasterised glyph for `ch` at `size` pixels, creating and
    /// caching it on first use.
    pub fn glyph(&mut self, ch: char, size: f32) -> &Glyph {
        let key = (ch, (size * 10.0).round() as u32);
        if !self.cache.contains_key(&key) {
            let glyph = self.rasterize(ch, size);
            self.cache.insert(key, glyph);
        }
        &self.cache[&key]
    }

    fn rasterize(&self, ch: char, size: f32) -> Glyph {
        let advance = self.advance(ch, size);
        let Some(id) = self.face.glyph_index(ch) else {
            return Glyph::blank(advance);
        };
        let scale = size / f32::from(self.face.units_per_em());
        let mut builder = EdgeBuilder::new(scale);
        // Blank glyphs such as space have no outline; they keep their advance.
        if self.face.outline_glyph(id, &mut builder).is_none() {
            return Glyph::blank(advance);
        }
        match coverage_from_edges(&builder.edges) {
            Some((left, top, width, height, coverage)) => Glyph {
                left,
                top,
                width,
                height,
                coverage,
                advance,
            },
            None => Glyph::blank(advance),
        }
    }
}

/// Collects a glyph outline as straight edges in pixel space, with the
/// baseline at y = 0 and y pointing down.
struct EdgeBuilder {
    edges: Vec<[f32; 4]>,
    scale: f32,
    cur: (f32, f32),
    start: (f32, f32),
}

impl EdgeBuilder {
    fn new(scale: f32) -> Self {
        EdgeBuilder {
            edges: Vec::new(),
            scale,
            cur: (0.0, 0.0),
            start: (0.0, 0.0),
        }
    }

    fn to_pixels(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale, -y * self.scale)
    }

    fn line(&mut self, to: (f32, f32)) {
        let from = self.cur;
        // Horizontal edges never cross a scanline, so they are skipped.
        if from.1 != to.1 {
            self.edges.push([from.0, from.1, to.0, to.1]);
        }
        self.cur = to;
    }
}

impl OutlineBuilder for EdgeBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.to_pixels(x, y);
        self.cur = p;
        self.start = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.to_pixels(x, y);
        self.line(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c = self.to_pixels(x1, y1);
        let end = self.to_pixels(x, y);
        let start = self.cur;
        for i in 1..=QUAD_STEPS {
            let t = i as f32 / QUAD_STEPS as f32;
            let mt = 1.0 - t;
            let px = mt * mt * start.0 + 2.0 * mt * t * c.0 + t * t * end.0;
            let py = mt * mt * start.1 + 2.0 * mt * t * c.1 + t * t * end.1;
            self.line((px, py));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.to_pixels(x1, y1);
        let c2 = self.to_pixels(x2, y2);
        let end = self.to_pixels(x, y);
        let start = self.cur;
        for i in 1..=CUBIC_STEPS {
            let t = i as f32 / CUBIC_STEPS as f32;
            let mt = 1.0 - t;
            let a = mt * mt * mt;
            let b = 3.0 * mt * mt * t;
            let c = 3.0 * mt * t * t;
            let d = t * t * t;
            let px = a * start.0 + b * c1.0 + c * c2.0 + d * end.0;
            let py = a * start.1 + b * c1.1 + c * c2.1 + d * end.1;
            self.line((px, py));
        }
    }

    fn close(&mut self) {
        let start = self.start;
        self.line(start);
    }
}

/// Fills the closed edge set with nonzero winding and returns the bitmap
/// bounds and coverage. Returns `None` when there are no edges.
fn coverage_from_edges(edges: &[[f32; 4]]) -> Option<(i32, i32, usize, usize, Vec<u8>)> {
    if edges.is_empty() {
        return None;
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for e in edges {
        min_x = min_x.min(e[0]).min(e[2]);
        max_x = max_x.max(e[0]).max(e[2]);
        min_y = min_y.min(e[1]).min(e[3]);
        max_y = max_y.max(e[1]).max(e[3]);
    }
    let left = min_x.floor() as i32;
    let top = min_y.floor() as i32;
    let width = (max_x.ceil() as i32 - left).max(1) as usize;
    let height = (max_y.ceil() as i32 - top).max(1) as usize;

    let ss = SUPERSAMPLE;
    let step = 1.0 / ss as f32;
    let mut hits = vec![0u32; width * height];
    let mut crossings: Vec<(f32, i32)> = Vec::new();

    for sub_row in 0..height * ss {
        let y = top as f32 + (sub_row as f32 + 0.5) * step;
        let row = sub_row / ss;

        crossings.clear();
        for e in edges {
            let (y0, y1) = (e[1], e[3]);
            if (y0 <= y && y < y1) || (y1 <= y && y < y0) {
                let t = (y - y0) / (y1 - y0);
                let x = e[0] + t * (e[2] - e[0]);
                let dir = if y1 > y0 { 1 } else { -1 };
                crossings.push((x, dir));
            }
        }
        crossings.sort_by(|a, b| a.0.total_cmp(&b.0));

        // Walk the sorted crossings; a span is inside while the winding
        // number is nonzero.
        let mut winding = 0;
        let mut span_start = 0.0;
        for &(x, dir) in &crossings {
            let was_inside = winding != 0;
            winding += dir;
            let is_inside = winding != 0;
            if !was_inside && is_inside {
                span_start = x;
            } else if was_inside && !is_inside {
                accumulate_span(&mut hits, width, row, left, span_start, x);
            }
        }
    }

    let full = (ss * ss) as u32;
    let coverage = hits
        .iter()
        .map(|&h| ((h * 255 + full / 2) / full).min(255) as u8)
        .collect();
    Some((left, top, width, height, coverage))
}

/// Adds the horizontal subsamples of one pixel row that fall inside `[a, b)`.
fn accumulate_span(hits: &mut [u32], width: usize, row: usize, left: i32, a: f32, b: f32) {
    let step = 1.0 / SUPERSAMPLE as f32;
    let first = (a.floor() as i32 - left).max(0) as usize;
    let last = ((b.ceil() as i32 - left).max(0) as usize).min(width);
    for col in first..last {
        for sx in 0..SUPERSAMPLE {
            let x = left as f32 + col as f32 + (sx as f32 + 0.5) * step;
            if x >= a && x < b {
                hits[row * width + col] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_has_metrics_for_latin_text() {
        let font = Font::bundled();
        assert!(font.advance('a', 16.0) > 0.0);
        assert!(font.measure("hello", 16.0) > font.measure("hi", 16.0));
    }

    #[test]
    fn space_has_advance_but_no_bitmap() {
        let mut font = Font::bundled();
        let g = font.glyph(' ', 16.0).clone();
        assert!(g.advance > 0.0);
        assert_eq!(g.width, 0);
        assert!(g.coverage.is_empty());
    }

    #[test]
    fn letter_bitmap_has_partial_coverage_edges() {
        let mut font = Font::bundled();
        let g = font.glyph('o', 24.0).clone();
        assert!(g.width > 0 && g.height > 0);
        assert_eq!(g.coverage.len(), g.width * g.height);
        // An anti-aliased outline has both solid and partially covered pixels.
        assert!(g.coverage.contains(&255));
        assert!(g.coverage.iter().any(|&c| c > 0 && c < 255));
        // Letters sit on the baseline, so their top is above it.
        assert!(g.top < 0);
    }

    #[test]
    fn glyphs_are_cached_per_character_and_size() {
        let mut font = Font::bundled();
        let first = font.glyph('e', 16.0).clone();
        let second = font.glyph('e', 16.0).clone();
        assert_eq!(first, second);
        assert_eq!(font.cache.len(), 1);
        font.glyph('e', 32.0);
        assert_eq!(font.cache.len(), 2);
    }

    #[test]
    fn unknown_characters_fall_back_to_a_blank_advance() {
        let mut font = Font::bundled();
        let g = font.glyph('\u{1F600}', 16.0).clone();
        assert_eq!(g.width, 0);
        assert!(g.advance > 0.0);
    }
}
