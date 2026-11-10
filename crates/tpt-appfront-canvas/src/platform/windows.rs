//! Win32 backend for the internal window layer (see `platform/mod.rs`).
//!
//! The window procedure only queues [`Event`]s; the frame loop drains them
//! before calling the user closure. The framebuffer is presented with
//! `StretchDIBits` as a top-down 32-bit BGRX DIB, which matches the
//! `0x00RRGGBB` pixel layout exposed to callers.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null;
use std::time::Duration;

use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    GetDC, ReleaseDC, ScreenToClient, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, RGBQUAD, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetWindowLongPtrW, PeekMessageW, PostQuitMessage, RegisterClassW,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, GWLP_USERDATA, MSG, PM_REMOVE, SW_SHOW, WM_CHAR, WM_CLOSE, WM_DESTROY,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCCREATE, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SIZE, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};

use super::{pixel_count, ControlFlow, Event, Frame, MouseButton, PlatformError};

/// Window class name, UTF-16, nul-terminated.
const CLASS_NAME: &[u16] = &[
    b't' as u16,
    b'p' as u16,
    b't' as u16,
    b'_' as u16,
    b'c' as u16,
    b'a' as u16,
    b'n' as u16,
    b'v' as u16,
    b'a' as u16,
    b's' as u16,
    0,
];

/// Background colour used to clear the framebuffer before each frame.
const CLEAR: u32 = 0x00FF_FFFF;

/// Per-window state shared with the window procedure through
/// `GWLP_USERDATA`. Lives on the stack of [`run`], which outlives the window.
struct State {
    events: Vec<Event>,
    /// High UTF-16 surrogate waiting for its low half in a following `WM_CHAR`.
    pending_high: Option<u16>,
}

pub(super) fn run(
    title: &str,
    width: u32,
    height: u32,
    mut on_frame: impl FnMut(Frame<'_>) -> ControlFlow,
) -> Result<(), PlatformError> {
    let mut state = State {
        events: Vec::new(),
        pending_high: None,
    };

    // SAFETY: plain Win32 calls with valid, nul-terminated UTF-16 strings and
    // pointers to locals that live until the window is destroyed below.
    unsafe {
        let instance = GetModuleHandleW(null());
        if instance == 0 {
            return Err(os_error("GetModuleHandleW"));
        }

        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: 0,
            hCursor: 0,
            hbrBackground: 0,
            lpszMenuName: null(),
            lpszClassName: CLASS_NAME.as_ptr(),
        };
        // Registering twice (a second `run` in the same process) fails with
        // ERROR_CLASS_ALREADY_EXISTS; that is fine, CreateWindowExW below
        // reports a real failure if the class is genuinely missing.
        RegisterClassW(&class);

        // Ask for a client area of `width` x `height`, not an outer window size.
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };
        AdjustWindowRect(&mut rect, WS_OVERLAPPEDWINDOW, 0);

        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = CreateWindowExW(
            0,
            CLASS_NAME.as_ptr(),
            title_w.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            0,
            0,
            instance,
            &mut state as *mut State as *const c_void,
        );
        if hwnd == 0 {
            return Err(os_error("CreateWindowExW"));
        }
        ShowWindow(hwnd, SW_SHOW);

        let result = frame_loop(hwnd, &mut state, &mut on_frame);
        DestroyWindow(hwnd);
        drain_messages();
        result
    }
}

/// Runs frames until the closure exits or the window is destroyed.
///
/// # Safety
/// `hwnd` must be a live window created with `state` as its creation data.
unsafe fn frame_loop(
    hwnd: HWND,
    state: &mut State,
    on_frame: &mut impl FnMut(Frame<'_>) -> ControlFlow,
) -> Result<(), PlatformError> {
    let mut pixels: Vec<u32> = Vec::new();
    let mut size = (0u32, 0u32);

    loop {
        if pump_messages() {
            return Ok(());
        }

        let (w, h) = client_size(hwnd)?;
        if w == 0 || h == 0 {
            // Minimised: nothing to paint, keep the loop responsive.
            std::thread::sleep(Duration::from_millis(16));
            continue;
        }
        if (w, h) != size {
            pixels.clear();
            pixels.resize(pixel_count(w, h), CLEAR);
            size = (w, h);
        }
        pixels.fill(CLEAR);

        let events = std::mem::take(&mut state.events);
        let flow = on_frame(Frame {
            width: w,
            height: h,
            pixels: &mut pixels,
            events: &events,
        });

        present(hwnd, w, h, &pixels)?;

        if flow == ControlFlow::Exit {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

/// Dispatches queued messages. Returns `true` when `WM_QUIT` arrives.
///
/// # Safety
/// Must be called on the thread that created the window.
unsafe fn pump_messages() -> bool {
    let mut msg: MSG = std::mem::zeroed();
    while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
        if msg.message == WM_QUIT {
            return true;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    false
}

/// Consumes any remaining messages, including the `WM_QUIT` that
/// `WM_DESTROY` posts, so it doesn't leak into a later `run` call.
///
/// # Safety
/// Must be called on the thread that created the window.
unsafe fn drain_messages() {
    let mut msg: MSG = std::mem::zeroed();
    while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {}
}

/// # Safety
/// `hwnd` must be a live window.
unsafe fn client_size(hwnd: HWND) -> Result<(u32, u32), PlatformError> {
    let mut rect: RECT = std::mem::zeroed();
    if GetClientRect(hwnd, &mut rect) == 0 {
        return Err(os_error("GetClientRect"));
    }
    Ok((
        (rect.right - rect.left).max(0) as u32,
        (rect.bottom - rect.top).max(0) as u32,
    ))
}

/// Copies the framebuffer to the window's device context.
///
/// # Safety
/// `hwnd` must be a live window and `pixels.len() == w * h`.
unsafe fn present(hwnd: HWND, w: u32, h: u32, pixels: &[u32]) -> Result<(), PlatformError> {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            // Negative height: rows are stored top-down, like the framebuffer.
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }],
    };
    let hdc = GetDC(hwnd);
    if hdc == 0 {
        return Err(os_error("GetDC"));
    }
    StretchDIBits(
        hdc,
        0,
        0,
        w as i32,
        h as i32,
        0,
        0,
        w as i32,
        h as i32,
        pixels.as_ptr() as *const c_void,
        &bmi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
    ReleaseDC(hwnd, hdc);
    Ok(())
}

/// Window procedure: queues input events and handles lifecycle messages.
///
/// # Safety
/// Called by the system with a valid `hwnd`; `GWLP_USERDATA` holds a pointer to
/// a live [`State`] once `WM_NCCREATE` has run.
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return 0;
    }

    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if !state_ptr.is_null() {
        let state = &mut *state_ptr;
        let x = (lparam as u32 & 0xffff) as u16 as i16 as i32;
        let y = ((lparam as u32 >> 16) & 0xffff) as u16 as i16 as i32;
        match msg {
            WM_MOUSEMOVE => state.events.push(Event::PointerMoved { x, y }),
            WM_LBUTTONDOWN => state.events.push(Event::PointerDown {
                x,
                y,
                button: MouseButton::Primary,
            }),
            WM_LBUTTONUP => state.events.push(Event::PointerUp {
                x,
                y,
                button: MouseButton::Primary,
            }),
            WM_RBUTTONDOWN => state.events.push(Event::PointerDown {
                x,
                y,
                button: MouseButton::Secondary,
            }),
            WM_RBUTTONUP => state.events.push(Event::PointerUp {
                x,
                y,
                button: MouseButton::Secondary,
            }),
            WM_MBUTTONDOWN => state.events.push(Event::PointerDown {
                x,
                y,
                button: MouseButton::Middle,
            }),
            WM_MBUTTONUP => state.events.push(Event::PointerUp {
                x,
                y,
                button: MouseButton::Middle,
            }),
            WM_MOUSEWHEEL => {
                // The wheel reports screen coordinates; convert to the client area.
                let mut point = POINT { x, y };
                ScreenToClient(hwnd, &mut point);
                // One notch is 120 units; the high word of wparam is signed.
                let units = ((wparam as u32 >> 16) & 0xffff) as u16 as i16;
                state.events.push(Event::Wheel {
                    x: point.x,
                    y: point.y,
                    delta: f32::from(units) / 120.0,
                });
            }
            WM_KEYDOWN => state.events.push(Event::KeyDown {
                code: wparam as u32,
                shift: key_down(VK_SHIFT),
                ctrl: key_down(VK_CONTROL),
                alt: key_down(VK_MENU),
            }),
            WM_CHAR => push_char(state, wparam as u16),
            WM_SIZE => state.events.push(Event::Resized {
                width: (lparam as u32 & 0xffff),
                height: ((lparam as u32 >> 16) & 0xffff),
            }),
            WM_CLOSE => {
                // Don't destroy here: the closure decides, via its return
                // value, whether the window closes.
                state.events.push(Event::CloseRequested);
                return 0;
            }
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Turns one UTF-16 code unit from `WM_CHAR` into a `Text` event, joining
/// surrogate pairs across messages.
fn push_char(state: &mut State, unit: u16) {
    let ch = if (0xD800..=0xDBFF).contains(&unit) {
        state.pending_high = Some(unit);
        return;
    } else if (0xDC00..=0xDFFF).contains(&unit) {
        match state.pending_high.take() {
            Some(high) => char::decode_utf16([high, unit]).next().and_then(Result::ok),
            None => None,
        }
    } else {
        state.pending_high = None;
        char::from_u32(u32::from(unit))
    };
    if let Some(ch) = ch {
        // Control characters (Backspace, Enter, Tab, Escape) arrive as WM_CHAR
        // too; they are reported as key presses, not text.
        if !ch.is_control() {
            state.events.push(Event::Text(ch));
        }
    }
}

/// Builds a [`PlatformError::Os`] from the calling thread's last Win32 error.
///
/// # Safety
/// Reads thread-local error state; call directly after the failing call.
unsafe fn os_error(call: &str) -> PlatformError {
    PlatformError::Os(format!("{call} failed (error {})", GetLastError()))
}

/// Whether the virtual key `vk` is currently held down.
fn key_down(vk: u16) -> bool {
    // SAFETY: GetKeyState only reads the calling thread's input state.
    unsafe { GetKeyState(i32::from(vk)) as u16 & 0x8000 != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_name_is_nul_terminated_utf16() {
        assert_eq!(CLASS_NAME.last(), Some(&0));
        let text = String::from_utf16(&CLASS_NAME[..CLASS_NAME.len() - 1]).unwrap();
        assert_eq!(text, "tpt_canvas");
    }

    #[test]
    fn surrogate_pairs_join_across_messages() {
        let mut state = State {
            events: Vec::new(),
            pending_high: None,
        };
        // U+1F600 (grinning face) is D83D DE00 in UTF-16.
        push_char(&mut state, 0xD83D);
        assert!(state.events.is_empty());
        push_char(&mut state, 0xDE00);
        assert_eq!(state.events, vec![Event::Text('\u{1F600}')]);
    }

    #[test]
    fn control_characters_are_not_text() {
        let mut state = State {
            events: Vec::new(),
            pending_high: None,
        };
        push_char(&mut state, 0x08); // Backspace
        push_char(&mut state, 0x0D); // Enter
        assert!(state.events.is_empty());
        push_char(&mut state, b'a' as u16);
        assert_eq!(state.events, vec![Event::Text('a')]);
    }
}
