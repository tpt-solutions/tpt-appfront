//! Global keyboard shortcut specs.
//!
//! System-wide shortcuts. On Windows, [`ShortcutRegistry`] registers each spec with
//! `RegisterHotKey` against a hidden message-only window, and the app polls
//! [`ShortcutRegistry::take_pressed`] from its event loop. Elsewhere, registration
//! fails with a clear error and the app runs without shortcuts. The `global-hotkey`
//! crate is not used: it pulls in `x11rb` and the banned `gethostname`.

use std::fmt;

/// Errors from parsing or registering a shortcut spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutError {
    /// The shortcut string could not be parsed.
    Parse(String),
    /// Registration with the OS failed, or no backend is available.
    Register(String),
}

impl fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShortcutError::Parse(msg) => write!(f, "shortcut parse error: {msg}"),
            ShortcutError::Register(msg) => write!(f, "shortcut registration error: {msg}"),
        }
    }
}

impl std::error::Error for ShortcutError {}

/// A parsed shortcut: a set of modifiers and one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    /// `Ctrl` (or `Cmd`/`Super`, see [`parse_shortcut`]) is folded into `meta`.
    pub ctrl: bool,
    /// `Alt` / `Option`.
    pub alt: bool,
    /// `Shift`.
    pub shift: bool,
    /// `Cmd` / `Meta` / `Super`.
    pub meta: bool,
    /// The normalized (lowercase) key name, such as `k`, `1`, `space`, or `f5`.
    pub key: String,
}

/// Key names accepted in a spec, after lowercasing. Kept to a stable subset of
/// common keys so specs are portable across platforms.
const KEYS: &[&str] = &[
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "space",
    "enter",
    "escape",
    "esc",
    "tab",
    "backspace",
    "delete",
    "up",
    "down",
    "left",
    "right",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
    "comma",
    "period",
    "slash",
    "semicolon",
    "equal",
    "minus",
    "bracketleft",
    "bracketright",
    "backquote",
    "quote",
];

/// Parses a shortcut string like `"Ctrl+Shift+K"`.
///
/// Recognised modifiers: `Ctrl`/`Control`, `Alt`/`Option`, `Shift`, and
/// `Cmd`/`Command`/`Meta`/`Super`. The last token is the key. A spec with no key,
/// two keys, or an unknown key is rejected.
pub fn parse_shortcut(spec: &str) -> Result<Shortcut, ShortcutError> {
    let mut shortcut = Shortcut {
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
        key: String::new(),
    };
    for part in spec.split('+') {
        let token = part.trim();
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => shortcut.ctrl = true,
            "alt" | "option" => shortcut.alt = true,
            "shift" => shortcut.shift = true,
            "cmd" | "command" | "meta" | "super" => shortcut.meta = true,
            key => {
                if !shortcut.key.is_empty() {
                    return Err(ShortcutError::Parse(format!(
                        "more than one key in `{spec}`"
                    )));
                }
                if !KEYS.contains(&key) {
                    return Err(ShortcutError::Parse(format!("unknown key `{token}`")));
                }
                shortcut.key = key.to_string();
            }
        }
    }
    if shortcut.key.is_empty() {
        return Err(ShortcutError::Parse(format!("no key in `{spec}`")));
    }
    Ok(shortcut)
}

/// System-wide shortcuts for one app. Windows registers them with `RegisterHotKey`
/// against a hidden message-only window. Other platforms return
/// [`ShortcutError::Register`]. Dropping the registry unregisters every shortcut.
pub struct ShortcutRegistry {
    inner: imp::Registry,
}

impl ShortcutRegistry {
    /// Parses and registers each `(id, spec)` pair. Specs are checked first, so a
    /// typo returns [`ShortcutError::Parse`] before anything is registered. A
    /// combination already taken by another app returns
    /// [`ShortcutError::Register`].
    pub fn new(specs: &[(String, String)]) -> Result<Self, ShortcutError> {
        let mut entries = Vec::with_capacity(specs.len());
        for (id, spec) in specs {
            let shortcut = parse_shortcut(spec)?;
            let vk = shortcut.virtual_key().ok_or_else(|| {
                ShortcutError::Parse(format!(
                    "key `{}` has no key code on this platform",
                    shortcut.key
                ))
            })?;
            entries.push(imp::Entry {
                id: id.clone(),
                modifiers: shortcut.modifier_bits(),
                key: vk,
            });
        }
        Ok(ShortcutRegistry {
            inner: imp::Registry::register(&entries)?,
        })
    }

    /// Returns the ids of the shortcuts pressed since the last call, in press order.
    pub fn take_pressed(&mut self) -> Vec<String> {
        self.inner.take_pressed()
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, HWND_MESSAGE, WM_HOTKEY,
        WNDCLASSW,
    };

    use super::ShortcutError;

    pub(super) struct Entry {
        pub id: String,
        pub modifiers: u32,
        pub key: u32,
    }

    /// Presses seen by the window procedure: `(window handle, hotkey id)`. Every
    /// registry filters on its own handle.
    static PRESSED: Mutex<Vec<(isize, u32)>> = Mutex::new(Vec::new());

    const CLASS_NAME: &[u16] = &[
        'T' as u16, 'p' as u16, 't' as u16, 'S' as u16, 'h' as u16, 'o' as u16, 'r' as u16,
        't' as u16, 'c' as u16, 'u' as u16, 't' as u16, 0,
    ];

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_HOTKEY {
            if let Ok(mut pressed) = PRESSED.lock() {
                pressed.push((hwnd, wparam as u32));
            }
            return 0;
        }
        // SAFETY: forwarding unhandled messages to the default procedure is what
        // every window procedure does.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn register_class() {
        // SAFETY: the class name and procedure outlive the call. Registering the same
        // class again fails, which is harmless: it is already there.
        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());
            let class = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: 0,
                hCursor: 0,
                hbrBackground: 0,
                lpszMenuName: std::ptr::null(),
                lpszClassName: CLASS_NAME.as_ptr(),
            };
            RegisterClassW(&class);
        }
    }

    pub(super) struct Registry {
        hwnd: HWND,
        /// `(hotkey id, app shortcut id)`. The hotkey id is the index plus one.
        ids: Vec<(u32, String)>,
    }

    impl Registry {
        pub(super) fn register(entries: &[Entry]) -> Result<Self, ShortcutError> {
            register_class();
            // SAFETY: a message-only window owned by this thread. Its procedure is
            // `wnd_proc`, which only touches the static queue.
            let hwnd = unsafe {
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
            if hwnd == 0 {
                return Err(ShortcutError::Register(
                    "could not create the shortcut window".into(),
                ));
            }
            let mut registry = Registry {
                hwnd,
                ids: Vec::with_capacity(entries.len()),
            };
            for (index, entry) in entries.iter().enumerate() {
                let id = index as u32 + 1;
                // SAFETY: `hwnd` is a live window owned by this registry, and the key and
                // modifiers came from a parsed spec.
                let ok = unsafe {
                    RegisterHotKey(hwnd, id as i32, entry.modifiers | MOD_NOREPEAT, entry.key)
                };
                if ok == 0 {
                    // SAFETY: GetLastError reads the calling thread's last error code.
                    let code = unsafe { GetLastError() };
                    // `registry` drops here and unregisters what was already registered.
                    return Err(ShortcutError::Register(format!(
                        "could not register `{}`: it may be in use by another app (Windows error {code})",
                        entry.id
                    )));
                }
                registry.ids.push((id, entry.id.clone()));
            }
            Ok(registry)
        }

        /// Delivers a press to this registry's window, as if the user pressed the key.
        #[cfg(test)]
        pub(super) fn post_test_press(&self, id: u32) {
            // SAFETY: `hwnd` is this registry's live window. SendMessageW runs `wnd_proc`
            // on this thread and returns once it has.
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(
                    self.hwnd,
                    WM_HOTKEY,
                    id as usize,
                    0,
                )
            };
        }

        pub(super) fn take_pressed(&mut self) -> Vec<String> {
            let Ok(mut pressed) = PRESSED.lock() else {
                return Vec::new();
            };
            let mine = self.hwnd;
            let mut out = Vec::new();
            pressed.retain(|(owner, id)| {
                if *owner != mine {
                    return true;
                }
                if let Some((_, name)) = self.ids.iter().find(|(hk, _)| hk == id) {
                    out.push(name.clone());
                }
                false
            });
            out
        }
    }

    impl Drop for Registry {
        fn drop(&mut self) {
            for (id, _) in &self.ids {
                // SAFETY: unregisters the hotkeys this registry registered on its own window.
                unsafe { UnregisterHotKey(self.hwnd, *id as i32) };
            }
            // SAFETY: the window was created by this registry and is destroyed once.
            unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::ShortcutError;

    // The fields are only read by the Windows backend. Registration fails here, so
    // nothing reads them on this platform.
    #[allow(dead_code)]
    pub(super) struct Entry {
        pub id: String,
        pub modifiers: u32,
        pub key: u32,
    }

    pub(super) struct Registry;

    impl Registry {
        pub(super) fn register(_entries: &[Entry]) -> Result<Self, ShortcutError> {
            Err(ShortcutError::Register(
                "global shortcuts are not available on this platform yet".into(),
            ))
        }

        pub(super) fn take_pressed(&mut self) -> Vec<String> {
            Vec::new()
        }
    }
}

impl Shortcut {
    /// The Windows virtual-key code for this shortcut's key. Letters and digits map
    /// to their ASCII codes, and punctuation uses the US-layout OEM codes.
    pub fn virtual_key(&self) -> Option<u32> {
        key_virtual_code(&self.key)
    }

    /// The modifier flags in `RegisterHotKey` form: Alt = 1, Ctrl = 2, Shift = 4,
    /// Win (`Cmd`/`Meta`/`Super`) = 8.
    pub fn modifier_bits(&self) -> u32 {
        (self.alt as u32)
            | ((self.ctrl as u32) << 1)
            | ((self.shift as u32) << 2)
            | ((self.meta as u32) << 3)
    }
}

fn key_virtual_code(key: &str) -> Option<u32> {
    if let [c] = key.as_bytes() {
        return match c {
            b'a'..=b'z' => Some(u32::from(c.to_ascii_uppercase())),
            b'0'..=b'9' => Some(u32::from(*c)),
            _ => None,
        };
    }
    Some(match key {
        "space" => 0x20,
        "enter" => 0x0D,
        "escape" | "esc" => 0x1B,
        "tab" => 0x09,
        "backspace" => 0x08,
        "delete" => 0x2E,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "comma" => 0xBC,
        "period" => 0xBE,
        "slash" => 0xBF,
        "semicolon" => 0xBA,
        "equal" => 0xBB,
        "minus" => 0xBD,
        "bracketleft" => 0xDB,
        "bracketright" => 0xDD,
        "backquote" => 0xC0,
        "quote" => 0xDE,
        f if f.starts_with('f') => {
            let n: u32 = f[1..].parse().ok()?;
            if (1..=12).contains(&n) {
                0x6F + n
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_shortcut() {
        let s = parse_shortcut("Ctrl+Shift+K").unwrap();
        assert!(s.ctrl && s.shift && !s.alt && !s.meta);
        assert_eq!(s.key, "k");
    }

    #[test]
    fn parses_meta_alias_and_digit_key() {
        let s = parse_shortcut("Command+Option+1").unwrap();
        assert!(s.meta && s.alt && !s.ctrl && !s.shift);
        assert_eq!(s.key, "1");
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(matches!(
            parse_shortcut("Ctrl+Zzz"),
            Err(ShortcutError::Parse(_))
        ));
    }

    #[test]
    fn rejects_modifiers_only_and_two_keys() {
        assert!(parse_shortcut("Ctrl+Shift").is_err());
        assert!(parse_shortcut("Ctrl+K+L").is_err());
    }

    #[test]
    fn invalid_spec_is_rejected_before_registering() {
        let bad = vec![("save".to_string(), "Ctrl+Zzz".to_string())];
        assert!(matches!(
            ShortcutRegistry::new(&bad),
            Err(ShortcutError::Parse(_))
        ));
    }
}

#[cfg(test)]
mod key_code_tests {
    use super::*;

    fn shortcut(spec: &str) -> Shortcut {
        parse_shortcut(spec).unwrap()
    }

    #[test]
    fn letters_digits_and_function_keys_map_to_virtual_keys() {
        assert_eq!(shortcut("Ctrl+K").virtual_key(), Some(0x4B));
        assert_eq!(shortcut("Ctrl+1").virtual_key(), Some(0x31));
        assert_eq!(shortcut("F5").virtual_key(), Some(0x74));
        assert_eq!(shortcut("F12").virtual_key(), Some(0x7B));
        assert_eq!(shortcut("Escape").virtual_key(), Some(0x1B));
    }

    #[test]
    fn modifier_bits_follow_register_hotkey_flags() {
        assert_eq!(shortcut("Ctrl+K").modifier_bits(), 2);
        assert_eq!(shortcut("Alt+K").modifier_bits(), 1);
        assert_eq!(shortcut("Shift+K").modifier_bits(), 4);
        assert_eq!(shortcut("Cmd+K").modifier_bits(), 8);
        assert_eq!(shortcut("Ctrl+Alt+Shift+K").modifier_bits(), 7);
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    /// Registers a rarely used combination, then simulates its press by sending
    /// `WM_HOTKEY` to the registry's window. This checks the window procedure and the
    /// queue without a real key press. The registration itself can fail if another app
    /// already owns the combination. The test then reports that and passes.
    #[test]
    fn simulated_press_is_reported_once_by_id() {
        let specs = vec![("secret".to_string(), "Ctrl+Alt+Shift+F12".to_string())];
        let mut registry = match ShortcutRegistry::new(&specs) {
            Ok(registry) => registry,
            Err(e) => {
                eprintln!("skipping: could not register Ctrl+Alt+Shift+F12 ({e})");
                return;
            }
        };
        assert!(registry.take_pressed().is_empty());
        registry.inner.post_test_press(1);
        assert_eq!(registry.take_pressed(), vec!["secret".to_string()]);
        assert!(registry.take_pressed().is_empty());
    }
}
