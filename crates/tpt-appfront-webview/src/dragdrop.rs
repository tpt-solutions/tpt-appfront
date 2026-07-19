//! Drag-and-drop file handling.
//!
//! Wires wry's webview file-drop handler into the same event channel the app
//! already pumps: when files are dropped onto the window, a `filedrop` IPC
//! event carrying the dropped paths is forwarded to `on_command`, exactly like
//! a click action. This lets the hosted page react to OS file drags without
//! re-implementing the webview plumbing.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender};

/// A drag-and-drop event delivered to the app.
#[derive(Debug, Clone)]
pub enum DragDropEvent {
    /// Files hovered over the window at `(x, y)`.
    Hovered(Vec<PathBuf>, f64, f64),
    /// Files dropped at `(x, y)`.
    Dropped(Vec<PathBuf>, f64, f64),
    /// A drag was cancelled.
    Cancelled,
}

/// Dispatch channel for drag-and-drop events, polled by the event loop.
#[derive(Clone)]
pub struct DragDropDispatcher {
    tx: Sender<DragDropEvent>,
    rx: Arc<Mutex<Receiver<DragDropEvent>>>,
}

impl Default for DragDropDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DragDropDispatcher {
    /// Creates a new dispatcher.
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<DragDropEvent>();
        DragDropDispatcher {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Returns a cloneable sender for the webview file-drop handler.
    pub fn sender(&self) -> DragDropSender {
        DragDropSender {
            tx: self.tx.clone(),
        }
    }

    /// Returns a receiver the event loop polls (non-blocking).
    pub fn receiver(&self) -> DragDropReceiver {
        DragDropReceiver {
            rx: self.rx.clone(),
        }
    }
}

/// Cloneable sender half.
#[derive(Clone)]
pub struct DragDropSender {
    tx: Sender<DragDropEvent>,
}

impl DragDropSender {
    /// Sends a drag-and-drop event.
    pub fn send(&self, ev: DragDropEvent) {
        let _ = self.tx.send(ev);
    }
}

/// Non-blocking receiver half.
#[derive(Clone)]
pub struct DragDropReceiver {
    rx: Arc<Mutex<Receiver<DragDropEvent>>>,
}

impl DragDropReceiver {
    /// Returns the next queued event, or `None` if empty.
    pub fn try_recv(&self) -> Option<DragDropEvent> {
        self.rx.lock().unwrap().try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_dropped_files() {
        let d = DragDropDispatcher::new();
        d.sender().send(DragDropEvent::Dropped(
            vec![PathBuf::from("/tmp/a.txt")],
            10.0,
            20.0,
        ));
        match d.receiver().try_recv() {
            Some(DragDropEvent::Dropped(paths, x, y)) => {
                assert_eq!(paths.len(), 1);
                assert_eq!((x, y), (10.0, 20.0));
            }
            _ => panic!("expected dropped event"),
        }
    }
}
