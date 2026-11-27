//! `appfront.toml`: project defaults for `dev` and `build`.
//!
//! ```toml
//! [dev]
//! target = "desktop"      # desktop | web | tui | desktop-webview
//! devtools = false        # same as --devtools
//! reload = true           # false is the same as --no-reload
//!
//! [build]
//! target = "canvas"       # the value --target would take
//! bundle = false          # same as --bundle
//! ```
//!
//! Command-line flags always win over the file. The file is optional. Unknown keys and invalid
//! values are errors, so a typo cannot silently change what gets built.

use std::path::Path;

use anyhow::{bail, Context};

/// The file name, relative to the project directory.
pub const FILE_NAME: &str = "appfront.toml";

/// The `dev` target a project can choose by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevTarget {
    Desktop,
    Web,
    Tui,
    DesktopWebview,
}

impl DevTarget {
    fn parse(s: &str) -> anyhow::Result<Self> {
        Ok(match s {
            "desktop" => DevTarget::Desktop,
            "web" => DevTarget::Web,
            "tui" => DevTarget::Tui,
            "desktop-webview" => DevTarget::DesktopWebview,
            other => bail!(
                "appfront.toml: [dev] target `{other}` is not one of desktop, web, tui, desktop-webview"
            ),
        })
    }
}

/// The parsed file. Every field defaults to "not set", which means the flags decide.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    pub dev_target: Option<DevTarget>,
    pub dev_devtools: bool,
    pub dev_reload: bool,
    pub build_target: Option<String>,
    pub build_bundle: bool,
}

impl ProjectConfig {
    /// Loads `project/appfront.toml`. A missing file gives the defaults.
    pub fn load(project: &Path) -> anyhow::Result<Self> {
        let path = project.join(FILE_NAME);
        if !path.is_file() {
            return Ok(Self {
                dev_reload: true,
                ..Self::default()
            });
        }
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    /// Parses the file contents.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let table: toml::Table = text.parse().context("invalid TOML")?;
        let mut cfg = ProjectConfig {
            dev_reload: true,
            ..ProjectConfig::default()
        };

        for (section, value) in &table {
            match section.as_str() {
                "dev" => {
                    let t = section_table(section, value)?;
                    for (key, v) in t {
                        match key.as_str() {
                            "target" => {
                                cfg.dev_target = Some(DevTarget::parse(string(section, key, v)?)?)
                            }
                            "devtools" => cfg.dev_devtools = boolean(section, key, v)?,
                            "reload" => cfg.dev_reload = boolean(section, key, v)?,
                            other => bail!("appfront.toml: unknown key [dev] {other}"),
                        }
                    }
                }
                "build" => {
                    let t = section_table(section, value)?;
                    for (key, v) in t {
                        match key.as_str() {
                            "target" => {
                                cfg.build_target = Some(string(section, key, v)?.to_string())
                            }
                            "bundle" => cfg.build_bundle = boolean(section, key, v)?,
                            other => bail!("appfront.toml: unknown key [build] {other}"),
                        }
                    }
                }
                other => bail!("appfront.toml: unknown section [{other}]"),
            }
        }
        Ok(cfg)
    }
}

fn section_table<'a>(section: &str, value: &'a toml::Value) -> anyhow::Result<&'a toml::Table> {
    value
        .as_table()
        .with_context(|| format!("appfront.toml: [{section}] must be a table"))
}

fn string<'a>(section: &str, key: &str, value: &'a toml::Value) -> anyhow::Result<&'a str> {
    value
        .as_str()
        .with_context(|| format!("appfront.toml: [{section}] {key} must be a string"))
}

fn boolean(section: &str, key: &str, value: &toml::Value) -> anyhow::Result<bool> {
    value
        .as_bool()
        .with_context(|| format!("appfront.toml: [{section}] {key} must be true or false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_file_parses() {
        let cfg = ProjectConfig::parse(
            r#"
[dev]
target = "web"
devtools = true
reload = false

[build]
target = "canvas"
bundle = true
"#,
        )
        .unwrap();
        assert_eq!(cfg.dev_target, Some(DevTarget::Web));
        assert!(cfg.dev_devtools);
        assert!(!cfg.dev_reload);
        assert_eq!(cfg.build_target.as_deref(), Some("canvas"));
        assert!(cfg.build_bundle);
    }

    #[test]
    fn an_empty_file_keeps_the_defaults_with_reload_on() {
        let cfg = ProjectConfig::parse("").unwrap();
        assert_eq!(cfg.dev_target, None);
        assert!(cfg.dev_reload);
        assert!(!cfg.dev_devtools);
        assert_eq!(cfg.build_target, None);
        assert!(!cfg.build_bundle);
    }

    #[test]
    fn an_unknown_key_is_an_error_not_a_silent_default() {
        let err = ProjectConfig::parse("[dev]\ntarget_typo = \"web\"\n").unwrap_err();
        assert!(
            format!("{err:#}").contains("unknown key [dev] target_typo"),
            "{err:#}"
        );
    }

    #[test]
    fn an_unknown_section_is_an_error() {
        let err = ProjectConfig::parse("[deploy]\nx = 1\n").unwrap_err();
        assert!(format!("{err:#}").contains("unknown section [deploy]"));
    }

    #[test]
    fn an_invalid_dev_target_names_the_allowed_values() {
        let err = ProjectConfig::parse("[dev]\ntarget = \"phone\"\n").unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("phone") && msg.contains("desktop-webview"),
            "{msg}"
        );
    }

    #[test]
    fn a_wrong_type_is_reported_with_the_key() {
        let err = ProjectConfig::parse("[build]\nbundle = \"yes\"\n").unwrap_err();
        assert!(format!("{err:#}").contains("bundle must be true or false"));
    }

    #[test]
    fn a_missing_file_is_the_defaults() {
        let dir = std::env::temp_dir().join(format!("tpt-config-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = ProjectConfig::load(&dir).unwrap();
        assert!(cfg.dev_reload);
        assert_eq!(cfg.dev_target, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
