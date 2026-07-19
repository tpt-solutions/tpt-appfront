//! Deep-link / custom OS URL-scheme handling (runtime registration).
//!
//! Titan can register a custom scheme (e.g. `myapp://`) so that when the OS
//! opens a link of that scheme the app is launched (install-time registration
//! belongs to the installer — see Phase 2) and the URL is delivered to the
//! running instance. This module covers the *runtime* half: registering the
//! scheme with the OS where supported, and dispatching received URLs to the
//! app via a callback.
//!
//! On Windows, registration writes the `HKEY_CURRENT_USER\Software\Classes\<scheme>`
//! keys. On macOS/Linux the equivalent is handled by the bundle/launcher, but
//! the [`DeepLinkDispatcher`] dispatch path is platform-independent.

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender};

/// A dispatcher that buffers received deep-link URLs and lets the app drain
/// them from its event loop via [`DeepLinkDispatcher::receiver`].
#[derive(Clone)]
pub struct DeepLinkDispatcher {
    tx: Sender<String>,
    rx: Arc<Mutex<Receiver<String>>>,
}

impl Default for DeepLinkDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DeepLinkDispatcher {
    /// Creates a dispatcher backed by a shared channel.
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        DeepLinkDispatcher {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Returns a cloneable handle for pushing received URLs (e.g. from an OS
    /// handler or a command-line `argv` URL on launch).
    pub fn sender(&self) -> DeepLinkSender {
        DeepLinkSender {
            tx: self.tx.clone(),
        }
    }

    /// Returns a receiver the event loop should poll (non-blocking) for
    /// incoming URLs.
    pub fn receiver(&self) -> DeepLinkReceiver {
        DeepLinkReceiver {
            rx: self.rx.clone(),
        }
    }

    /// Pushes a received URL into the dispatch channel.
    pub fn push(&self, url: String) {
        let _ = self.tx.send(url);
    }
}

/// Cloneable sender half of the deep-link channel.
#[derive(Clone)]
pub struct DeepLinkSender {
    tx: Sender<String>,
}

impl DeepLinkSender {
    /// Sends a received deep-link URL.
    pub fn send(&self, url: String) -> Result<(), String> {
        self.tx.send(url).map_err(|e| e.to_string())
    }
}

/// Non-blocking receiver half of the deep-link channel.
#[derive(Clone)]
pub struct DeepLinkReceiver {
    rx: Arc<Mutex<Receiver<String>>>,
}

impl DeepLinkReceiver {
    /// Returns the next queued URL, or `None` if the channel is empty.
    pub fn try_recv(&self) -> Option<String> {
        self.rx.lock().unwrap().try_recv().ok()
    }
}

/// Registers `scheme` (without `://`) as a handler for this executable.
///
/// Returns `Ok(())` on success, or an error string if registration failed.
/// On non-Windows platforms this is a no-op success (the launcher/bundle owns
/// registration).
pub fn register_scheme(scheme: &str, executable: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let classes = hkcu
            .open_subkey_with_flags("Software\\Classes", KEY_WRITE)
            .map_err(|e| e.to_string())?;
        let (scheme_key, _) = classes.create_subkey(scheme).map_err(|e| e.to_string())?;
        scheme_key
            .set_value("", &format!("URL:{} protocol", scheme))
            .map_err(|e| e.to_string())?;
        scheme_key
            .set_value("URL Protocol", &"")
            .map_err(|e| e.to_string())?;
        let (cmd, _) = scheme_key
            .create_subkey("shell\\open\\command")
            .map_err(|e| e.to_string())?;
        let exe = executable.to_string_lossy().into_owned();
        cmd.set_value("", &format!("\"{}\" \"%1\"", exe))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (scheme, executable);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatcher_roundtrips_urls() {
        let d = DeepLinkDispatcher::new();
        let sender = d.sender();
        let receiver = d.receiver();
        sender.send("myapp://open/42".into()).unwrap();
        assert_eq!(receiver.try_recv(), Some("myapp://open/42".into()));
        assert_eq!(receiver.try_recv(), None);
    }
}
