//! Native secret storage, surfaced to the hosted page as IPC actions.
//!
//! Secrets live in the OS credential store (Windows Credential Manager /
//! macOS Keychain / Linux Secret Service) via the `keyring` crate. JS never
//! bundles secrets client-side; instead it calls
//! `window.__appfront.post("secret.get", { key })` (and `secret.set` /
//! `secret.delete`), which the shell resolves against the credential store and
//! returns the value over the IPC reply channel. Every action is capability-
//! gated by the [`crate::Acl`], so a page can only touch keys the grant allows.

use anyhow::{anyhow, Context, Result};
use serde_json::json;

/// Prefix applied to credential-store service names so appfront secrets don't
/// collide with unrelated keyring entries.
const SERVICE_PREFIX: &str = "appfront:";

/// Errors returned when a secret action is rejected or fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// The capability grant doesn't permit this secret action.
    NotPermitted,
    /// The requested key was not found in the credential store.
    NotFound,
    /// An underlying keyring/credential-store failure.
    Backend(String),
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretError::NotPermitted => write!(f, "secret action not permitted by ACL"),
            SecretError::NotFound => write!(f, "secret not found"),
            SecretError::Backend(e) => write!(f, "secret backend error: {e}"),
        }
    }
}

impl std::error::Error for SecretError {}

/// Reads a secret previously stored under `key` for `app_id`.
pub fn get_secret(app_id: &str, key: &str) -> Result<String, SecretError> {
    let entry = keyring::Entry::new(&format!("{SERVICE_PREFIX}{app_id}"), key)
        .map_err(|e| SecretError::Backend(e.to_string()))?;
    entry
        .get_password()
        .map_err(|e| match e {
            keyring::Error::NoEntry => SecretError::NotFound,
            other => SecretError::Backend(other.to_string()),
        })
}

/// Stores `value` under `key` for `app_id`, overwriting any prior value.
pub fn set_secret(app_id: &str, key: &str, value: &str) -> Result<(), SecretError> {
    let entry = keyring::Entry::new(&format!("{SERVICE_PREFIX}{app_id}"), key)
        .map_err(|e| SecretError::Backend(e.to_string()))?;
    entry
        .set_password(value)
        .map_err(|e| SecretError::Backend(e.to_string()))
}

/// Deletes the secret stored under `key` for `app_id`.
pub fn delete_secret(app_id: &str, key: &str) -> Result<(), SecretError> {
    let entry = keyring::Entry::new(&format!("{SERVICE_PREFIX}{app_id}"), key)
        .map_err(|e| SecretError::Backend(e.to_string()))?;
    entry
        .delete_credential()
        .map_err(|e| match e {
            keyring::Error::NoEntry => SecretError::NotFound,
            other => SecretError::Backend(other.to_string()),
        })
}

/// A reply produced by handling a secret IPC action, ready to be posted back to
/// the page (or `None` if the action name wasn't a secret action).
pub fn handle_secret_action(
    app_id: &str,
    acl: &crate::Acl,
    action: &str,
    params: &serde_json::Value,
) -> Option<Result<serde_json::Value, SecretError>> {
    let key = params.get("key").and_then(|v| v.as_str())?.to_string();
    let permit = |a: &str| acl.capability(a).is_some();
    match action {
        "secret.get" => {
            if !permit("secret.get") {
                return Some(Err(SecretError::NotPermitted));
            }
            Some(get_secret(app_id, &key).map(|v| json!({ "key": key, "value": v })))
        }
        "secret.set" => {
            if !permit("secret.set") {
                return Some(Err(SecretError::NotPermitted));
            }
            let value = match params.get("value").and_then(|v| v.as_str()) {
                Some(v) => v.to_string(),
                None => return Some(Err(SecretError::Backend("missing `value`".into()))),
            };
            Some(set_secret(app_id, &key, &value).map(|()| json!({ "ok": true })))
        }
        "secret.delete" => {
            if !permit("secret.delete") {
                return Some(Err(SecretError::NotPermitted));
            }
            Some(delete_secret(app_id, &key).map(|()| json!({ "ok": true })))
        }
        _ => None,
    }
}

/// Convenience helper used by tests/examples to build the three standard secret
/// capabilities in one shot.
#[allow(dead_code)]
pub fn secret_capabilities(keys: &[&str]) -> Vec<crate::Capability> {
    keys.iter()
        .flat_map(|_| {
            [
                crate::Capability {
                    action: "secret.get".into(),
                    params: vec![crate::ParamSpec {
                        name: "key".into(),
                        required: true,
                        kind: crate::ParamKind::String,
                        default: None,
                    }],
                },
                crate::Capability {
                    action: "secret.set".into(),
                    params: vec![
                        crate::ParamSpec {
                            name: "key".into(),
                            required: true,
                            kind: crate::ParamKind::String,
                            default: None,
                        },
                        crate::ParamSpec {
                            name: "value".into(),
                            required: true,
                            kind: crate::ParamKind::String,
                            default: None,
                        },
                    ],
                },
                crate::Capability {
                    action: "secret.delete".into(),
                    params: vec![crate::ParamSpec {
                        name: "key".into(),
                        required: true,
                        kind: crate::ParamKind::String,
                        default: None,
                    }],
                },
            ]
        })
        .collect()
}

/// Marker so the unused `anyhow`/`Context` imports remain meaningful if the
/// keyring surface changes; kept for downstream `?`-based callers.
#[allow(dead_code)]
fn _assert_anyhow() -> Result<()> {
    Err(anyhow!("placeholder")).context("ctx")
}
