//! Design tokens: named values (colors, spacing, radii, fonts) defined once and
//! read by every backend.
//!
//! A [`Theme`] holds a light set and an optional dark set. Names are plain
//! strings such as `"color.primary"`, and the matching CSS custom property is
//! `--color-primary` (dots become dashes).
//!
//! ```ignore
//! let theme = Theme::new()
//!     .light("color.primary", "#2563eb")
//!     .light("color.surface", "#ffffff")
//!     .dark("color.surface", "#0f172a");
//! let css = theme.to_css(); // :root vars, plus a prefers-color-scheme: dark block
//! let primary = theme.value("color.primary", Scheme::Light); // Some("#2563eb")
//! ```
//!
//! The DOM and HTML backends consume [`Theme::to_css`]. Canvas and TUI read the
//! resolved values with [`Theme::value`]. Tokens hold strings and are not typed,
//! so each backend decides how to interpret a value.

use std::collections::BTreeMap;

/// Which token set to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Light,
    Dark,
}

/// A set of design tokens for light and dark color schemes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Theme {
    light: BTreeMap<String, String>,
    dark: BTreeMap<String, String>,
}

impl Theme {
    /// An empty theme.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a light-scheme token.
    pub fn light(mut self, name: &str, value: &str) -> Self {
        self.light.insert(name.to_string(), value.to_string());
        self
    }

    /// Sets a dark-scheme token. Tokens missing here fall back to the light value.
    pub fn dark(mut self, name: &str, value: &str) -> Self {
        self.dark.insert(name.to_string(), value.to_string());
        self
    }

    /// The value of `name` in `scheme`. A dark lookup with no dark value falls
    /// back to the light value. Returns `None` for an unknown name.
    pub fn value(&self, name: &str, scheme: Scheme) -> Option<&str> {
        match scheme {
            Scheme::Dark => self
                .dark
                .get(name)
                .or_else(|| self.light.get(name))
                .map(String::as_str),
            Scheme::Light => self.light.get(name).map(String::as_str),
        }
    }

    /// The CSS custom property name for a token: `color.primary` becomes
    /// `--color-primary`.
    pub fn css_var(name: &str) -> String {
        format!("--{}", name.replace('.', "-"))
    }

    /// Stylesheet text: a `:root` block with the light values, and a
    /// `prefers-color-scheme: dark` block with the dark overrides. Output is
    /// sorted by name, so it is stable across runs.
    pub fn to_css(&self) -> String {
        let mut out = String::from(":root {\n");
        for (name, value) in &self.light {
            out.push_str(&format!("  {}: {};\n", Self::css_var(name), value));
        }
        out.push_str("}\n");
        if !self.dark.is_empty() {
            out.push_str("@media (prefers-color-scheme: dark) {\n  :root {\n");
            for (name, value) in &self.dark {
                out.push_str(&format!("    {}: {};\n", Self::css_var(name), value));
            }
            out.push_str("  }\n}\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Theme {
        Theme::new()
            .light("color.primary", "#2563eb")
            .light("color.surface", "#ffffff")
            .light("space.md", "1rem")
            .dark("color.surface", "#0f172a")
    }

    #[test]
    fn dark_lookup_falls_back_to_light() {
        let t = sample();
        assert_eq!(t.value("color.surface", Scheme::Dark), Some("#0f172a"));
        assert_eq!(t.value("color.primary", Scheme::Dark), Some("#2563eb"));
        assert_eq!(t.value("color.primary", Scheme::Light), Some("#2563eb"));
        assert_eq!(t.value("missing", Scheme::Light), None);
    }

    #[test]
    fn css_var_turns_dots_into_dashes() {
        assert_eq!(Theme::css_var("color.primary"), "--color-primary");
        assert_eq!(Theme::css_var("radius"), "--radius");
    }

    #[test]
    fn to_css_emits_root_and_dark_overrides_only() {
        let css = sample().to_css();
        assert!(css.starts_with(":root {\n"));
        assert!(css.contains("  --color-primary: #2563eb;\n"));
        assert!(css.contains("@media (prefers-color-scheme: dark) {"));
        assert!(css.contains("    --color-surface: #0f172a;\n"));
        // Light-only tokens are not repeated in the dark block.
        let dark_block = &css[css.find("@media").unwrap()..];
        assert!(!dark_block.contains("--space-md"));
    }

    #[test]
    fn theme_without_dark_tokens_has_no_media_block() {
        let css = Theme::new().light("a", "1").to_css();
        assert!(!css.contains("@media"));
    }
}
