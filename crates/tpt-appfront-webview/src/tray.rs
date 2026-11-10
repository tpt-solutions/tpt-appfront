//! System tray icon with a context menu (Windows).
//!
//! Enabled by the `tray` feature. On Windows, [`TrayController`] adds a notification-area icon
//! with `Shell_NotifyIconW`, owned by a hidden message-only window. Right-click, or the keyboard
//! context-menu key, opens the menu. A menu item press reports [`TrayEvent::MenuItem`], and a
//! left click reports [`TrayEvent::Activate`]. The icon is re-added if Explorer restarts.
//!
//! Events are queued by the window procedure and drained by the app's event loop, the same way
//! global shortcuts are. Use [`AppBuilder::with_tray`](crate::AppBuilder::with_tray) to have the
//! app's `on_command` receive them (`tray.menu` with `{"id": ...}`, and `tray.activate`). On
//! macOS and Linux, [`TrayController::new`] returns an error and the app runs without a tray.

/// A single tray menu item. `id` is reported back when the item is clicked.
#[derive(Debug, Clone)]
pub struct TrayMenuItem {
    /// Application-defined id, echoed in [`TrayEvent::MenuItem`].
    pub id: String,
    /// Visible label.
    pub label: String,
    /// If `true`, renders a separator instead of a clickable item.
    pub separator: bool,
    /// If `true`, the item is greyed out.
    pub disabled: bool,
}

/// Events emitted from the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// A menu item was selected.
    MenuItem(String),
    /// The tray icon itself was clicked (left button released).
    Activate,
}

/// A platform tray backend.
pub trait TrayBackend {
    /// Sets the tooltip shown when hovering the icon.
    fn set_tooltip(&mut self, tooltip: &str);
    /// Replaces the context menu.
    fn set_menu(&mut self, items: &[TrayMenuItem]);
    /// Returns the next queued event, if any.
    fn next_event(&mut self) -> Option<TrayEvent>;
}

/// A tray icon the app drives from its event loop. Dropping it removes the icon.
pub struct TrayController {
    backend: Box<dyn TrayBackend + Send>,
}

impl TrayController {
    /// Adds a tray icon with `icon_tooltip`. Returns an error if the icon cannot be created,
    /// and on platforms without a tray backend yet.
    #[cfg(windows)]
    pub fn new(icon_tooltip: &str) -> anyhow::Result<Self> {
        Ok(TrayController {
            backend: Box::new(win::WindowsTray::new(icon_tooltip)?),
        })
    }

    /// Adds a tray icon with `icon_tooltip`. Always fails on this platform.
    #[cfg(not(windows))]
    pub fn new(icon_tooltip: &str) -> anyhow::Result<Self> {
        let _ = icon_tooltip;
        anyhow::bail!("the system tray is not available on this platform yet")
    }

    /// Updates the tooltip.
    pub fn set_tooltip(&mut self, tooltip: &str) {
        self.backend.set_tooltip(tooltip);
    }

    /// Replaces the context menu.
    pub fn set_menu(&mut self, items: &[TrayMenuItem]) {
        self.backend.set_menu(items);
    }

    /// Returns the next queued tray event, if any.
    pub fn next_event(&mut self) -> Option<TrayEvent> {
        self.backend.next_event()
    }
}

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        GetCursorPos, LoadIconW, PostMessageW, RegisterClassW, RegisterWindowMessageW,
        SetForegroundWindow, TrackPopupMenu, HWND_MESSAGE, IDI_APPLICATION, MF_GRAYED,
        MF_SEPARATOR, MF_STRING, TPM_RIGHTBUTTON, WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_LBUTTONUP,
        WM_NULL, WM_RBUTTONUP, WNDCLASSW,
    };

    use super::{TrayBackend, TrayEvent, TrayMenuItem};

    /// Callback message the shell sends for mouse events on the icon.
    const WM_TRAY: u32 = WM_APP + 1;
    /// The icon's id within our window. One icon per window.
    const ICON_UID: u32 = 1;
    /// Menu command ids start here, which keeps them clear of anything else `WM_COMMAND`.
    const FIRST_COMMAND_ID: u32 = 1000;
    const CLASS_NAME: &[u16] = &[
        'T' as u16, 'p' as u16, 't' as u16, 'T' as u16, 'r' as u16, 'a' as u16, 'y' as u16, 0,
    ];

    // Shared state, keyed by window handle so several trays can coexist. The window procedure
    // runs on the thread that owns the window, and so does every caller.
    static EVENTS: Mutex<Vec<(isize, TrayEvent)>> = Mutex::new(Vec::new());
    /// `(window, command id, app menu id)` for the live menu.
    static COMMANDS: Mutex<Vec<(isize, u32, String)>> = Mutex::new(Vec::new());
    /// `(window, menu handle)`, so the window procedure can show it.
    static MENUS: Mutex<Vec<(isize, isize)>> = Mutex::new(Vec::new());
    /// `(window, tooltip)`, used again when the icon is re-added.
    static TIPS: Mutex<Vec<(isize, String)>> = Mutex::new(Vec::new());
    /// The message Explorer broadcasts when it restarts. Zero until registered.
    static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

    /// Encodes `s` as NUL-terminated UTF-16.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Copies `tip` into the fixed-size tooltip buffer, truncating to fit with a NUL.
    fn fill_tip(buffer: &mut [u16; 128], tip: &str) {
        let units: Vec<u16> = tip.encode_utf16().take(buffer.len() - 1).collect();
        buffer[..units.len()].copy_from_slice(&units);
        buffer[units.len()] = 0;
    }

    fn push_event(window: isize, event: TrayEvent) {
        if let Ok(mut events) = EVENTS.lock() {
            events.push((window, event));
        }
    }

    fn command_for(window: isize, command: u32) -> Option<String> {
        let commands = COMMANDS.lock().ok()?;
        commands
            .iter()
            .find(|(w, c, _)| *w == window && *c == command)
            .map(|(_, _, id)| id.clone())
    }

    /// Removes and returns the oldest queued event for `window`.
    fn take_event(window: isize) -> Option<TrayEvent> {
        let mut events = EVENTS.lock().ok()?;
        let index = events.iter().position(|(w, _)| *w == window)?;
        Some(events.remove(index).1)
    }

    fn menu_for(window: isize) -> Option<isize> {
        let menus = MENUS.lock().ok()?;
        menus.iter().find(|(w, _)| *w == window).map(|(_, m)| *m)
    }

    fn tip_for(window: isize) -> String {
        TIPS.lock()
            .ok()
            .and_then(|tips| {
                tips.iter()
                    .find(|(w, _)| *w == window)
                    .map(|(_, t)| t.clone())
            })
            .unwrap_or_default()
    }

    fn notify_data(window: isize, tip: &str, icon: isize) -> NOTIFYICONDATAW {
        // SAFETY: NOTIFYICONDATAW is plain old data, and all-zero is a valid starting value.
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = window;
        data.uID = ICON_UID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_TRAY;
        data.hIcon = icon;
        fill_tip(&mut data.szTip, tip);
        data
    }

    /// Shows the context menu for `window` at the cursor.
    fn show_menu(window: isize) {
        let Some(menu) = menu_for(window) else {
            return;
        };
        let mut point = POINT { x: 0, y: 0 };
        // SAFETY: `point` is a valid out-parameter for the cursor position.
        unsafe { GetCursorPos(&mut point) };
        // SAFETY: the menu and window are live, and this thread owns the window. Without
        // SetForegroundWindow the menu may not dismiss on a click elsewhere. The WM_NULL post
        // is the documented follow-up for the same reason.
        unsafe {
            SetForegroundWindow(window);
            TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                window,
                std::ptr::null(),
            );
            PostMessageW(window, WM_NULL, 0, 0);
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let key = hwnd;
        if msg == WM_TRAY {
            // The mouse message is in the low word of lparam (legacy callback form).
            match (lparam as u32) & 0xFFFF {
                WM_LBUTTONUP => push_event(key, TrayEvent::Activate),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(key),
                _ => {}
            }
            return 0;
        }
        if msg == WM_COMMAND {
            let command = (wparam as u32) & 0xFFFF;
            if let Some(id) = command_for(key, command) {
                push_event(key, TrayEvent::MenuItem(id));
            }
            return 0;
        }
        let taskbar = TASKBAR_CREATED.load(Ordering::Relaxed);
        if taskbar != 0 && msg == taskbar {
            // Explorer restarted and dropped the icon. Add it again.
            let tip = tip_for(key);
            let icon = unsafe { LoadIconW(0, IDI_APPLICATION) };
            let data = notify_data(key, &tip, icon);
            // SAFETY: `data` is a fully initialised NOTIFYICONDATAW for this window.
            unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
            return 0;
        }
        // SAFETY: unhandled messages go to the default procedure, as every window procedure does.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn register_class() {
        // SAFETY: the class name and procedure outlive the call. Registering the class a second
        // time fails, which is harmless because it already exists.
        unsafe {
            let class = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(std::ptr::null()),
                hIcon: 0,
                hCursor: 0,
                hbrBackground: 0,
                lpszMenuName: std::ptr::null(),
                lpszClassName: CLASS_NAME.as_ptr(),
            };
            RegisterClassW(&class);
            TASKBAR_CREATED.store(
                RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
                Ordering::Relaxed,
            );
        }
    }

    /// The Win32 tray backend. It holds only handles, so it is `Send`.
    pub(super) struct WindowsTray {
        window: isize,
        icon: isize,
        menu: isize,
        next_command: u32,
    }

    impl WindowsTray {
        pub(super) fn new(tooltip: &str) -> anyhow::Result<Self> {
            register_class();
            // SAFETY: a message-only window owned by this thread, with `wnd_proc` as its procedure.
            let window = unsafe {
                CreateWindowExW(
                    0,
                    CLASS_NAME.as_ptr(),
                    CLASS_NAME.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    0,
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null(),
                )
            };
            if window == 0 {
                anyhow::bail!("could not create the tray window");
            }
            // SAFETY: loads a stock system icon, which needs no cleanup.
            let icon = unsafe { LoadIconW(0, IDI_APPLICATION) };
            let data = notify_data(window, tooltip, icon);
            // SAFETY: `data` is fully initialised for this window and icon id.
            let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
            if added == 0 {
                // SAFETY: the window was created above and is destroyed once.
                unsafe { DestroyWindow(window) };
                anyhow::bail!(
                    "Shell_NotifyIconW(NIM_ADD) failed; the notification area may be unavailable"
                );
            }
            if let Ok(mut tips) = TIPS.lock() {
                tips.push((window, tooltip.to_string()));
            }
            Ok(WindowsTray {
                window,
                icon,
                menu: 0,
                next_command: FIRST_COMMAND_ID,
            })
        }
    }

    impl TrayBackend for WindowsTray {
        fn set_tooltip(&mut self, tooltip: &str) {
            if let Ok(mut tips) = TIPS.lock() {
                tips.retain(|(w, _)| *w != self.window);
                tips.push((self.window, tooltip.to_string()));
            }
            let mut data = notify_data(self.window, tooltip, self.icon);
            data.uFlags = NIF_TIP;
            // SAFETY: `data` is initialised for this window's icon.
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        }

        fn set_menu(&mut self, items: &[TrayMenuItem]) {
            // SAFETY: the old menu is ours and no longer referenced once replaced.
            unsafe {
                if self.menu != 0 {
                    DestroyMenu(self.menu);
                }
                self.menu = CreatePopupMenu();
            }
            if let Ok(mut commands) = COMMANDS.lock() {
                commands.retain(|(w, _, _)| *w != self.window);
            }
            if let Ok(mut menus) = MENUS.lock() {
                menus.retain(|(w, _)| *w != self.window);
                menus.push((self.window, self.menu));
            }
            for item in items {
                if item.separator {
                    // SAFETY: appends a separator to the menu created above.
                    unsafe { AppendMenuW(self.menu, MF_SEPARATOR, 0, std::ptr::null()) };
                    continue;
                }
                let command = self.next_command;
                self.next_command = self.next_command.wrapping_add(1).max(FIRST_COMMAND_ID);
                if let Ok(mut commands) = COMMANDS.lock() {
                    commands.push((self.window, command, item.id.clone()));
                }
                let flags = MF_STRING | if item.disabled { MF_GRAYED } else { 0 };
                let label = wide(&item.label);
                // SAFETY: `label` is NUL-terminated and outlives the call.
                unsafe { AppendMenuW(self.menu, flags, command as usize, label.as_ptr()) };
            }
        }

        fn next_event(&mut self) -> Option<TrayEvent> {
            take_event(self.window)
        }
    }

    impl Drop for WindowsTray {
        fn drop(&mut self) {
            let data = notify_data(self.window, "", self.icon);
            // SAFETY: removes the icon this backend added. The window is still alive here.
            unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
            // SAFETY: the menu and window are owned by this backend and destroyed once.
            unsafe {
                if self.menu != 0 {
                    DestroyMenu(self.menu);
                }
                DestroyWindow(self.window);
            }
            if let Ok(mut events) = EVENTS.lock() {
                events.retain(|(w, _)| *w != self.window);
            }
            if let Ok(mut commands) = COMMANDS.lock() {
                commands.retain(|(w, _, _)| *w != self.window);
            }
            if let Ok(mut menus) = MENUS.lock() {
                menus.retain(|(w, _)| *w != self.window);
            }
            if let Ok(mut tips) = TIPS.lock() {
                tips.retain(|(w, _)| *w != self.window);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn tooltip_is_truncated_and_nul_terminated() {
            let mut buffer = [0xFFFFu16; 128];
            let long = "x".repeat(300);
            fill_tip(&mut buffer, &long);
            assert_eq!(buffer[126], u16::from(b'x'));
            assert_eq!(buffer[127], 0);
        }

        #[test]
        fn queued_events_are_scoped_to_their_window() {
            // Window handles here are invented, so they cannot clash with a real tray.
            push_event(9_000_001, TrayEvent::Activate);
            push_event(9_000_002, TrayEvent::MenuItem("other".into()));
            assert_eq!(take_event(9_000_001), Some(TrayEvent::Activate));
            assert_eq!(take_event(9_000_001), None);
            // The other window's event is still queued, and is removed here so it does not leak.
            assert_eq!(
                take_event(9_000_002),
                Some(TrayEvent::MenuItem("other".into()))
            );
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Creates a real tray icon, opens a menu through the public API, and removes the icon
    /// again. Passes in a desktop session. Skips with a note if the notification area is
    /// unavailable, for example on a headless CI runner.
    #[test]
    fn tray_icon_can_be_created_and_its_menu_replaced() {
        let mut tray = match TrayController::new("tray test") {
            Ok(tray) => tray,
            Err(e) => {
                eprintln!("skipping: {e}");
                return;
            }
        };
        tray.set_menu(&[
            TrayMenuItem {
                id: "a".into(),
                label: "Open".into(),
                separator: false,
                disabled: false,
            },
            TrayMenuItem {
                id: "sep".into(),
                label: String::new(),
                separator: true,
                disabled: false,
            },
            TrayMenuItem {
                id: "b".into(),
                label: "Quit".into(),
                separator: false,
                disabled: true,
            },
        ]);
        tray.set_tooltip("tray test 2");
        assert_eq!(tray.next_event(), None);
    }
}
