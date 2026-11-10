//! Minimal HTTP client for the `generate --llm` providers, backed by the system `curl`.
//!
//! Shelling out keeps TLS out of the dependency tree (no `ring`, `rustls` or OpenSSL
//! crates), so the workspace's license policy holds. Requests are described to `curl`
//! through a config file piped on stdin, which keeps API keys and request bodies out
//! of the process argument list.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use serde::Serialize;

/// Per-request ceiling, in seconds. Completions can take a while, so this is generous.
const MAX_TIME_SECS: &str = "120";

/// A completed HTTP exchange: the status code and the body as text.
#[derive(Debug)]
pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) body: String,
}

/// `POST` a JSON body with the given headers.
pub(crate) fn post_json<T: Serialize>(
    url: &str,
    headers: &[(&str, &str)],
    body: &T,
) -> anyhow::Result<Response> {
    let json = serde_json::to_string(body).context("failed to serialize the request body")?;
    run("POST", url, headers, Some(&json))
}

/// `GET` a URL with the given headers.
pub(crate) fn get(url: &str, headers: &[(&str, &str)]) -> anyhow::Result<Response> {
    run("GET", url, headers, None)
}

fn run(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> anyhow::Result<Response> {
    let mut config = String::new();
    push_option(&mut config, "url", url);
    push_option(&mut config, "request", method);
    for (name, value) in headers {
        push_option(&mut config, "header", &format!("{name}: {value}"));
    }
    if let Some(body) = body {
        push_option(&mut config, "data-raw", body);
    }

    let mut child = Command::new("curl")
        .args([
            "--config",
            "-",
            "--silent",
            "--show-error",
            "--max-time",
            MAX_TIME_SECS,
            // Appends the status code on its own line after the body.
            "--write-out",
            "\n%{http_code}",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(
            "could not run `curl`; install it (it ships with Windows 10+, macOS and most \
             Linux distributions) to use `generate --llm`",
        )?;

    // Write the config, then drop stdin so curl sees end-of-file and starts the request.
    child
        .stdin
        .take()
        .context("curl stdin was not piped")?
        .write_all(config.as_bytes())
        .context("failed to send the request to curl")?;

    let output = child
        .wait_with_output()
        .context("failed to read curl's output")?;
    if !output.status.success() {
        bail!(
            "request to {url} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_output(&String::from_utf8_lossy(&output.stdout))
        .with_context(|| format!("unexpected response from {url}"))
}

/// Splits curl's `--write-out` output into the body and the trailing status code.
fn parse_output(text: &str) -> anyhow::Result<Response> {
    let (body, code) = text
        .rsplit_once('\n')
        .context("curl output had no status line")?;
    let status: u16 = code
        .trim()
        .parse()
        .with_context(|| format!("curl reported a non-numeric status `{}`", code.trim()))?;
    Ok(Response {
        status,
        body: body.to_string(),
    })
}

/// Appends `key = "value"` to a curl config, escaping the characters curl treats specially
/// inside double quotes.
fn push_option(config: &mut String, key: &str, value: &str) {
    config.push_str(key);
    config.push_str(" = \"");
    for ch in value.chars() {
        match ch {
            '\\' => config.push_str("\\\\"),
            '"' => config.push_str("\\\""),
            '\n' => config.push_str("\\n"),
            '\r' => config.push_str("\\r"),
            '\t' => config.push_str("\\t"),
            other => config.push(other),
        }
    }
    config.push_str("\"\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_escapes_quotes_backslashes_and_newlines() {
        let mut config = String::new();
        push_option(&mut config, "data-raw", "a\"b\\c\nd");
        assert_eq!(config, "data-raw = \"a\\\"b\\\\c\\nd\"\n");
    }

    #[test]
    fn parses_body_and_trailing_status() {
        let resp = parse_output("{\"ok\":true}\n200").unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "{\"ok\":true}");
    }

    #[test]
    fn empty_body_keeps_status() {
        let resp = parse_output("\n404").unwrap();
        assert_eq!(resp.status, 404);
        assert_eq!(resp.body, "");
    }

    #[test]
    fn rejects_output_without_status() {
        assert!(parse_output("no newline here").is_err());
    }
}
