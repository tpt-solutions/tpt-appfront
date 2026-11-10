//! Internal, Rust-only window layer for the canvas backend.
//!
//! This replaces `eframe`/`winit` (Apache-2.0 only, banned in `deny.toml`).
//! Each platform opens a window, pumps its native event queue, and presents a
//! software framebuffer. The shared surface is deliberately small:
//!
//! * [`run`] opens a window and calls a per-frame closure with a [`Frame`].
//! * [`Frame::pixels`] is the escape hatch: a raw `0x00RRGGBB` framebuffer
//!   that custom code can paint into directly, the equivalent of the raw-egui
//!   escape hatch used by `examples/node-graph`.
//! * [`Event`]s are translated into one platform-neutral model.
//!
//! Native backends: Windows (Win32 via `windows-sys`), Linux (X11 via
//! `x11rb-protocol`, pure Rust) and macOS (AppKit via `objc2`), all behind
//! [`run`]. The browser backend (`web`, wasm32 only) is started by `run_web` in
//! the crate root, because it paints onto a named `<canvas>` rather than opening
//! a window. On other targets [`run`] returns [`PlatformError::Unsupported`].

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web;
#[cfg(windows)]
mod windows;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11;

use std::fmt;

/// Mouse buttons, numbered to match the DOM `MouseEvent.button` convention
/// (0 = primary, 1 = middle, 2 = secondary) so the DOM and canvas backends
/// agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Primary,
    Middle,
    Secondary,
    Other(u8),
}

impl MouseButton {
    /// DOM-compatible button index.
    pub fn index(self) -> i16 {
        match self {
            MouseButton::Primary => 0,
            MouseButton::Middle => 1,
            MouseButton::Secondary => 2,
            MouseButton::Other(n) => i16::from(n),
        }
    }
}

/// One input event delivered during a frame. Coordinates are in pixels,
/// relative to the top-left of the framebuffer.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PointerMoved {
        x: i32,
        y: i32,
    },
    PointerDown {
        x: i32,
        y: i32,
        button: MouseButton,
    },
    PointerUp {
        x: i32,
        y: i32,
        button: MouseButton,
    },
    /// A key went down. `code` is the platform virtual-key code; use
    /// [`Event::Text`] for characters. The modifier flags are the state
    /// when the key went down.
    KeyDown {
        code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
    },
    /// The mouse wheel turned at `(x, y)`. `delta` is in notches: one
    /// physical wheel click is `1.0`, positive when turned away from the user.
    Wheel {
        x: i32,
        y: i32,
        delta: f32,
    },
    /// A printable character was typed.
    Text(char),
    /// The window's client area changed size (the new size is also on
    /// [`Frame`]).
    Resized {
        width: u32,
        height: u32,
    },
    /// The user asked to close the window (title-bar button, Alt+F4). The
    /// window stays open: return [`ControlFlow::Exit`] to close it, or keep
    /// returning [`ControlFlow::Continue`] to refuse (for example, to show a
    /// "unsaved changes" prompt first).
    CloseRequested,
}

/// What the per-frame closure asks the event loop to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFlow {
    Continue,
    Exit,
}

/// Per-frame view handed to the closure in [`run`].
pub struct Frame<'a> {
    pub width: u32,
    pub height: u32,
    /// Escape hatch: the raw framebuffer, row-major, `width * height` pixels
    /// of `0x00RRGGBB`. It is cleared to white before each frame and shown
    /// after the closure returns.
    pub pixels: &'a mut [u32],
    /// Events received since the previous frame, in order.
    pub events: &'a [Event],
}

/// Failure to open or run a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// No backend exists for this target yet.
    Unsupported,
    /// The OS refused an operation. The string names the call and its code.
    Os(String),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlatformError::Unsupported => {
                write!(
                    f,
                    "native window backend not implemented on this platform yet"
                )
            }
            PlatformError::Os(msg) => write!(f, "platform error: {msg}"),
        }
    }
}

impl std::error::Error for PlatformError {}

/// Opens a window of `width` x `height` pixels and calls `on_frame` once per
/// frame until it returns [`ControlFlow::Exit`] or the window is closed.
///
/// Blocks the calling thread for the lifetime of the window.
pub fn run(
    title: &str,
    width: u32,
    height: u32,
    on_frame: impl FnMut(Frame<'_>) -> ControlFlow,
) -> Result<(), PlatformError> {
    #[cfg(windows)]
    {
        windows::run(title, width, height, on_frame)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        x11::run(title, width, height, on_frame)
    }
    #[cfg(target_os = "macos")]
    {
        macos::run(title, width, height, on_frame)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (title, width, height, on_frame);
        Err(PlatformError::Unsupported)
    }
}

/// Converts a `0x00RRGGBB` framebuffer to RGBA bytes, for `ImageData`.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn pixels_to_rgba(pixels: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 4);
    for &p in pixels {
        out.push((p >> 16) as u8);
        out.push((p >> 8) as u8);
        out.push(p as u8);
        out.push(255);
    }
    out
}

/// Number of pixels in a `width` x `height` framebuffer.
#[cfg(any(windows, unix, target_arch = "wasm32", test))]
pub(crate) fn pixel_count(width: u32, height: u32) -> usize {
    width as usize * height as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_button_indices_match_the_dom_convention() {
        assert_eq!(MouseButton::Primary.index(), 0);
        assert_eq!(MouseButton::Middle.index(), 1);
        assert_eq!(MouseButton::Secondary.index(), 2);
        assert_eq!(MouseButton::Other(4).index(), 4);
    }

    #[test]
    fn rgba_conversion_puts_channels_in_order_with_opaque_alpha() {
        assert_eq!(pixels_to_rgba(&[0x00FF_8001]), vec![0xFF, 0x80, 0x01, 0xFF]);
    }

    #[test]
    fn pixel_count_is_width_times_height() {
        assert_eq!(pixel_count(640, 480), 307_200);
        assert_eq!(pixel_count(0, 480), 0);
    }

    #[test]
    fn errors_have_readable_messages() {
        assert!(PlatformError::Unsupported
            .to_string()
            .contains("not implemented"));
        assert_eq!(
            PlatformError::Os("RegisterClassW failed".into()).to_string(),
            "platform error: RegisterClassW failed"
        );
    }
}
