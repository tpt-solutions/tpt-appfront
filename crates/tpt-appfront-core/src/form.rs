//! `use_form`: a controlled-form helper bundling named signal-backed fields
//! (string fields for `Input`/`Textarea`/`Select`/`Radio`, boolean fields for
//! `Checkbox`) plus lightweight per-field validation — the "controlled-input
//! helper" companion to the `Input`/`Textarea`/`Checkbox`/`Select`/`Radio`
//! `NodeKind`s, mirroring React's controlled-form ergonomics without a full
//! form library.
//!
//! Each field is its own [`Signal`], so binding a field's value to a node
//! (and its validator's error, if any, to another) re-renders only that
//! node's subtree on change — not the whole form.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::signal::Signal;

struct StringField {
    value: Signal<String>,
    error: Signal<Option<String>>,
    validator: Option<Rc<dyn Fn(&str) -> Option<String>>>,
}

/// A controlled-form helper. Create one per form (typically stored in a
/// component's local state / captured by its closures) and bind fields to
/// `Input`/`Textarea`/`Select`/`Radio`/`Checkbox` nodes via
/// [`FormState::field`]/[`FormState::checkbox`] for the current value and
/// [`FormState::set`]/[`FormState::set_checkbox`] (typically from
/// `.on_input`/`.on_toggle`) to write new values back.
#[derive(Clone)]
pub struct FormState {
    fields: Rc<RefCell<HashMap<String, StringField>>>,
    checks: Rc<RefCell<HashMap<String, Signal<bool>>>>,
}

impl FormState {
    pub fn new() -> Self {
        FormState {
            fields: Rc::new(RefCell::new(HashMap::new())),
            checks: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// The value signal for string field `name`, creating it with `default`
    /// on first access. Read this to bind a node's current value.
    pub fn field(&self, name: &str, default: impl Into<String>) -> Signal<String> {
        self.fields
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| StringField {
                value: Signal::new(default.into()),
                error: Signal::new(None),
                validator: None,
            })
            .value
            .clone()
    }

    /// Registers a validator for `name`, run against every new value passed
    /// to [`FormState::set`]. `f` returns `Some(message)` to reject the value
    /// (recorded, but the value is still stored — callers can still see what
    /// the user typed) or `None` when it's valid.
    pub fn validate(&self, name: &str, f: impl Fn(&str) -> Option<String> + 'static) {
        self.fields
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| StringField {
                value: Signal::new(String::new()),
                error: Signal::new(None),
                validator: None,
            })
            .validator = Some(Rc::new(f));
    }

    /// Sets `name`'s value (creating the field with an empty default if this
    /// is the first write) and re-runs its validator.
    pub fn set(&self, name: &str, value: impl Into<String>) {
        let value = value.into();
        let mut fields = self.fields.borrow_mut();
        let field = fields
            .entry(name.to_string())
            .or_insert_with(|| StringField {
                value: Signal::new(String::new()),
                error: Signal::new(None),
                validator: None,
            });
        let error = field.validator.as_ref().and_then(|v| v(&value));
        field.value.set(value);
        field.error.set(error);
    }

    /// The current validation error for string field `name`, if any.
    pub fn error(&self, name: &str) -> Option<String> {
        self.fields.borrow().get(name).and_then(|f| f.error.get())
    }

    /// The boolean field backing a `Checkbox`, creating it with `default` on
    /// first access.
    pub fn checkbox(&self, name: &str, default: bool) -> Signal<bool> {
        self.checks
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| Signal::new(default))
            .clone()
    }

    /// Sets boolean field `name` (creating it if this is the first write).
    pub fn set_checkbox(&self, name: &str, value: bool) {
        self.checks
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| Signal::new(value))
            .set(value);
    }

    /// `true` if every registered string field currently has no validation
    /// error. Fields with no validator (or never validated) are always
    /// considered valid.
    pub fn is_valid(&self) -> bool {
        self.fields.borrow().values().all(|f| f.error.get().is_none())
    }

    /// Snapshot of every string field's current value (e.g. for submission),
    /// keyed by field name. Boolean (`Checkbox`) fields aren't included —
    /// read those individually via [`FormState::checkbox`].
    pub fn values(&self) -> HashMap<String, String> {
        self.fields
            .borrow()
            .iter()
            .map(|(k, f)| (k.clone(), f.value.get()))
            .collect()
    }
}

impl Default for FormState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_defaults_and_updates() {
        let form = FormState::new();
        let email = form.field("email", "a@example.com");
        assert_eq!(email.get(), "a@example.com");

        form.set("email", "b@example.com");
        assert_eq!(email.get(), "b@example.com");
        assert_eq!(form.field("email", "unused").get(), "b@example.com");
    }

    #[test]
    fn validator_runs_on_set_and_reports_error() {
        let form = FormState::new();
        form.field("age", "");
        form.validate("age", |v| {
            if v.parse::<u32>().is_ok() {
                None
            } else {
                Some("must be a number".to_string())
            }
        });

        form.set("age", "abc");
        assert_eq!(form.error("age").as_deref(), Some("must be a number"));
        assert!(!form.is_valid());

        form.set("age", "42");
        assert_eq!(form.error("age"), None);
        assert!(form.is_valid());
    }

    #[test]
    fn checkbox_field_defaults_and_updates() {
        let form = FormState::new();
        let agree = form.checkbox("agree", false);
        assert!(!agree.get());

        form.set_checkbox("agree", true);
        assert!(agree.get());
        assert!(form.checkbox("agree", false).get());
    }

    #[test]
    fn values_snapshots_string_fields() {
        let form = FormState::new();
        form.set("name", "Ada");
        form.set("email", "ada@example.com");

        let values = form.values();
        assert_eq!(values.get("name").map(String::as_str), Some("Ada"));
        assert_eq!(values.get("email").map(String::as_str), Some("ada@example.com"));
    }
}
