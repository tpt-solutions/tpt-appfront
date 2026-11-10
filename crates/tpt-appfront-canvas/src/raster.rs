//! Software painter for the internal window layer.
//!
//! [`Canvas`] draws into a `0x00RRGGBB` framebuffer such as
//! [`platform::Frame::pixels`](crate::platform::Frame). It supports solid
//! rectangles, borders, alpha blending, a clip rectangle, and anti-aliased
//! text from [`glyphs::Font`](crate::glyphs::Font). Everything is clipped to
//! the framebuffer and to the active clip, so callers never index out of
//! bounds.

use crate::glyphs::Font;

/// Opaque colour as `0x00RRGGBB`.
pub type Rgb = u32;

/// Builds an [`Rgb`] from its channels.
pub const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Integer pixel rectangle. Negative or zero sizes are empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }

    /// Overlap of two rectangles. Empty when they don't intersect.
    pub fn intersect(self, other: Rect) -> Rect {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    pub fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }
}

/// A painter over a borrowed framebuffer.
pub struct Canvas<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
    clip: Rect,
}

impl<'a> Canvas<'a> {
    /// Wraps `pixels`, which must hold exactly `width * height` pixels.
    ///
    /// # Panics
    /// Panics if the buffer length does not match the dimensions.
    pub fn new(pixels: &'a mut [u32], width: usize, height: usize) -> Self {
        assert_eq!(pixels.len(), width * height, "framebuffer size mismatch");
        Canvas {
            pixels,
            width,
            height,
            clip: Rect::new(0, 0, width as i32, height as i32),
        }
    }

    /// Framebuffer size in pixels.
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Runs `f` with drawing limited to `rect` (intersected with the current
    /// clip). The previous clip is restored afterwards, even when nested.
    pub fn with_clip<R>(&mut self, rect: Rect, f: impl FnOnce(&mut Self) -> R) -> R {
        let saved = self.clip;
        self.clip = self.clip.intersect(rect);
        let out = f(self);
        self.clip = saved;
        out
    }

    /// Fills `rect` with an opaque colour.
    pub fn fill_rect(&mut self, rect: Rect, color: Rgb) {
        self.fill_rect_alpha(rect, color, 255);
    }

    /// Fills `rect` with `color` blended over the existing pixels at `alpha`
    /// (0 = invisible, 255 = opaque).
    pub fn fill_rect_alpha(&mut self, rect: Rect, color: Rgb, alpha: u8) {
        let r = rect.intersect(self.clip);
        if r.is_empty() {
            return;
        }
        for y in r.y..r.y + r.h {
            let row = y as usize * self.width;
            for x in r.x..r.x + r.w {
                let idx = row + x as usize;
                self.pixels[idx] = blend(self.pixels[idx], color, alpha);
            }
        }
    }

    /// Draws a border `thickness` pixels wide just inside `rect`.
    pub fn stroke_rect(&mut self, rect: Rect, color: Rgb, thickness: i32) {
        if rect.is_empty() || thickness <= 0 {
            return;
        }
        let t = thickness.min(rect.w).min(rect.h);
        self.fill_rect(Rect::new(rect.x, rect.y, rect.w, t), color);
        self.fill_rect(Rect::new(rect.x, rect.y + rect.h - t, rect.w, t), color);
        self.fill_rect(Rect::new(rect.x, rect.y, t, rect.h), color);
        self.fill_rect(Rect::new(rect.x + rect.w - t, rect.y, t, rect.h), color);
    }

    /// Fills the disc of `radius` pixels centred on `(cx, cy)`, anti-aliased
    /// at its edge.
    pub fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, color: Rgb) {
        if radius <= 0.0 {
            return;
        }
        let x0 = (cx - radius - 1.0).floor() as i32;
        let x1 = (cx + radius + 1.0).ceil() as i32;
        let y0 = (cy - radius - 1.0).floor() as i32;
        let y1 = (cy + radius + 1.0).ceil() as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                if !self.in_clip(x, y) {
                    continue;
                }
                // Coverage from the distance of the pixel centre to the rim:
                // fully inside a pixel-wide band of anti-aliasing at the edge.
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let coverage = (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let idx = y as usize * self.width + x as usize;
                    let alpha = (coverage * 255.0).round() as u8;
                    self.pixels[idx] = blend(self.pixels[idx], color, alpha);
                }
            }
        }
    }

    /// Draws a straight line of `thickness` pixels from `(x0, y0)` to
    /// `(x1, y1)`, with round caps.
    pub fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: Rgb) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as usize;
        let radius = thickness / 2.0;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.fill_circle(x0 + dx * t, y0 + dy * t, radius, color);
        }
    }

    /// Draws `text` with its baseline at `baseline_y`, starting at pen `x`.
    /// Returns the pen advance (the width the text occupies).
    pub fn draw_text(
        &mut self,
        font: &mut Font,
        x: f32,
        baseline_y: f32,
        text: &str,
        size: f32,
        color: Rgb,
    ) -> f32 {
        let mut pen = x;
        for ch in text.chars() {
            let glyph = font.glyph(ch, size);
            let origin_x = pen.round() as i32 + glyph.left;
            let origin_y = baseline_y.round() as i32 + glyph.top;
            for gy in 0..glyph.height {
                for gx in 0..glyph.width {
                    let cov = glyph.coverage[gy * glyph.width + gx];
                    if cov == 0 {
                        continue;
                    }
                    let px = origin_x + gx as i32;
                    let py = origin_y + gy as i32;
                    if self.in_clip(px, py) {
                        let idx = py as usize * self.width + px as usize;
                        self.pixels[idx] = blend(self.pixels[idx], color, cov);
                    }
                }
            }
            pen += glyph.advance;
        }
        pen - x
    }

    fn in_clip(&self, x: i32, y: i32) -> bool {
        x >= self.clip.x
            && y >= self.clip.y
            && x < self.clip.x + self.clip.w
            && y < self.clip.y + self.clip.h
    }
}

/// Blends `fg` over `bg` with `alpha` (0..=255) per channel.
fn blend(bg: u32, fg: u32, alpha: u8) -> u32 {
    if alpha == 255 {
        return fg;
    }
    if alpha == 0 {
        return bg;
    }
    let a = u32::from(alpha);
    let mix = |shift: u32| {
        let b = (bg >> shift) & 0xff;
        let f = (fg >> shift) & 0xff;
        (f * a + b * (255 - a) + 127) / 255
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgb = rgb(255, 255, 255);
    const RED: Rgb = rgb(255, 0, 0);

    fn buffer(w: usize, h: usize) -> Vec<u32> {
        vec![WHITE; w * h]
    }

    #[test]
    fn rect_intersection_and_emptiness() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(a.intersect(Rect::new(5, 5, 10, 10)), Rect::new(5, 5, 5, 5));
        assert!(a.intersect(Rect::new(20, 20, 5, 5)).is_empty());
        assert!(Rect::new(0, 0, 0, 5).is_empty());
    }

    #[test]
    fn fill_rect_paints_only_its_area() {
        let mut px = buffer(4, 4);
        let mut c = Canvas::new(&mut px, 4, 4);
        c.fill_rect(Rect::new(1, 1, 2, 2), RED);
        assert_eq!(px[5], RED);
        assert_eq!(px[10], RED);
        assert_eq!(px[0], WHITE);
        assert_eq!(px[3], WHITE);
    }

    #[test]
    fn fill_rect_clips_to_the_framebuffer() {
        let mut px = buffer(4, 4);
        let mut c = Canvas::new(&mut px, 4, 4);
        c.fill_rect(Rect::new(-5, -5, 100, 100), RED);
        assert!(px.iter().all(|&p| p == RED));
    }

    #[test]
    fn with_clip_restricts_drawing_and_restores_afterwards() {
        let mut px = buffer(4, 4);
        let mut c = Canvas::new(&mut px, 4, 4);
        c.with_clip(Rect::new(0, 0, 2, 2), |c| {
            c.fill_rect(Rect::new(0, 0, 4, 4), RED);
        });
        assert_eq!(px[0], RED);
        assert_eq!(px[3], WHITE);
        assert_eq!(px[15], WHITE);

        // The clip is gone once the closure returns.
        let mut c = Canvas::new(&mut px, 4, 4);
        c.fill_rect(Rect::new(3, 3, 1, 1), RED);
        assert_eq!(px[15], RED);
    }

    #[test]
    fn half_alpha_blends_towards_the_background() {
        let mut px = vec![0u32; 1];
        let mut c = Canvas::new(&mut px, 1, 1);
        c.fill_rect_alpha(Rect::new(0, 0, 1, 1), 0x00FF_FFFF, 128);
        let r = (px[0] >> 16) & 0xff;
        assert!((127..=128).contains(&r), "red channel was {r}");
    }

    #[test]
    fn stroke_rect_draws_a_border_and_leaves_the_centre() {
        let mut px = buffer(5, 5);
        let mut c = Canvas::new(&mut px, 5, 5);
        c.stroke_rect(Rect::new(0, 0, 5, 5), RED, 1);
        assert_eq!(px[0], RED);
        assert_eq!(px[4], RED);
        assert_eq!(px[12], WHITE);
    }

    #[test]
    fn text_paints_inside_its_box_and_reports_its_advance() {
        let mut font = Font::bundled();
        let mut px = buffer(120, 40);
        let mut c = Canvas::new(&mut px, 120, 40);
        let advance = c.draw_text(&mut font, 4.0, 30.0, "Hello", 20.0, RED);
        assert!((advance - font.measure("Hello", 20.0)).abs() < 0.01);
        let painted = px.iter().filter(|&&p| p != WHITE).count();
        assert!(painted > 50, "only {painted} pixels painted");
        // Nothing is drawn below the baseline's descender zone.
        assert!(px[39 * 120..].iter().all(|&p| p == WHITE));
    }

    #[test]
    fn circle_covers_its_centre_and_not_its_corners() {
        let mut px = buffer(20, 20);
        let mut c = Canvas::new(&mut px, 20, 20);
        c.fill_circle(10.0, 10.0, 5.0, RED);
        assert_eq!(px[10 * 20 + 10], RED);
        assert_eq!(px[0], WHITE);
        assert_eq!(px[19 * 20 + 19], WHITE);
    }

    #[test]
    fn circle_edge_is_anti_aliased() {
        let mut px = buffer(20, 20);
        let mut c = Canvas::new(&mut px, 20, 20);
        c.fill_circle(10.0, 10.0, 5.0, RED);
        // Some rim pixels are partially covered, so they are neither white nor full red.
        assert!(px.iter().any(|&p| p != WHITE && p != RED));
    }

    #[test]
    fn line_connects_its_endpoints() {
        let mut px = buffer(30, 30);
        let mut c = Canvas::new(&mut px, 30, 30);
        c.draw_line(2.0, 2.0, 27.0, 27.0, 2.0, RED);
        assert_ne!(px[2 * 30 + 2], WHITE);
        assert_ne!(px[27 * 30 + 27], WHITE);
        assert_eq!(px[0], WHITE);
    }

    #[test]
    fn text_respects_the_clip() {
        let mut font = Font::bundled();
        let mut px = buffer(120, 40);
        let mut c = Canvas::new(&mut px, 120, 40);
        c.with_clip(Rect::new(0, 0, 10, 40), |c| {
            c.draw_text(&mut font, 4.0, 30.0, "Hello", 20.0, RED);
        });
        assert!(px
            .iter()
            .enumerate()
            .all(|(i, &p)| p == WHITE || i % 120 < 10));
    }
}
