//! Starter presets for `tpt-appfront init --preset <name>`.
//!
//! Each preset scaffolds a runnable DOM app that uses the
//! `tpt-appfront-templates` starter templates (or, for the login form, the
//! `view!` two-way-binding path) wired to real `Signal`-backed state — a "real"
//! UI shape to start from instead of the bare counter (todo.md Phase 19).
//!
//! The generated project is always a DOM (`trunk`-built) crate, since the
//! starter templates are most useful as web UI.

/// A starter preset selectable via `tpt-appfront init --preset <name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// A username/password sign-in screen with live `Signal`-bound fields.
    Login,
    /// A nav sidebar + content area (`dashboard_shell`) composing a CRUD list.
    Dashboard,
    /// A focused CRUD list (`settings_list`) with add/edit/delete state.
    CrudApp,
    /// A fuller SaaS landing: nav shell whose content switches per route.
    SaasStarter,
}

impl Preset {
    /// The `init --preset` token for this preset.
    pub fn name(&self) -> &'static str {
        match self {
            Preset::Login => "login",
            Preset::Dashboard => "dashboard",
            Preset::CrudApp => "crud-app",
            Preset::SaasStarter => "saas-starter",
        }
    }

    /// One-line description shown by `init --list-presets`.
    pub const fn description(&self) -> &'static str {
        match self {
            Preset::Login => "Sign-in form with live, Signal-bound username/password fields",
            Preset::Dashboard => "Sidebar nav shell composing a CRUD settings list",
            Preset::CrudApp => "Standalone CRUD list with add/edit/delete state",
            Preset::SaasStarter => "Full app shell with per-route content switching",
        }
    }

    /// Parses a `init --preset` token, or `None` if it isn't a known preset.
    pub fn from_str(s: &str) -> Option<Preset> {
        match s {
            "login" => Some(Preset::Login),
            "dashboard" => Some(Preset::Dashboard),
            "crud-app" | "crud" => Some(Preset::CrudApp),
            "saas-starter" | "saas" => Some(Preset::SaasStarter),
            _ => None,
        }
    }
}

/// `(preset, description)` pairs for `init --list-presets`.
pub fn list_presets() -> &'static [(Preset, &'static str)] {
    static PRESETS: [(Preset, &str); 4] = [
        (Preset::Login, Preset::Login.description()),
        (Preset::Dashboard, Preset::Dashboard.description()),
        (Preset::CrudApp, Preset::CrudApp.description()),
        (Preset::SaasStarter, Preset::SaasStarter.description()),
    ];
    &PRESETS
}

// ---------------------------------------------------------------------------
// à la carte composition pieces (init --with <a,b,c>)
// ---------------------------------------------------------------------------

/// A single `tpt_appfront_templates` piece selectable via `tpt-appfront init
/// --with <list>` (e.g. `dashboard,settings` composes `dashboard_shell` whose
/// content area holds `settings_list`, matching `examples/templates-demo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithPiece {
    /// `login_form` — a username/password sign-in screen.
    Login,
    /// `dashboard_shell` — a nav sidebar + content area.
    Dashboard,
    /// `settings_list` — a CRUD list of rows.
    Settings,
}

impl WithPiece {
    /// The `--with` token for this piece.
    pub fn name(&self) -> &'static str {
        match self {
            WithPiece::Login => "login",
            WithPiece::Dashboard => "dashboard",
            WithPiece::Settings => "settings",
        }
    }

    /// Parses a `--with` token, or `None` if it isn't a known piece.
    pub fn from_str(s: &str) -> Option<WithPiece> {
        match s.trim() {
            "login" => Some(WithPiece::Login),
            "dashboard" => Some(WithPiece::Dashboard),
            "settings" => Some(WithPiece::Settings),
            _ => None,
        }
    }
}

/// `(piece, description)` pairs for documentation/help.
pub fn list_with_pieces() -> &'static [(WithPiece, &'static str)] {
    static PIECES: [(WithPiece, &str); 3] = [
        (WithPiece::Login, "Sign-in form (login_form)"),
        (WithPiece::Dashboard, "Sidebar nav shell (dashboard_shell)"),
        (WithPiece::Settings, "CRUD list (settings_list)"),
    ];
    &PIECES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_round_trips_through_name() {
        for (preset, _) in list_presets() {
            let parsed = Preset::from_str(preset.name())
                .unwrap_or_else(|| panic!("failed to parse preset `{}`", preset.name()));
            assert_eq!(*preset, parsed);
        }
    }

    #[test]
    fn unknown_preset_is_none() {
        assert!(Preset::from_str("nope").is_none());
        assert!(Preset::from_str("").is_none());
    }

    #[test]
    fn crud_alias_resolves() {
        assert_eq!(Preset::from_str("crud"), Some(Preset::CrudApp));
    }

    #[test]
    fn with_piece_round_trips_through_name() {
        for (piece, _) in list_with_pieces() {
            let parsed = WithPiece::from_str(piece.name())
                .unwrap_or_else(|| panic!("failed to parse piece `{}`", piece.name()));
            assert_eq!(*piece, parsed);
        }
    }

    #[test]
    fn unknown_with_piece_is_none() {
        assert!(WithPiece::from_str("bogus").is_none());
        assert!(WithPiece::from_str("  settings  ").is_some());
    }
}
