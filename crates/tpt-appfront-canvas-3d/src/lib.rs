//! The 3D viewport contract for TPT AppFront.
//!
//! [`SceneDelegate`] is what a 3D engine implements. [`Viewport3D`] hosts one: it
//! forwards frames and pointer input, and returns the delegate's picks. Points are in
//! viewport pixels, with the origin at the top left and y pointing down.

/// A position inside a 3D viewport, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportPoint {
    pub x: f32,
    pub y: f32,
}

/// The viewport's size in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSize {
    pub width: f32,
    pub height: f32,
}

/// The nearest thing a pointer hit in the delegate's scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit3D {
    /// The delegate's own identifier for the hit object.
    pub id: u64,
    /// World-space point of the hit.
    pub point: [f32; 3],
    /// Distance from the viewer to `point`.
    pub distance: f32,
}

/// Whether a delegate could present a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentStatus {
    /// Pixels were presented into the viewport.
    Presented,
    /// The delegate renders somewhere the host cannot show yet, for example GPU-only
    /// output without a readback path. Picking still works.
    Unsupported,
}

/// A rendered frame the host can draw: row-major RGBA8, top row first, no row padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFrame<'a> {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: &'a [u8],
}

/// The contract a 3D engine implements to live inside an appfront viewport.
pub trait SceneDelegate {
    /// Advance the scene by `dt` seconds.
    fn update(&mut self, dt: f32);

    /// Resolve a viewport position to the nearest hit, if any.
    fn pick(&self, size: ViewportSize, at: ViewportPoint) -> Option<Hit3D>;

    /// Present a frame sized to `size`.
    fn present(&mut self, size: ViewportSize) -> PresentStatus;

    /// The most recent frame from `present`, for hosts that cannot take GPU output
    /// directly (a webview canvas, an image export). The default has none.
    fn pixels(&self) -> Option<PixelFrame<'_>> {
        None
    }
}

/// Hosts a [`SceneDelegate`] and tracks the viewport size.
pub struct Viewport3D<D: SceneDelegate> {
    delegate: D,
    size: ViewportSize,
}

impl<D: SceneDelegate> Viewport3D<D> {
    pub fn new(delegate: D, size: ViewportSize) -> Self {
        Viewport3D { delegate, size }
    }

    pub fn size(&self) -> ViewportSize {
        self.size
    }

    /// Change the viewport size, for example after a window resize.
    pub fn resize(&mut self, size: ViewportSize) {
        self.size = size;
    }

    pub fn delegate(&self) -> &D {
        &self.delegate
    }

    pub fn delegate_mut(&mut self) -> &mut D {
        &mut self.delegate
    }

    /// The delegate's most recent frame, if it produces pixels.
    pub fn pixels(&self) -> Option<PixelFrame<'_>> {
        self.delegate.pixels()
    }

    /// Advance one frame, then present it.
    pub fn frame(&mut self, dt: f32) -> PresentStatus {
        self.delegate.update(dt);
        self.delegate.present(self.size)
    }

    /// Resolve a pointer press to a hit. Positions outside the viewport are ignored.
    pub fn pointer_down(&self, at: ViewportPoint) -> Option<Hit3D> {
        let inside =
            at.x >= 0.0 && at.y >= 0.0 && at.x < self.size.width && at.y < self.size.height;
        if !inside {
            return None;
        }
        self.delegate.pick(self.size, at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records what the host sends it, and picks a fixed object at the pointer.
    #[derive(Default)]
    struct Recorder {
        updates: Vec<f32>,
        presents: Vec<ViewportSize>,
    }

    impl SceneDelegate for Recorder {
        fn update(&mut self, dt: f32) {
            self.updates.push(dt);
        }
        fn pick(&self, _size: ViewportSize, at: ViewportPoint) -> Option<Hit3D> {
            Some(Hit3D {
                id: 7,
                point: [at.x, at.y, 0.0],
                distance: 1.0,
            })
        }
        fn present(&mut self, size: ViewportSize) -> PresentStatus {
            self.presents.push(size);
            PresentStatus::Presented
        }
    }

    fn size(width: f32, height: f32) -> ViewportSize {
        ViewportSize { width, height }
    }

    #[test]
    fn a_frame_updates_then_presents_at_the_current_size() {
        let mut vp = Viewport3D::new(Recorder::default(), size(640.0, 480.0));
        assert_eq!(vp.frame(0.016), PresentStatus::Presented);
        vp.resize(size(800.0, 600.0));
        vp.frame(0.016);
        let d = vp.delegate();
        assert_eq!(d.updates, vec![0.016, 0.016]);
        assert_eq!(d.presents, vec![size(640.0, 480.0), size(800.0, 600.0)]);
    }

    #[test]
    fn pointer_presses_inside_the_viewport_are_picked() {
        let vp = Viewport3D::new(Recorder::default(), size(640.0, 480.0));
        let hit = vp
            .pointer_down(ViewportPoint { x: 10.0, y: 20.0 })
            .expect("inside");
        assert_eq!(hit.id, 7);
        assert_eq!(hit.point, [10.0, 20.0, 0.0]);
    }

    #[test]
    fn pointer_presses_outside_the_viewport_are_ignored() {
        let vp = Viewport3D::new(Recorder::default(), size(640.0, 480.0));
        assert!(vp.pointer_down(ViewportPoint { x: -1.0, y: 0.0 }).is_none());
        assert!(vp
            .pointer_down(ViewportPoint { x: 640.0, y: 10.0 })
            .is_none());
        assert!(vp
            .pointer_down(ViewportPoint { x: 10.0, y: 480.0 })
            .is_none());
    }

    #[test]
    fn a_delegate_without_pixels_has_no_frame() {
        let mut vp = Viewport3D::new(Recorder::default(), size(4.0, 4.0));
        vp.frame(0.0);
        assert!(vp.pixels().is_none());
    }

    #[test]
    fn a_delegate_with_pixels_hands_its_frame_to_the_host() {
        struct Solid(Vec<u8>, u32, u32);
        impl SceneDelegate for Solid {
            fn update(&mut self, _dt: f32) {}
            fn pick(&self, _s: ViewportSize, _at: ViewportPoint) -> Option<Hit3D> {
                None
            }
            fn present(&mut self, _s: ViewportSize) -> PresentStatus {
                PresentStatus::Presented
            }
            fn pixels(&self) -> Option<PixelFrame<'_>> {
                Some(PixelFrame {
                    width: self.1,
                    height: self.2,
                    rgba: &self.0,
                })
            }
        }
        let vp = Viewport3D::new(Solid(vec![1, 2, 3, 4], 1, 1), size(1.0, 1.0));
        let frame = vp.pixels().expect("a frame");
        assert_eq!((frame.width, frame.height), (1, 1));
        assert_eq!(frame.rgba, &[1, 2, 3, 4]);
    }

    #[test]
    fn an_unsupported_delegate_still_picks() {
        struct GpuOnly;
        impl SceneDelegate for GpuOnly {
            fn update(&mut self, _dt: f32) {}
            fn pick(&self, _s: ViewportSize, _at: ViewportPoint) -> Option<Hit3D> {
                None
            }
            fn present(&mut self, _s: ViewportSize) -> PresentStatus {
                PresentStatus::Unsupported
            }
        }
        let mut vp = Viewport3D::new(GpuOnly, size(10.0, 10.0));
        assert_eq!(vp.frame(0.0), PresentStatus::Unsupported);
        assert!(vp.pointer_down(ViewportPoint { x: 1.0, y: 1.0 }).is_none());
    }
}
