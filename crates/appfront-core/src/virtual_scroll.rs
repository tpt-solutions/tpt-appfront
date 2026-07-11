//! Virtual scrolling primitive for `List`/`DataGrid` (Phase 2). Backends
//! that render large collections (`appfront-dom`, `appfront-canvas`) can use
//! [`VirtualScroll::visible_range`] to render only the items currently in
//! view plus a small overscan buffer, instead of every item in the
//! collection. Backends that don't scroll interactively (`appfront-html`,
//! `appfront-ai-schema`) simply ignore [`NodeMeta::virtual_scroll`] and
//! render everything, which is correct for SSR/crawl/agent consumption.

use serde::{Deserialize, Serialize};

/// Configuration for windowed rendering of a fixed-height-item list.
/// Set via [`NodeRef::virtual_scroll`][crate::NodeRef::virtual_scroll].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VirtualScroll {
    /// Height (in the backend's own units — px for DOM/canvas) of a single item.
    pub item_height: f32,
    /// Height of the scrollable viewport.
    pub viewport_height: f32,
    /// Current scroll position, measured from the top of the full
    /// (unvirtualized) list. Backends update this from their own scroll
    /// event and feed it back in on the next render.
    pub scroll_offset: f32,
    /// Extra items rendered above/below the visible window to avoid a
    /// blank flash during fast scrolling.
    pub overscan: usize,
}

impl VirtualScroll {
    pub fn new(item_height: f32, viewport_height: f32) -> Self {
        VirtualScroll {
            item_height,
            viewport_height,
            scroll_offset: 0.0,
            overscan: 3,
        }
    }

    pub fn with_offset(mut self, offset: f32) -> Self {
        self.scroll_offset = offset.max(0.0);
        self
    }

    pub fn with_overscan(mut self, overscan: usize) -> Self {
        self.overscan = overscan;
        self
    }

    /// Computes which item indices (of `total_items`) fall within the
    /// viewport plus overscan, and the pixel-height spacers needed above/
    /// below that window so the scrollable area's total height (and thus
    /// scrollbar behavior) still matches the full, unvirtualized list.
    pub fn visible_range(&self, total_items: usize) -> VisibleRange {
        if total_items == 0 || self.item_height <= 0.0 {
            return VisibleRange { start: 0, end: 0, top_spacer: 0.0, bottom_spacer: 0.0 };
        }

        let first_visible = (self.scroll_offset / self.item_height).floor() as usize;
        let visible_count = (self.viewport_height / self.item_height).ceil() as usize + 1;

        let start = first_visible.saturating_sub(self.overscan).min(total_items);
        let end = (first_visible + visible_count + self.overscan).min(total_items);

        let top_spacer = start as f32 * self.item_height;
        let bottom_spacer = (total_items - end) as f32 * self.item_height;

        VisibleRange { start, end, top_spacer, bottom_spacer }
    }
}

/// The slice of items to actually render, plus spacer heights to preserve
/// the illusion of the full list being present for scrollbar purposes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleRange {
    /// First visible item index (inclusive).
    pub start: usize,
    /// Last visible item index (exclusive).
    pub end: usize,
    /// Pixel height of a spacer to place before the rendered slice.
    pub top_spacer: f32,
    /// Pixel height of a spacer to place after the rendered slice.
    pub bottom_spacer: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_has_empty_range() {
        let vs = VirtualScroll::new(20.0, 200.0);
        let range = vs.visible_range(0);
        assert_eq!(range, VisibleRange { start: 0, end: 0, top_spacer: 0.0, bottom_spacer: 0.0 });
    }

    #[test]
    fn scrolled_to_top_renders_first_window_plus_overscan() {
        // 20px items, 200px viewport => ~10 fit, +1 rounding, +3 overscan below (no overscan above at offset 0)
        let vs = VirtualScroll::new(20.0, 200.0);
        let range = vs.visible_range(1000);
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 14); // 10 + 1 + 3 overscan
        assert_eq!(range.top_spacer, 0.0);
        assert_eq!(range.bottom_spacer, (1000 - 14) as f32 * 20.0);
    }

    #[test]
    fn scrolled_mid_list_windows_around_offset() {
        let vs = VirtualScroll::new(20.0, 200.0).with_offset(1000.0); // first_visible = 50
        let range = vs.visible_range(1000);
        assert_eq!(range.start, 47); // 50 - 3 overscan
        assert_eq!(range.end, 64); // 50 + 11 + 3
        assert!(range.top_spacer > 0.0);
        assert!(range.bottom_spacer > 0.0);
    }

    #[test]
    fn range_clamped_to_total_items() {
        let vs = VirtualScroll::new(20.0, 200.0).with_offset(10_000.0);
        let range = vs.visible_range(10);
        assert_eq!(range.end, 10);
        assert_eq!(range.bottom_spacer, 0.0);
    }
}
