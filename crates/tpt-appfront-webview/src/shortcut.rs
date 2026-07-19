//! Global keyboard shortcuts, registered natively and surfaced to the app as
//! high-level shortcut events.
//!
//! Uses [`global_hotkey`] (pairs with tao 0.16, the same windowing stack wry
//! uses). Shortcuts are registered by string id; when pressed, the manager
//! emits a `shortcut:<id>` IPC-style event that [`crate::run`] forwards to the
//! `on_command` closure like any other action.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use std::collections::HashMap;

/// Errors from parsing/registering a shortcut spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutError {
    /// The shortcut string could not be parsed.
    Parse(String),
    /// Registration with the OS failed.
    Register(String),
}

/// Parses a shortcut string like `"Ctrl+Shift+K"` into a [`HotKey`].
///
/// Recognised modifiers: `Ctrl`/`Cmd` (maps to the platform command key),
/// `Alt`, `Shift`, `Super`. The final token is a key name from
/// [`Code`] (e.g. `KeyK`, `Digit1`, `Space`).
pub fn parse_shortcut(spec: &str) -> Result<HotKey, ShortcutError> {
    let mut modifiers = Modifiers::empty();
    let mut key_token: &str = spec;
    for part in spec.split('+') {
        let p = part.trim();
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "cmd" | "command" | "meta" | "super" => modifiers |= Modifiers::SUPER,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            _ => key_token = p,
        }
    }
    let code = parse_code(key_token)
        .ok_or_else(|| ShortcutError::Parse(format!("unknown key `{key_token}`")))?;
    Ok(HotKey::new(Some(modifiers), code))
}

fn parse_code(token: &str) -> Option<Code> {
    // A small, stable subset covering the common keys. `Code` variants are
    // exhaustive; we match the documented names from keyboard-types.
    use Code::*;
    Some(match token.to_ascii_lowercase().as_str() {
        "a" => KeyA, "b" => KeyB, "c" => KeyC, "d" => KeyD, "e" => KeyE, "f" => KeyF,
        "g" => KeyG, "h" => KeyH, "i" => KeyI, "j" => KeyJ, "k" => KeyK, "l" => KeyL,
        "m" => KeyM, "n" => KeyN, "o" => KeyO, "p" => KeyP, "q" => KeyQ, "r" => KeyR,
        "s" => KeyS, "t" => KeyT, "u" => KeyU, "v" => KeyV, "w" => KeyW, "x" => KeyX,
        "y" => KeyY, "z" => KeyZ,
        "0" => Digit0, "1" => Digit1, "2" => Digit2, "3" => Digit3, "4" => Digit4,
        "5" => Digit5, "6" => Digit6, "7" => Digit7, "8" => Digit8, "9" => Digit9,
        "space" => Space, "enter" => Enter, "escape" | "esc" => Escape,
        "tab" => Tab, "backspace" => Backspace, "delete" => Delete,
        "up" => ArrowUp, "down" => ArrowDown, "left" => ArrowLeft, "right" => ArrowRight,
        "f1" => F1, "f2" => F2, "f3" => F3, "f4" => F4, "f5" => F5, "f6" => F6,
        "f7" => F7, "f8" => F8, "f9" => F9, "f10" => F10, "f11" => F11, "f12" => F12,
        "comma" => Comma, "period" => Period, "slash" => Slash, "semicolon" => Semicolon,
        "equal" => Equal, "minus" => Minus, "bracketleft" => BracketLeft,
        "bracketright" => BracketRight, "backquote" => Backquote, "quote" => Quote,
        _ => return None,
    })
}

/// Owns the global-hotkey manager and the id->spec mapping. Shortcut presses
/// are delivered on a receiver the app pumps from its event loop.
pub struct ShortcutRegistry {
    #[allow(dead_code)]
    manager: GlobalHotKeyManager,
    ids: HashMap<u32, String>,
}

impl ShortcutRegistry {
    /// Creates a registry and registers each `(id, spec)` pair.
    pub fn new(specs: &[(String, String)]) -> Result<Self, ShortcutError> {
        let manager =
            GlobalHotKeyManager::new().map_err(|e| ShortcutError::Register(e.to_string()))?;
        let mut ids = HashMap::new();
        for (id, spec) in specs {
            let hotkey = parse_shortcut(spec)?;
            manager
                .register(hotkey)
                .map_err(|e| ShortcutError::Register(e.to_string()))?;
            ids.insert(hotkey.id(), id.clone());
        }
        Ok(ShortcutRegistry { manager, ids })
    }

    /// Returns the receiver that yields [`GlobalHotKeyEvent`]s on press.
    pub fn event_receiver() -> crossbeam_channel::Receiver<GlobalHotKeyEvent> {
        GlobalHotKeyEvent::receiver().clone()
    }

    /// Resolves a hotkey id (from an event) to the app-level shortcut id.
    pub fn resolve(&self, hotkey_id: u32) -> Option<&String> {
        self.ids.get(&hotkey_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_shortcut() {
        let hk = parse_shortcut("Ctrl+Shift+K").unwrap();
        assert!(hk.mods.contains(Modifiers::CONTROL));
        assert!(hk.mods.contains(Modifiers::SHIFT));
        assert_eq!(hk.key, Code::KeyK);
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(parse_shortcut("Ctrl+Zzz").is_err());
    }
}
