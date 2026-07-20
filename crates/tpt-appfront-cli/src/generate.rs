//! Offline, rule-based UI scaffolding for `tpt-appfront generate --prompt "..."`.
//!
//! This is deliberately *not* an LLM call: no network access, no API key, no
//! nondeterminism. It keyword-matches the prompt against a small set of known
//! UI patterns and emits the corresponding `view!` snippet, falling back to a
//! minimal skeleton when nothing matches. A real `--llm` mode that calls out
//! to a model provider is future work (todo.md Phase 11) — this covers the
//! common-case "give me a starting point" use without needing credentials.

/// One recognized UI pattern: a set of trigger keywords and the `view!`
/// snippet to emit when any of them appear in the prompt (case-insensitive).
struct Pattern {
    name: &'static str,
    keywords: &'static [&'static str],
    snippet: &'static str,
}

const PATTERNS: &[Pattern] = &[
    Pattern {
        name: "counter",
        keywords: &["counter", "increment", "count"],
        snippet: r#"tpt_appfront_core::view! {
    <Container class="counter">
        <Heading level={1u8}>"Count: 0"</Heading>
        <Button on_click={Msg::Increment}>"+1"</Button>
    </Container>
}"#,
    },
    Pattern {
        name: "login form",
        keywords: &["login", "log in", "sign in", "signin"],
        snippet: r#"tpt_appfront_core::view! {
    <Container class="login-form">
        <Heading level={1u8}>"Sign in"</Heading>
        <Input value="Email" />
        <Input value="Password" />
        <Button on_click={Msg::Submit}>"Sign in"</Button>
    </Container>
}"#,
    },
    Pattern {
        name: "todo list",
        keywords: &["todo", "to-do", "task list", "checklist"],
        snippet: r#"tpt_appfront_core::view! {
    <Container class="todo-list">
        <Heading level={1u8}>"Tasks"</Heading>
        <Input value="New task" />
        <Button on_click={Msg::AddTask}>"Add"</Button>
    </Container>
}"#,
    },
    Pattern {
        name: "card",
        keywords: &["card", "profile card", "product card"],
        snippet: r#"tpt_appfront_core::view! {
    <Container class="card">
        <Heading level={2u8}>"Title"</Heading>
        <Text>"Description goes here."</Text>
        <Button on_click={Msg::CardAction}>"Learn more"</Button>
    </Container>
}"#,
    },
    Pattern {
        name: "nav bar",
        keywords: &["nav", "navbar", "navigation", "menu bar"],
        snippet: r#"tpt_appfront_core::view! {
    <Container class="navbar">
        <Heading level={1u8}>"Brand"</Heading>
        <Button on_click={Msg::NavHome}>"Home"</Button>
        <Button on_click={Msg::NavAbout}>"About"</Button>
    </Container>
}"#,
    },
    Pattern {
        name: "settings list",
        keywords: &["settings", "crud list", "list of items", "manage items"],
        snippet: r#"// Prefer the prebuilt `tpt_appfront_templates::settings_list` for a
// keyed CRUD list. Inline `view!` equivalent:
tpt_appfront_core::view! {
    <Container class="settings">
        <Heading level={1u8}>"Settings"</Heading>
        <List>
            <Container class="row flex items-center">
                <Text>"Item 1"</Text>
                <Button on_click={Msg::Edit(1)}>"Edit"</Button>
                <Button on_click={Msg::Delete(1)}>"Delete"</Button>
            </Container>
        </List>
    </Container>
}"#,
    },
    Pattern {
        name: "dashboard",
        keywords: &["dashboard", "admin panel", "app shell", "nav and content"],
        snippet: r#"// Prefer the prebuilt `tpt_appfront_templates::dashboard_shell`
// (sidebar nav + content area) and fill its `content` with a template.
tpt_appfront_core::view! {
    <Container class="flex h-screen">
        <Container class="w-64 bg-gray-50 p-4">
            <Heading level={2u8}>"App"</Heading>
            <Button on_click={Msg::NavHome}>"Home"</Button>
            <Button on_click={Msg::NavSettings}>"Settings"</Button>
        </Container>
        <Container class="flex-1 p-6">
            <Text>"Content goes here"</Text>
        </Container>
    </Container>
}"#,
    },
];

const FALLBACK_SNIPPET: &str = r#"tpt_appfront_core::view! {
    <Container class="app">
        <Heading level={1u8}>"Hello"</Heading>
        <Text>"Describe your UI more specifically (e.g. \"counter\", \"login form\", \"todo list\", \"card\", \"nav bar\") for a closer starting point."</Text>
    </Container>
}"#;

/// Picks the best-matching [`Pattern`] for `prompt`, or `None` if no keyword
/// hits — the caller falls back to [`FALLBACK_SNIPPET`] in that case.
fn match_pattern(prompt: &str) -> Option<&'static Pattern> {
    let lower = prompt.to_lowercase();
    PATTERNS.iter().find(|p| p.keywords.iter().any(|kw| lower.contains(kw)))
}

/// Generates a `view!`-macro Rust snippet for `prompt`. Always succeeds:
/// falls back to a minimal skeleton (with a hint on recognized keywords) if
/// no known pattern matches.
pub fn generate(prompt: &str) -> String {
    let snippet = match match_pattern(prompt) {
        Some(pattern) => {
            format!("// Matched pattern: {}\n{}", pattern.name, pattern.snippet)
        }
        None => format!("// No known pattern matched \"{prompt}\" — showing a minimal skeleton.\n{FALLBACK_SNIPPET}"),
    };
    format!(
        "// Generated by `tpt-appfront generate --prompt \"{prompt}\"`.\n\
         // This is a rule-based, offline scaffold — not a live LLM call.\n\
         // Wire the emitted Msg variants into your app's Msg enum and update loop.\n\n{snippet}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_counter_keyword() {
        let out = generate("build me a simple counter app");
        assert!(out.contains("Matched pattern: counter"));
        assert!(out.contains("Msg::Increment"));
    }

    #[test]
    fn matches_login_form_case_insensitively() {
        let out = generate("I need a LOGIN screen");
        assert!(out.contains("Matched pattern: login form"));
    }

    #[test]
    fn matches_todo_list() {
        let out = generate("a todo list for groceries");
        assert!(out.contains("Matched pattern: todo list"));
    }

    #[test]
    fn matches_card_and_nav() {
        assert!(generate("a product card").contains("Matched pattern: card"));
        assert!(generate("top navbar with links").contains("Matched pattern: nav bar"));
    }

    #[test]
    fn matches_settings_and_dashboard() {
        assert!(generate("a settings page").contains("Matched pattern: settings list"));
        assert!(generate("admin dashboard shell").contains("Matched pattern: dashboard"));
    }

    #[test]
    fn falls_back_when_nothing_matches() {
        let out = generate("something completely unrelated to any pattern");
        assert!(out.contains("No known pattern matched"));
        assert!(out.contains(FALLBACK_SNIPPET));
    }

    #[test]
    fn output_always_notes_it_is_offline() {
        let out = generate("counter");
        assert!(out.contains("not a live LLM call"));
    }
}
