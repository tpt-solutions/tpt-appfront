# tpt-appfront-canvas-3d

The 3D viewport contract for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

A 3D engine implements `SceneDelegate` and is hosted by a `Viewport3D`. The host
forwards frames and pointer input. The delegate answers picks and says whether it
can present pixels.

This crate has no dependencies. It defines the contract only. Rendering is up to
the delegate, so the engine behind it can use any GPU API.

```rust
use tpt_appfront_canvas_3d::{Hit3D, PresentStatus, SceneDelegate, ViewportPoint, ViewportSize};

struct Scene;
impl SceneDelegate for Scene {
    fn update(&mut self, _dt: f32) {}
    fn pick(&self, _size: ViewportSize, _at: ViewportPoint) -> Option<Hit3D> { None }
    fn present(&mut self, _size: ViewportSize) -> PresentStatus { PresentStatus::Unsupported }
}
```
