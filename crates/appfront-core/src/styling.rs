//! Tailwind-style utility-class layer.
//!
//! A small, dependency-free stand-in for Tailwind: a curated dictionary of
//! utility class names → CSS declarations. There is no external build step or
//! scanner — utilities are resolved at runtime, so they work identically in
//! SSR ([`inline_style`]), in a shipped `<style>` block
//! ([`style_sheet`]), and as a `.class("af-u-...")` value ([`class_value`]).
//!
//! The companion `class!` macro (re-exported from `appfront_macros`) lets
//! authors write `.class(class!("bg-blue-500", "p-4"))` and get a
//! **compile-time** check that every utility name is recognized, surfacing a
//! typo as a build error instead of silently-unstyled output.

/// `(utility_name, css_declaration_without_trailing_semicolon)`.
///
/// Scoped to the most common spacing / color / typography / layout / flex
/// utilities. Add more pairs here to extend the layer.
pub const UTILITIES: &[(&str, &str)] = &[
    // --- display -------------------------------------------------------
    ("block", "display: block"),
    ("inline-block", "display: inline-block"),
    ("flex", "display: flex"),
    ("grid", "display: grid"),
    ("hidden", "display: none"),
    // --- flex ---------------------------------------------------------
    ("flex-row", "display: flex; flex-direction: row"),
    ("flex-col", "display: flex; flex-direction: column"),
    ("items-center", "align-items: center"),
    ("justify-center", "justify-content: center"),
    ("justify-between", "justify-content: space-between"),
    ("flex-1", "flex: 1 1 0%"),
    ("flex-wrap", "flex-wrap: wrap"),
    // --- spacing (padding) ------------------------------------------
    ("p-0", "padding: 0"),
    ("p-1", "padding: 0.25rem"),
    ("p-2", "padding: 0.5rem"),
    ("p-4", "padding: 1rem"),
    ("p-8", "padding: 2rem"),
    ("px-4", "padding-left: 1rem; padding-right: 1rem"),
    ("py-2", "padding-top: 0.5rem; padding-bottom: 0.5rem"),
    ("m-0", "margin: 0"),
    ("mt-4", "margin-top: 1rem"),
    ("mb-2", "margin-bottom: 0.5rem"),
    ("mx-auto", "margin-left: auto; margin-right: auto"),
    // --- sizing -------------------------------------------------------
    ("w-full", "width: 100%"),
    ("h-full", "height: 100%"),
    ("w-64", "width: 16rem"),
    ("max-w-screen-md", "max-width: 48rem"),
    // --- typography ---------------------------------------------------
    ("text-xs", "font-size: 0.75rem"),
    ("text-sm", "font-size: 0.875rem"),
    ("text-lg", "font-size: 1.125rem"),
    ("text-2xl", "font-size: 1.5rem"),
    ("font-bold", "font-weight: 700"),
    ("font-normal", "font-weight: 400"),
    ("text-center", "text-align: center"),
    ("uppercase", "text-transform: uppercase"),
    // --- colors -------------------------------------------------------
    ("text-white", "color: #ffffff"),
    ("text-gray-700", "color: #374151"),
    ("bg-blue-500", "background-color: #3b82f6"),
    ("bg-gray-100", "background-color: #f3f4f6"),
    ("bg-white", "background-color: #ffffff"),
    ("border", "border: 1px solid #d1d5db"),
    ("rounded", "border-radius: 0.25rem"),
    ("rounded-lg", "border-radius: 0.5rem"),
    ("shadow", "box-shadow: 0 1px 3px rgba(0,0,0,0.1)"),
];

/// Looks up the CSS declaration for a utility name, if recognized.
pub fn lookup(name: &str) -> Option<&'static str> {
    UTILITIES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, css)| *css)
}

/// True when `name` is a recognized utility.
pub fn is_utility(name: &str) -> bool {
    lookup(name).is_some()
}

/// A compile-time check used by the `class!` macro: panics (as a const
/// evaluation error) when `name` is not a recognized utility, so a typo like
/// `class!("bg-blue-5000")` fails the build instead of silently rendering
/// unstyled. `const fn` so it can run in a `const` block at macro-expansion.
///
/// `&str::==` is not yet const-stable, so we compare bytes by hand.
pub const fn class_macro_check(name: &str) {
    let mut i = 0;
    while i < UTILITIES.len() {
        if str_eq(UTILITIES[i].0, name) {
            return;
        }
        i += 1;
    }
    panic!("unknown appfront utility class — see `appfront_core::styling::UTILITIES`");
}

/// Const-equivalent of `&str == &str` (the trait method isn't const-stable
/// yet). Used by [`class_macro_check`].
const fn str_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let mut i = 0;
    while i < ab.len() {
        if ab[i] != bb[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// A space-separated class attribute value for the given utilities. Recognized
/// utilities get the `af-u-` prefix so they match the rules produced by
/// [`style_sheet`]; unrecognized names pass through unchanged (so authors can
/// mix in hand-written classes).
pub fn class_value(classes: &str) -> String {
    classes
        .split_whitespace()
        .map(|name| {
            if is_utility(name) {
                format!("af-u-{name}")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// An inline `style="..."` string covering every recognized utility in
/// `classes`. Returns an empty string when none are recognized, so callers can
/// skip emitting a `style` attribute entirely.
pub fn inline_style(classes: &str) -> String {
    let decls: Vec<&str> = classes
        .split_whitespace()
        .filter_map(lookup)
        .collect();
    if decls.is_empty() {
        String::new()
    } else {
        format!("{};", decls.join("; "))
    }
}

/// A `<style>` block declaring one `.af-u-<name> { ... }` rule per
/// recognized utility, suitable for embedding once in a page `<head>`.
pub fn style_sheet(classes: &str) -> String {
    let mut out = String::from("<style>\n");
    for name in classes.split_whitespace() {
        if let Some(css) = lookup(name) {
            out.push_str(&format!("  .af-u-{name} {{ {css} }}\n"));
        }
    }
    out.push_str("</style>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_finds_known_and_rejects_unknown() {
        assert_eq!(lookup("p-4"), Some("padding: 1rem"));
        assert_eq!(lookup("not-a-util"), None);
        assert!(is_utility("flex"));
        assert!(!is_utility("flexxxx"));
    }

    #[test]
    fn class_value_prefixes_utilities_leaves_unknowns() {
        let v = class_value("bg-blue-500 p-4 my-custom-class");
        assert!(v.contains("af-u-bg-blue-500"));
        assert!(v.contains("af-u-p-4"));
        assert!(v.contains("my-custom-class"));
    }

    #[test]
    fn inline_style_covers_only_recognized() {
        let s = inline_style("p-4 nonsense bg-blue-500");
        assert!(s.contains("padding: 1rem"));
        assert!(s.contains("background-color: #3b82f6"));
        assert!(!s.contains("nonsense"));
        assert!(s.ends_with(';'));
    }

    #[test]
    fn inline_style_empty_when_none_match() {
        assert_eq!(inline_style("totally-made-up"), "");
    }

    #[test]
    fn style_sheet_emits_one_rule_per_utility() {
        let sheet = style_sheet("p-4 flex");
        assert!(sheet.starts_with("<style>"));
        assert!(sheet.ends_with("</style>"));
        assert!(sheet.contains(".af-u-p-4 { padding: 1rem }"));
        assert!(sheet.contains(".af-u-flex { display: flex }"));
    }

    #[test]
    fn class_macro_builds_prefixed_value() {
        // Exercises the `class!` macro's expansion (validation + join).
        let v = crate::class!("bg-blue-500", "p-4");
        assert_eq!(v, "af-u-bg-blue-500 af-u-p-4");
    }
}
