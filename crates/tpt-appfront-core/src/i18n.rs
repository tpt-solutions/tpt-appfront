//! Translations: a per-locale string catalog, the `t!` macro, and a reactive
//! current locale.
//!
//! ```ignore
//! let mut catalog = Catalog::new("en");
//! catalog.add("en", &[("greeting", "Hello, {name}")]);
//! catalog.add("fr", &[("greeting", "Bonjour, {name}")]);
//! i18n::install(catalog);
//! i18n::set_locale("fr");
//! let text = t!("greeting", name = "Ada"); // "Bonjour, Ada"
//! ```
//!
//! Lookup order for a key: the exact locale (`fr-CA`), then its language
//! (`fr`), then the catalog's default locale, then the key itself. A missing
//! translation never panics: it shows the key, so a gap is visible on screen.
//!
//! The current locale is a [`Signal`]. Reading it inside an effect (which `t!`
//! does) makes that effect rerun when [`set_locale`] changes the language. Views
//! that rebuild every frame (canvas, TUI) pick up the change on their next frame.
//!
//! Placeholders are `{name}`. `t!("key", name = value)` replaces each `{name}`
//! with the value's `Display` form. Unknown placeholders are left as written.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::signal::Signal;

/// Translated strings, grouped by locale tag.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    default_locale: String,
    locales: HashMap<String, HashMap<String, String>>,
}

impl Catalog {
    /// An empty catalog. Keys missing from `default_locale` fall back to the key.
    pub fn new(default_locale: impl Into<String>) -> Self {
        Catalog {
            default_locale: default_locale.into(),
            locales: HashMap::new(),
        }
    }

    /// Adds `(key, text)` pairs for `locale`. A later call for the same key
    /// replaces the earlier text.
    pub fn add(&mut self, locale: &str, entries: &[(&str, &str)]) -> &mut Self {
        let table = self.locales.entry(locale.to_string()).or_default();
        for (key, text) in entries {
            table.insert((*key).to_string(), (*text).to_string());
        }
        self
    }

    /// Adds the entries of a flat JSON object (`{"key": "text", ...}`) for
    /// `locale`. Values must be strings.
    pub fn add_json(&mut self, locale: &str, json: &str) -> Result<&mut Self, serde_json::Error> {
        let table: HashMap<String, String> = serde_json::from_str(json)?;
        self.locales
            .entry(locale.to_string())
            .or_default()
            .extend(table);
        Ok(self)
    }

    /// The text for `key` in `locale`, following the fallback order in the
    /// module docs. Returns `key` when nothing matches.
    pub fn lookup(&self, locale: &str, key: &str) -> String {
        let language = locale.split(['-', '_']).next().unwrap_or(locale);
        let candidates = [locale, language, self.default_locale.as_str()];
        for candidate in candidates {
            if let Some(text) = self.locales.get(candidate).and_then(|t| t.get(key)) {
                return text.clone();
            }
        }
        key.to_string()
    }
}

thread_local! {
    static CATALOG: RefCell<Catalog> = RefCell::new(Catalog::default());
    static LOCALE: Signal<String> = Signal::new(String::from("en"));
}

/// Makes `catalog` the one `t!` reads from, replacing any earlier one.
pub fn install(catalog: Catalog) {
    CATALOG.with(|c| *c.borrow_mut() = catalog);
}

/// Sets the current locale. Effects that read it rerun.
pub fn set_locale(locale: &str) {
    LOCALE.with(|l| l.set(locale.to_string()));
}

/// The current locale tag. Reading it inside an effect subscribes that effect.
pub fn locale() -> String {
    LOCALE.with(|l| l.get())
}

/// The text for `key` in the current locale, without placeholder substitution.
/// Use the `t!` macro for normal calls; this is what the macro expands to.
pub fn translate(key: &str) -> String {
    let locale = locale();
    CATALOG.with(|c| c.borrow().lookup(&locale, key))
}

/// Replaces each `{name}` in `template` with the matching value. Used by `t!`
/// when it is given arguments.
pub fn interpolate(template: &str, args: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Translates a key in the current locale: `t!("key")`, or
/// `t!("key", name = value, ...)` to fill `{name}` placeholders.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::translate($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::interpolate(
            &$crate::i18n::translate($key),
            &[$((stringify!($name), ($value).to_string())),+],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Catalog {
        let mut c = Catalog::new("en");
        c.add("en", &[("hello", "Hello"), ("bye", "Goodbye")]);
        c.add("fr", &[("hello", "Bonjour")]);
        c.add("fr-CA", &[("hello", "Bonjour, eh")]);
        c
    }

    #[test]
    fn lookup_follows_exact_language_then_default_then_key() {
        let c = sample();
        assert_eq!(c.lookup("fr-CA", "hello"), "Bonjour, eh");
        // `fr-FR` has no table, so it falls to the `fr` language.
        assert_eq!(c.lookup("fr-FR", "hello"), "Bonjour");
        // `fr` has no `bye`, so the default locale supplies it.
        assert_eq!(c.lookup("fr", "bye"), "Goodbye");
        // Nothing has the key: the key itself is shown.
        assert_eq!(c.lookup("fr", "missing.key"), "missing.key");
    }

    #[test]
    fn add_json_merges_and_rejects_non_strings() {
        let mut c = Catalog::new("en");
        c.add_json("de", r#"{"hello": "Hallo"}"#).unwrap();
        assert_eq!(c.lookup("de", "hello"), "Hallo");
        assert!(c.add_json("de", r#"{"hello": 3}"#).is_err());
    }

    #[test]
    fn interpolate_fills_named_placeholders_and_keeps_unknown_ones() {
        let out = interpolate(
            "{who} has {n} items, {unknown}",
            &[("who", "Ada".into()), ("n", "3".into())],
        );
        assert_eq!(out, "Ada has 3 items, {unknown}");
    }

    #[test]
    fn t_macro_uses_the_installed_catalog_and_locale() {
        let mut c = sample();
        c.add("en", &[("greet", "Hi, {name}")]);
        install(c);
        set_locale("en");
        assert_eq!(t!("hello"), "Hello");
        assert_eq!(t!("greet", name = "Ada"), "Hi, Ada");
        set_locale("fr-CA");
        assert_eq!(t!("hello"), "Bonjour, eh");
        set_locale("en");
    }

    #[test]
    fn effect_reruns_when_locale_changes() {
        use crate::signal::create_effect;
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut c = sample();
        c.add("en", &[("status", "English")]);
        c.add("fr", &[("status", "Français")]);
        install(c);
        set_locale("en");

        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&seen);
        let _effect = create_effect(move || {
            sink.borrow_mut().push(t!("status"));
        });
        set_locale("fr");
        set_locale("en");
        assert_eq!(*seen.borrow(), vec!["English", "Français", "English"]);
    }
}
