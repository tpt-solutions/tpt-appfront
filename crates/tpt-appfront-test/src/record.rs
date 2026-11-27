//! `.tptrec` session recordings: the message stream of one app session, for bug reports and
//! regression tests.
//!
//! The format is JSON Lines. The first line is a header, `{"format":"tptrec","version":1}`. Every
//! later line is one `Msg`, serialised with serde_json. Replay is a reducer. The app's own update
//! function turns each message into the next state, so a recording exercises the real logic and
//! does not need a running UI.
//!
//! ```ignore
//! let mut rec = Recording::new();
//! rec.push(Msg::Increment);
//! let text = rec.to_tptrec()?;              // save this with the bug report
//! let back = Recording::<Msg>::from_tptrec(&text)?;
//! let state = back.replay(State::default(), update);
//! ```

use std::error::Error;
use std::fmt;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

/// The `format` field of the header.
pub const FORMAT: &str = "tptrec";
/// The `version` field of the header. Readers reject any other version.
pub const VERSION: u64 = 1;

/// Why a recording could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    /// The text has no header line.
    Empty,
    /// The header line is not a `tptrec` header.
    BadHeader(String),
    /// The header names a version this build does not read.
    UnsupportedVersion(u64),
    /// A message line does not decode into the message type. `line` is 1-based.
    BadMessage { line: usize, error: String },
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordError::Empty => write!(f, "recording is empty; expected a {FORMAT} header"),
            RecordError::BadHeader(h) => write!(f, "not a {FORMAT} header: {h}"),
            RecordError::UnsupportedVersion(v) => write!(
                f,
                "recording version {v} is not supported (this build reads version {VERSION})"
            ),
            RecordError::BadMessage { line, error } => {
                write!(f, "line {line} is not a valid message: {error}")
            }
        }
    }
}

impl Error for RecordError {}

/// The messages of one session, in the order they were dispatched.
#[derive(Debug, Clone, PartialEq)]
pub struct Recording<Msg> {
    msgs: Vec<Msg>,
}

impl<Msg> Default for Recording<Msg> {
    fn default() -> Self {
        Recording { msgs: Vec::new() }
    }
}

impl<Msg: Serialize + DeserializeOwned + Clone> Recording<Msg> {
    /// An empty recording.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one dispatched message.
    pub fn push(&mut self, msg: Msg) {
        self.msgs.push(msg);
    }

    /// The recorded messages, oldest first.
    pub fn messages(&self) -> &[Msg] {
        &self.msgs
    }

    /// Encodes the recording as `.tptrec` text. Fails only if a message cannot be serialised.
    pub fn to_tptrec(&self) -> Result<String, serde_json::Error> {
        let mut out = serde_json::json!({ "format": FORMAT, "version": VERSION }).to_string();
        out.push('\n');
        for msg in &self.msgs {
            out.push_str(&serde_json::to_string(msg)?);
            out.push('\n');
        }
        Ok(out)
    }

    /// Decodes `.tptrec` text. Blank lines are ignored.
    pub fn from_tptrec(text: &str) -> Result<Self, RecordError> {
        let mut lines = text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty());
        let (_, header) = lines.next().ok_or(RecordError::Empty)?;
        check_header(header)?;

        let mut msgs = Vec::new();
        for (index, line) in lines {
            let msg = serde_json::from_str::<Msg>(line).map_err(|e| RecordError::BadMessage {
                line: index + 1,
                error: e.to_string(),
            })?;
            msgs.push(msg);
        }
        Ok(Recording { msgs })
    }

    /// Runs the recording through `update` from `initial` and returns the final state.
    pub fn replay<S>(&self, initial: S, update: impl Fn(S, Msg) -> S) -> S {
        self.msgs
            .iter()
            .cloned()
            .fold(initial, update)
    }
}

fn check_header(line: &str) -> Result<(), RecordError> {
    let value: Value =
        serde_json::from_str(line).map_err(|_| RecordError::BadHeader(line.to_string()))?;
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(RecordError::BadHeader(line.to_string()));
    }
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| RecordError::BadHeader(line.to_string()))?;
    if version != VERSION {
        return Err(RecordError::UnsupportedVersion(version));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum Msg {
        Increment,
        Add { by: i64 },
        Name { value: String },
    }

    fn sample() -> Recording<Msg> {
        let mut r = Recording::new();
        r.push(Msg::Increment);
        r.push(Msg::Add { by: 5 });
        r.push(Msg::Name { value: "Ada".into() });
        r
    }

    #[test]
    fn a_recording_round_trips_through_text() {
        let text = sample().to_tptrec().unwrap();
        assert!(text.starts_with("{\"format\":\"tptrec\",\"version\":1}\n"), "{text}");
        assert_eq!(Recording::<Msg>::from_tptrec(&text).unwrap(), sample());
    }

    #[test]
    fn replay_folds_the_messages_through_the_update_function() {
        let state = sample().replay(0i64, |n, msg| match msg {
            Msg::Increment => n + 1,
            Msg::Add { by } => n + by,
            Msg::Name { .. } => n,
        });
        assert_eq!(state, 6);
    }

    #[test]
    fn an_empty_or_headerless_text_is_rejected() {
        assert_eq!(Recording::<Msg>::from_tptrec("\n  \n"), Err(RecordError::Empty));
        let err = Recording::<Msg>::from_tptrec("{\"kind\":\"increment\"}\n").unwrap_err();
        assert!(matches!(err, RecordError::BadHeader(_)), "{err:?}");
    }

    #[test]
    fn a_newer_version_is_refused_rather_than_misread() {
        let err = Recording::<Msg>::from_tptrec("{\"format\":\"tptrec\",\"version\":2}\n")
            .unwrap_err();
        assert_eq!(err, RecordError::UnsupportedVersion(2));
    }

    #[test]
    fn a_bad_message_reports_its_line_number() {
        let text = "{\"format\":\"tptrec\",\"version\":1}\n{\"kind\":\"increment\"}\n{\"kind\":\"nope\"}\n";
        match Recording::<Msg>::from_tptrec(text) {
            Err(RecordError::BadMessage { line, .. }) => assert_eq!(line, 3),
            other => panic!("expected BadMessage on line 3, got {other:?}"),
        }
    }
}
