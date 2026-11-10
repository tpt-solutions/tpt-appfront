//! macOS backend for the internal window layer (see `platform/mod.rs`).
//!
//! Built on the `objc2` family of crates (MIT or Zlib/Apache/MIT). It is the same
//! shape as the Win32 backend: input events are queued from AppKit callbacks, the
//! frame loop drains the queue before calling the user closure, and the framebuffer
//! is presented by drawing it into the view.
//!
//! The framebuffer is in points (window content units), not device pixels. On a
//! Retina display the picture is therefore upscaled by the backing scale. HiDPI
//! output is a follow-up.
//!
//! Everything here runs on the main thread, which AppKit requires. [`run`] fails
//! with an OS error when it is called from another thread.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSEvent, NSEventMask,
    NSEventModifierFlags, NSGraphicsContext, NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext,
};
use objc2_foundation::{
    NSDate, NSDefaultRunLoopMode, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use super::{pixel_count, ControlFlow, Event, Frame, MouseButton, PlatformError};

/// Background colour used to clear the framebuffer before each frame.
const CLEAR: u32 = 0x00FF_FFFF;

/// Core Graphics flags for a little-endian 32-bit, alpha-skipped-first bitmap.
/// Memory order is B, G, R, unused, which matches the `0x00RRGGBB` pixels.
const BITMAP_INFO_BGRX: u32 = 0x2000 | 6;

/// The framebuffer the view should draw on its next `drawRect`.
#[derive(Clone, Copy)]
struct FrameView {
    pixels: *const u32,
    width: u32,
    height: u32,
}

/// Per-object state, shared between the view, the window delegate and the frame loop.
/// Only the main thread touches it.
#[derive(Default)]
pub(super) struct Ivars {
    events: Rc<RefCell<Vec<Event>>>,
    frame: Cell<Option<FrameView>>,
}

define_class!(
    /// The content view. Draws the framebuffer and turns AppKit events into [`Event`]s.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct CanvasView;

    impl CanvasView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            // Bottom-left origin, as AppKit uses by default. Pointer coordinates are
            // converted to top-left in `pointer_at`.
            false
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.pointer(event, |x, y| Event::PointerMoved { x, y });
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.pointer(event, |x, y| Event::PointerMoved { x, y });
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            self.pointer(event, |x, y| Event::PointerMoved { x, y });
        }

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            self.pointer(event, |x, y| Event::PointerMoved { x, y });
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.button(event, MouseButton::Primary, true);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.button(event, MouseButton::Primary, false);
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.button(event, MouseButton::Secondary, true);
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.button(event, MouseButton::Secondary, false);
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            let button = other_button(event);
            self.button(event, button, true);
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            let button = other_button(event);
            self.button(event, button, false);
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            // Precise (trackpad) deltas are in points; roughly 10 points is one notch.
            // Line deltas are already close to notches. The sign follows macOS's scroll
            // direction setting, so it is not the same convention as Win32.
            let raw = event.scrollingDeltaY();
            let delta = if event.hasPreciseScrollingDeltas() {
                raw / 10.0
            } else {
                raw
            };
            let (x, y) = self.pointer_at(event);
            self.push(Event::Wheel {
                x,
                y,
                delta: delta as f32,
            });
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let flags = event.modifierFlags();
            self.push(Event::KeyDown {
                code: u32::from(event.keyCode()),
                shift: flags.contains(NSEventModifierFlags::Shift),
                ctrl: flags.contains(NSEventModifierFlags::Control),
                alt: flags.contains(NSEventModifierFlags::Option),
            });
            // Printable characters follow as `Text` events. Control characters and the
            // private-use range AppKit uses for function keys are not text.
            if let Some(chars) = event.characters() {
                for ch in chars.to_string().chars() {
                    if !ch.is_control() && !('\u{F700}'..='\u{F8FF}').contains(&ch) {
                        self.push(Event::Text(ch));
                    }
                }
            }
        }
    }
);

impl CanvasView {
    fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        events: Rc<RefCell<Vec<Event>>>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            events,
            frame: Cell::new(None),
        });
        // SAFETY: `initWithFrame:` is NSView's designated initializer, and `this` is a
        // freshly allocated instance of this subclass.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn push(&self, event: Event) {
        self.ivars().events.borrow_mut().push(event);
    }

    /// Converts a window-relative event location to top-left-origin pixels.
    fn pointer_at(&self, event: &NSEvent) -> (i32, i32) {
        // SAFETY: plain AppKit geometry queries on the main thread.
        let location = event.locationInWindow();
        let point = self.convertPoint_fromView(location, None);
        let height = self.bounds().size.height;
        (point.x.floor() as i32, (height - point.y).floor() as i32)
    }

    fn pointer(&self, event: &NSEvent, make: impl FnOnce(i32, i32) -> Event) {
        let (x, y) = self.pointer_at(event);
        self.push(make(x, y));
    }

    fn button(&self, event: &NSEvent, button: MouseButton, down: bool) {
        let (x, y) = self.pointer_at(event);
        self.push(if down {
            Event::PointerDown { x, y, button }
        } else {
            Event::PointerUp { x, y, button }
        });
    }

    /// Draws the framebuffer published by the frame loop into the current context.
    fn draw(&self) {
        let Some(frame) = self.ivars().frame.get() else {
            return;
        };
        if frame.pixels.is_null() || frame.width == 0 || frame.height == 0 {
            return;
        }
        let Some(context) = NSGraphicsContext::currentContext() else {
            return;
        };
        let cg_context = context.CGContext();
        let colour_space = CGColorSpace::new_device_rgb();
        // SAFETY: the buffer holds `width * height` u32 pixels, and the bitmap context
        // only reads them. The image copies what it needs, and both objects are
        // dropped at the end of this function, before the frame loop changes the buffer.
        let bitmap = unsafe {
            CGBitmapContextCreate(
                frame.pixels as *mut c_void,
                frame.width as usize,
                frame.height as usize,
                8,
                frame.width as usize * 4,
                colour_space.as_deref(),
                BITMAP_INFO_BGRX,
            )
        };
        let Some(bitmap) = bitmap else {
            return;
        };
        let Some(image) = CGBitmapContextCreateImage(Some(&bitmap)) else {
            return;
        };
        let rect = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(f64::from(frame.width), f64::from(frame.height)),
        );
        CGContext::draw_image(Some(&cg_context), rect, Some(&image));
    }
}

fn other_button(event: &NSEvent) -> MouseButton {
    match event.buttonNumber() {
        2 => MouseButton::Middle,
        n if n >= 0 => MouseButton::Other(n.min(255) as u8),
        _ => MouseButton::Other(0),
    }
}

define_class!(
    /// Receives window lifecycle callbacks. It refuses to close the window itself, so
    /// the frame closure decides, as on the other backends.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct WindowDelegate;

    unsafe impl NSObjectProtocol for WindowDelegate {}

    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: &NSWindow) -> bool {
            self.ivars().events.borrow_mut().push(Event::CloseRequested);
            false
        }
    }
);

impl WindowDelegate {
    fn new(mtm: MainThreadMarker, events: Rc<RefCell<Vec<Event>>>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            events,
            frame: Cell::new(None),
        });
        // SAFETY: `init` is NSObject's initializer, and `this` is freshly allocated.
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) fn run(
    title: &str,
    width: u32,
    height: u32,
    mut on_frame: impl FnMut(Frame<'_>) -> ControlFlow,
) -> Result<(), PlatformError> {
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        PlatformError::Os("macOS windows must be opened from the main thread".into())
    })?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    app.finishLaunching();

    let events: Rc<RefCell<Vec<Event>>> = Rc::new(RefCell::new(Vec::new()));
    let frame_rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(f64::from(width), f64::from(height)),
    );
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;

    // SAFETY: a fresh window with valid geometry, on the main thread.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame_rect,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // The window must not release itself on close: `Retained` owns it, and a second
    // release would crash.
    // SAFETY: called before the window is shown or closed.
    unsafe { window.setReleasedWhenClosed(false) };

    let view = CanvasView::new(mtm, frame_rect, events.clone());
    let delegate = WindowDelegate::new(mtm, events.clone());
    window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    window.setTitle(&NSString::from_str(title));
    window.setContentView(Some(&view));
    window.setAcceptsMouseMovedEvents(true);
    window.center();
    window.makeFirstResponder(Some(&view));
    window.makeKeyAndOrderFront(None);
    app.activate();

    let result = frame_loop(&app, &view, &events, &mut on_frame);
    window.close();
    result
}

/// Runs frames until the closure exits. Each frame drains queued AppKit events,
/// calls the closure, then draws the framebuffer.
fn frame_loop(
    app: &NSApplication,
    view: &CanvasView,
    events: &Rc<RefCell<Vec<Event>>>,
    on_frame: &mut impl FnMut(Frame<'_>) -> ControlFlow,
) -> Result<(), PlatformError> {
    let mut pixels: Vec<u32> = Vec::new();
    let mut size = (0u32, 0u32);

    loop {
        pump_events(app);

        let bounds = view.bounds().size;
        let w = bounds.width.max(0.0) as u32;
        let h = bounds.height.max(0.0) as u32;
        if w == 0 || h == 0 {
            // Minimised or zero-sized: nothing to paint, keep the loop responsive.
            std::thread::sleep(Duration::from_millis(16));
            continue;
        }
        if (w, h) != size {
            pixels.clear();
            pixels.resize(pixel_count(w, h), CLEAR);
            size = (w, h);
            events.borrow_mut().push(Event::Resized {
                width: w,
                height: h,
            });
        }
        pixels.fill(CLEAR);

        let queued = std::mem::take(&mut *events.borrow_mut());
        let flow = on_frame(Frame {
            width: w,
            height: h,
            pixels: &mut pixels,
            events: &queued,
        });

        view.ivars().frame.set(Some(FrameView {
            pixels: pixels.as_ptr(),
            width: w,
            height: h,
        }));
        // Draws synchronously, so `drawRect` reads the buffer before this returns.
        view.display();
        view.ivars().frame.set(None);

        if flow == ControlFlow::Exit {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

/// Dispatches every pending AppKit event without blocking.
fn pump_events(app: &NSApplication) {
    // SAFETY: standard AppKit event loop calls on the main thread. `distantPast`
    // makes the dequeue non-blocking.
    loop {
        let next = unsafe {
            app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&NSDate::distantPast()),
                NSDefaultRunLoopMode,
                true,
            )
        };
        let Some(event) = next else {
            break;
        };
        app.sendEvent(&event);
    }
    app.updateWindows();
}
