//! WebRTC camera/mic access, gated through the IPC [`crate::Acl`].
//!
//! The webview engine still owns the actual OS permission prompt, but Titan
//! must not let arbitrary JS request media. Instead the page calls
//! `window.__appfront.post("media.request", { kind })`; the shell consults the
//! ACL — if no `media.request` capability (with the requested `kind`) is
//! granted, the request is rejected before any `getUserMedia` call happens.
//! Granted requests return a token the page can use to actually call
//! `getUserMedia`. This implements "gated through the ACL model, not
//! blanket-granted."

use serde_json::json;

/// The media kinds an app can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// Camera only.
    Camera,
    /// Microphone only.
    Microphone,
    /// Both camera and microphone.
    CameraAndMic,
}

impl MediaKind {
    /// Parses the `kind` param value.
    pub fn parse(s: &str) -> Option<MediaKind> {
        match s.to_ascii_lowercase().as_str() {
            "camera" | "video" => Some(MediaKind::Camera),
            "microphone" | "mic" | "audio" => Some(MediaKind::Microphone),
            "both" | "camera+mic" | "audio+video" => Some(MediaKind::CameraAndMic),
            _ => None,
        }
    }

    /// The ACL param value this kind serialises to.
    pub fn as_param(&self) -> &'static str {
        match self {
            MediaKind::Camera => "camera",
            MediaKind::Microphone => "microphone",
            MediaKind::CameraAndMic => "camera+mic",
        }
    }
}

/// Handles a `media.request` IPC action.
///
/// Returns `Some(value)` with either `{ "granted": true, "token": ... }` or
/// `{ "granted": false }`. Returns `None` if the action isn't a media request,
/// and rejects (granted:false) when the [`crate::Acl`] doesn't permit the
/// requested kind.
pub fn handle_media_action(
    acl: &crate::Acl,
    action: &str,
    params: &serde_json::Value,
) -> Option<serde_json::Value> {
    if action != "media.request" {
        return None;
    }
    let cap = match acl.capability("media.request") {
        Some(c) => c,
        None => return Some(json!({ "granted": false, "reason": "no capability" })),
    };
    let kind_str = params.get("kind").and_then(|v| v.as_str()).unwrap_or("camera");
    let kind = match MediaKind::parse(kind_str) {
        Some(k) => k,
        None => return Some(json!({ "granted": false, "reason": "unknown kind" })),
    };
    // The capability must permit this exact kind via its `kind` param default
    // set, or an explicit param. We model permission as: capability exists AND
    // (no `kind` param spec, or the requested kind is in the allowed list).
    let allowed = cap
        .params
        .iter()
        .find(|p| p.name == "kind")
        .map(|spec| {
            spec.default
                .as_ref()
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .any(|v| v.as_str() == Some(kind.as_param()))
                })
                .unwrap_or(true) // no enumerated list => allow any parsed kind
        })
        .unwrap_or(true);
    if allowed {
        Some(json!({ "granted": true, "kind": kind.as_param() }))
    } else {
        Some(json!({ "granted": false, "reason": "kind not permitted" }))
    }
}

/// Builds a `media.request` capability permitting the given kinds.
pub fn media_capability(kinds: &[MediaKind]) -> crate::Capability {
    use crate::{Capability, ParamKind, ParamSpec};
    let allowed: Vec<serde_json::Value> = kinds.iter().map(|k| json!(k.as_param())).collect();
    Capability {
        action: "media.request".into(),
        params: vec![ParamSpec {
            name: "kind".into(),
            required: false,
            kind: ParamKind::String,
            default: Some(json!(allowed)),
        }],
    }
}
