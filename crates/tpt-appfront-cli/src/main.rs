mod generate;
mod ingest;
#[cfg(feature = "llm")]
mod llm;
mod presets;
mod templates;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process, Stdio};
use std::time::Duration;
use std::{fs, thread};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "tpt-appfront",
    about = "Unified UI framework for web, desktop, and AI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum InitTarget {
    Dom,
    Canvas,
    Tui,
    Both,
    Server,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new tpt-appfront project.
    Init {
        /// Project name (and directory to create).
        name: String,
        /// Which backend(s) to scaffold.
        #[arg(long, value_enum, default_value = "both")]
        target: InitTarget,
        /// Start from a starter preset instead of the bare counter
        /// (e.g. `login`, `dashboard`, `crud-app`, `saas-starter`).
        #[arg(long)]
        preset: Option<String>,
        /// Compose an à la carte project from `tpt-appfront-templates` pieces
        /// (comma-separated: `login`, `dashboard`, `settings`), e.g.
        /// `tpt-appfront init <name> --with dashboard,settings` fills the
        /// dashboard's content area with the CRUD settings list.
        #[arg(long, value_delimiter = ',')]
        with: Vec<String>,
        /// Print the available presets and exit.
        #[arg(long)]
        list_presets: bool,
    },
    /// Start the development server.
    Dev {
        /// Run the native desktop (canvas) build via `cargo run`.
        #[arg(long)]
        desktop: bool,
        /// Run the browser (DOM) build via `trunk serve`.
        #[arg(long)]
        web: bool,
        /// Run the terminal (TUI) build via `cargo run`.
        #[arg(long)]
        tui: bool,
        /// Run the desktop webview shell (`tpt-appfront-webview`) via `cargo run`,
        /// hosting the `ui/` trunk build inside the OS webview.
        #[arg(long)]
        desktop_webview: bool,
        /// Disable the watch/reload loop for `--desktop` and run a single plain
        /// `cargo run` (useful when you manage reloading externally).
        #[arg(long)]
        no_reload: bool,
        /// Enable the AppFront devtools inspector: sets `TPT_APPFRONT_DEVTOOLS=1`
        /// on the spawned dev process so the scaffolded app prints its `UITree`
        /// structure on startup (see `tpt_appfront_core::devtools`).
        #[arg(long)]
        devtools: bool,
        /// Directory of the crate to run (defaults to the current directory).
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Build the application for a target.
    Build {
        /// Target: dom, canvas, webview, server, or all.
        #[arg(long)]
        target: Option<String>,
        /// Directory of the crate to build (defaults to the current directory).
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// After building, produce signed installers via `cargo packager`
        /// (requires `cargo install cargo-packager`).
        #[arg(long)]
        bundle: bool,
    },
    /// Run benchmarks (`cargo bench`) for the project.
    Benchmark {
        /// Directory of the crate to benchmark (defaults to the current dir).
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Pre-flight environment check: verifies `trunk`, the
    /// `wasm32-unknown-unknown` target, and `cargo-packager` are available
    /// before you scaffold/build, and reports whether this CLI is running from
    /// the monorepo checkout (path deps) or a published install (version deps).
    Doctor {
        /// Directory of the project to inspect (defaults to the current dir).
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// Run a deterministic, no-AST heuristic accessibility lint over the
        /// project's `src/` (flags images/links/buttons that look like they're
        /// missing accessible names). Informational only — never fails the run.
        #[arg(long)]
        a11y: bool,
    },
    /// Build with size optimizations and report the resulting artifact size.
    Optimize {
        /// Target: canvas, dom, webview, or all.
        #[arg(long, default_value = "all")]
        target: String,
        /// Directory of the crate to optimize (defaults to the current dir).
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// Auto-pick the size-optimized profile / flag set (default on).
        #[arg(long, default_value_t = true)]
        auto: bool,
        /// After building, produce signed installers via `cargo packager`.
        #[arg(long)]
        bundle: bool,
        /// Run a deterministic heuristic scan (no AST/LLM) that flags
        /// unvirtualized long `List`/`DataGrid` usages and a release profile
        /// missing the CLI's own default size flags.
        #[arg(long)]
        analyze: bool,
    },
    /// Generate a `view!` UI scaffold from a text prompt. Offline and
    /// rule-based (keyword-matched against known patterns) by default — not a
    /// live LLM call. Pass `--llm` for a live, model-backed scaffold (needs the
    /// `llm` CLI feature and an API key; see `docs/quickstart.md`).
    Generate {
        /// Description of the UI to scaffold, e.g. "a login form".
        #[arg(long)]
        prompt: String,
        /// Write the generated snippet to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Use a live LLM provider instead of the offline rule-based generator.
        /// Requires this CLI to be built with the `llm` feature.
        #[arg(long)]
        llm: bool,
        /// Which LLM provider to use with `--llm` (only `anthropic` today).
        #[arg(long, default_value = "anthropic")]
        provider: String,
        /// Model id passed to the provider (e.g. `claude-sonnet-4-5`).
        #[arg(long, default_value = "claude-sonnet-4-5")]
        model: String,
        /// Override the provider's default API endpoint (self-hosted/proxy).
        #[arg(long)]
        base_url: Option<String>,
    },
    /// Ingest existing static / server-rendered HTML and emit a `view!`
    /// builder skeleton (structure + classes only; inline event handlers
    /// become `todo!()` stubs). Pipe the output into an existing project.
    Ingest {
        /// Path to the HTML file to ingest.
        input: PathBuf,
        /// Write the generated skeleton to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Grow an existing project: scaffold a new reusable `view!` component or
    /// route-sized page inside it and wire it into the module tree.
    Add {
        /// What to add: `component` or `page`.
        #[command(subcommand)]
        kind: AddKind,
    },
    /// Format the workspace and every standalone example with `cargo fmt`.
    Fmt,
    /// Lint the workspace and every standalone example with
    /// `cargo clippy --all-targets -- -D warnings`.
    Lint,
}

/// The sub-kind of `tpt-appfront add`.
#[derive(Subcommand)]
enum AddKind {
    /// Add a reusable `view!` component under `src/components/`.
    Component {
        /// Component name (PascalCase, e.g. `UserBadge`).
        name: String,
        /// Directory of the project to grow (defaults to the current dir).
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Add a route-sized `view!` page under `src/pages/`.
    Page {
        /// Page name (PascalCase, e.g. `Settings`).
        name: String,
        /// Directory of the project to grow (defaults to the current dir).
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init {
            name,
            target,
            preset,
            with,
            list_presets,
        } => {
            if list_presets {
                list_presets_cmd();
                return Ok(());
            }
            init(&name, target, preset.as_deref(), &with)
        }
        Command::Dev {
            desktop,
            web,
            tui,
            desktop_webview,
            no_reload,
            devtools,
            project,
        } => dev(
            desktop,
            web,
            tui,
            desktop_webview,
            no_reload,
            devtools,
            &project,
        ),
        Command::Build {
            target,
            project,
            bundle,
        } => build(target, &project, bundle),
        Command::Benchmark { project } => benchmark(&project),
        Command::Doctor { project, a11y } => {
            if a11y {
                doctor_a11y(&project)
            } else {
                doctor(&project)
            }
        }
        Command::Optimize {
            target,
            project,
            auto,
            bundle,
            analyze,
        } => {
            if analyze {
                optimize_analyze(&project)
            } else {
                optimize(&target, &project, auto, bundle)
            }
        }
        Command::Generate {
            prompt,
            out,
            llm,
            provider,
            model,
            base_url,
        } => generate_ui(&prompt, out.as_deref(), llm, &provider, &model, base_url.as_deref()),
        Command::Ingest { input, out } => ingest::ingest_file(&input, out.as_deref()),
        Command::Add { kind } => match kind {
            AddKind::Component { name, project } => add_component(&name, &project),
            AddKind::Page { name, project } => add_page(&name, &project),
        },
        Command::Fmt => fmt_cmd(),
        Command::Lint => lint_cmd(),
    }
}

// ---------------------------------------------------------------------------
// generate
// ---------------------------------------------------------------------------

fn generate_ui(
    prompt: &str,
    out: Option<&Path>,
    llm: bool,
    provider: &str,
    model: &str,
    base_url: Option<&str>,
) -> anyhow::Result<()> {
    let snippet = if llm {
        #[cfg(feature = "llm")]
        {
            llm::generate(prompt, provider, model, base_url)?
        }
        #[cfg(not(feature = "llm"))]
        {
            // Referenced so the params stay used (and the warning-free build) when
            // the `llm` feature is off; the only path here is a clear error.
            let _ = (provider, model, base_url);
            anyhow::bail!(
                "`generate --llm` requires this CLI to be built with the `llm` feature. \
                 Rebuild/install with `cargo install tpt-appfront-cli --features llm` (or \
                 `cargo build --features llm`)."
            )
        }
    } else {
        generate::generate(prompt)
    };
    match out {
        Some(path) => {
            fs::write(path, &snippet).with_context(|| format!("writing {}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => print!("{snippet}"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

/// Absolute path to the `crates/` directory of the `tpt-appfront` checkout
/// that built this CLI binary, so scaffolded projects can depend on the
/// (as-yet-unpublished) backend crates with zero manual edits.
fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tpt-appfront-cli is always nested under crates/")
        .to_path_buf()
}

fn dep_path(crate_name: &str) -> String {
    crates_dir()
        .join(crate_name)
        .to_string_lossy()
        .replace('\\', "/")
}

/// True when this CLI is running against the `tpt-appfront` monorepo checkout
/// that built it — i.e. the sibling `crates/tpt-appfront-core/Cargo.toml` exists
/// on disk. A `cargo install tpt-appfront-cli` has no such sibling, and its
/// `CARGO_MANIFEST_DIR` points at the (absent) build-time source dir, so this
/// is false and we fall back to version dependencies instead.
fn is_workspace_checkout() -> bool {
    crates_dir()
        .join("tpt-appfront-core")
        .join("Cargo.toml")
        .exists()
}

/// The version to require for crates on a published install, overridable via
/// the `TPT_APPFRONT_DEP_VERSION` env var (e.g. pinning a pre-release). Defaults to
/// this CLI's own `CARGO_PKG_VERSION`.
fn published_version() -> String {
    std::env::var("TPT_APPFRONT_DEP_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

/// Returns a ready-to-emit TOML dependency spec for `crate_name`: a `path`
/// dependency when running inside the monorepo checkout, or a version
/// dependency on a published install. This is what makes scaffolded
/// `Cargo.toml`s build both locally (against the checkout) and once the
/// crates are published to crates.io (`todo.md` Phase 15).
fn dep_ref(crate_name: &str) -> String {
    if is_workspace_checkout() {
        format!("{{ path = \"{}\" }}", dep_path(crate_name))
    } else {
        format!("\"{}\"", published_version())
    }
}

fn init(
    name: &str,
    target: InitTarget,
    preset: Option<&str>,
    with: &[String],
) -> anyhow::Result<()> {
    if name.is_empty()
        || name.contains(['/', '\\'])
        || name == ".."
        || Path::new(name).is_absolute()
    {
        bail!("invalid project name `{name}`: must be a plain directory name, not a path");
    }
    let root = PathBuf::from(name);
    if root.exists() {
        bail!("directory `{name}` already exists");
    }

    // `--with` and `--preset` are alternative scaffolds; picking both is a
    // user error rather than silently preferring one.
    if !with.is_empty() && preset.is_some() {
        bail!("pass only one of `--with` or `--preset`, not both");
    }

    // `--with` composes selected `tpt-appfront-templates` pieces into one DOM
    // app (à la carte), like the presets but user-curated (todo.md cross-cutting).
    if !with.is_empty() {
        let pieces: Vec<presets::WithPiece> = with
            .iter()
            .map(|s| {
                presets::WithPiece::from_str(s).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown `--with` piece `{s}`; valid pieces: login, dashboard, settings"
                    )
                })
            })
            .collect::<anyhow::Result<_>>()?;
        if pieces.is_empty() {
            bail!("`--with` needs at least one piece (login, dashboard, settings)");
        }
        fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
        scaffold_with_crate(&root, name, &format!("{name} — TPT AppFront"), &pieces)?;
        let csv = pieces
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>()
            .join(",");
        println!("Created `{name}` (with: {csv}).");
        println!("  cd {name} && trunk serve        # browser (DOM)");
        return Ok(());
    }

    // A preset overrides the backend target: presets scaffold a single DOM app
    // that uses `tpt-appfront-templates` + `Signal` state (todo.md Phase 19).
    if let Some(preset_name) = preset {
        let preset = presets::Preset::from_str(preset_name).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown preset `{preset_name}`; run `tpt-appfront init --list-presets` for the available presets"
            )
        })?;
        fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
        scaffold_preset_crate(&root, name, &format!("{name} — TPT AppFront"), preset)?;
        println!("Created `{name}` (preset: {}).", preset.name());
        println!("  cd {name} && trunk serve        # browser (DOM)");
        return Ok(());
    }

    fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;

    let app_title = format!("{name} — TPT AppFront");

    match target {
        InitTarget::Canvas => {
            scaffold_canvas_crate(&root, name, &app_title)?;
        }
        InitTarget::Dom => {
            scaffold_dom_crate(&root, name, &app_title)?;
        }
        InitTarget::Tui => {
            scaffold_tui_crate(&root, name, &app_title)?;
        }
        InitTarget::Both => {
            scaffold_canvas_crate(&root.join("canvas"), &format!("{name}-canvas"), &app_title)?;
            scaffold_dom_crate(&root.join("dom"), &format!("{name}-dom"), &app_title)?;
        }
        InitTarget::Server => {
            scaffold_server_crate(&root, name, &app_title)?;
        }
    }

    fs::write(root.join(".gitignore"), templates::gitignore())?;
    fs::write(
        root.join("README.md"),
        templates::readme(name, matches!(target, InitTarget::Both)),
    )?;

    println!("Created `{name}` ({}).", target_label(target));
    match target {
        InitTarget::Both => {
            println!("  cd {name}/canvas && cargo run          # desktop");
            println!("  cd {name}/dom    && trunk serve         # browser");
        }
        InitTarget::Canvas => println!("  cd {name} && cargo run"),
        InitTarget::Dom => println!("  cd {name} && trunk serve"),
        InitTarget::Tui => println!("  cd {name} && cargo run"),
        InitTarget::Server => println!("  cd {name} && cargo run   # smart-router server"),
    }
    Ok(())
}

/// Prints the available `init --preset` starters (and `init --with` pieces)
/// and exits.
fn list_presets_cmd() {
    println!("Available presets (`tpt-appfront init <name> --preset <preset>`):");
    for (preset, desc) in presets::list_presets() {
        println!("  {:<12} {}", preset.name(), desc);
    }
    println!();
    println!("Available `--with` pieces (`tpt-appfront init <name> --with <csv>`):");
    for (piece, desc) in presets::list_with_pieces() {
        println!("  {:<10} {}", piece.name(), desc);
    }
}

fn target_label(target: InitTarget) -> &'static str {
    match target {
        InitTarget::Dom => "dom",
        InitTarget::Canvas => "canvas",
        InitTarget::Tui => "tui",
        InitTarget::Both => "canvas + dom",
        InitTarget::Server => "server",
    }
}

fn scaffold_canvas_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::canvas_cargo_toml(
            pkg_name,
            &dep_ref("tpt-appfront-core"),
            &dep_ref("tpt-appfront-canvas"),
        ),
    )?;
    fs::write(
        dir.join("src").join("main.rs"),
        templates::canvas_main_rs(app_title),
    )?;
    Ok(())
}

fn scaffold_dom_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::dom_cargo_toml(
            pkg_name,
            &dep_ref("tpt-appfront-core"),
            &dep_ref("tpt-appfront-dom"),
        ),
    )?;
    fs::write(
        dir.join("src").join("lib.rs"),
        templates::dom_lib_rs(app_title),
    )?;
    fs::write(dir.join("index.html"), templates::index_html(app_title))?;
    Ok(())
}

fn scaffold_tui_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::tui_cargo_toml(
            pkg_name,
            &dep_ref("tpt-appfront-core"),
            &dep_ref("tpt-appfront-tui"),
        ),
    )?;
    fs::write(
        dir.join("src").join("main.rs"),
        templates::tui_main_rs(app_title),
    )?;
    Ok(())
}

fn scaffold_server_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::server_cargo_toml(
            pkg_name,
            &dep_ref("tpt-appfront-core"),
            &dep_ref("tpt-appfront-server"),
        ),
    )?;
    fs::write(
        dir.join("src").join("main.rs"),
        templates::server_main_rs(app_title),
    )?;
    Ok(())
}

/// Scaffolds a single DOM crate driven by an `init --with` piece list. The
/// `Cargo.toml` reuses the preset template (it also pulls in
/// `tpt-appfront-templates`); the `lib.rs` composes the selected pieces.
fn scaffold_with_crate(
    dir: &Path,
    pkg_name: &str,
    app_title: &str,
    pieces: &[presets::WithPiece],
) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::preset_cargo_toml(
            pkg_name,
            &dep_ref("tpt-appfront-core"),
            &dep_ref("tpt-appfront-dom"),
            &dep_ref("tpt-appfront-templates"),
        ),
    )?;
    fs::write(
        dir.join("src").join("lib.rs"),
        templates::with_lib_rs(pieces, app_title),
    )?;
    fs::write(dir.join("index.html"), templates::index_html(app_title))?;
    Ok(())
}

/// Scaffolds a single DOM crate driven by a starter preset. The Cargo.toml pulls
/// in `tpt-appfront-templates`; the `lib.rs` is generated by
/// [`templates::preset_lib_rs`].
fn scaffold_preset_crate(
    dir: &Path,
    pkg_name: &str,
    app_title: &str,
    preset: presets::Preset,
) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::preset_cargo_toml(
            pkg_name,
            &dep_ref("tpt-appfront-core"),
            &dep_ref("tpt-appfront-dom"),
            &dep_ref("tpt-appfront-templates"),
        ),
    )?;
    fs::write(
        dir.join("src").join("lib.rs"),
        templates::preset_lib_rs(&preset, app_title),
    )?;
    fs::write(dir.join("index.html"), templates::index_html(app_title))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// add
// ---------------------------------------------------------------------------

/// Converts a PascalCase `name` to a kebab-case `class`/`route` slug.
fn kebab_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch.is_uppercase() && prev_lower {
            out.push('-');
        }
        out.push(ch.to_ascii_lowercase());
        prev_lower = ch.is_lowercase() || ch.is_numeric();
    }
    out
}

/// Picks the project's root source file (where `mod` declarations live), or
/// `None` so the caller can skip module-tree wiring. Prefers `src/lib.rs`,
/// then `src/main.rs`.
fn root_src_file(project: &Path) -> Option<PathBuf> {
    for name in ["lib.rs", "main.rs"] {
        let p = project.join("src").join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Appends `mod <dir>;` to the project's root source file if a `src/<dir>`
/// module isn't already declared there.
fn declare_module(project: &Path, dir: &str) -> anyhow::Result<()> {
    let Some(root) = root_src_file(project) else {
        return Ok(());
    };
    let decl = format!("mod {dir};");
    let content = fs::read_to_string(&root).unwrap_or_default();
    if content.contains(&decl) {
        return Ok(());
    }
    let mut appended = content;
    if !appended.ends_with('\n') {
        appended.push('\n');
    }
    appended.push_str(&decl);
    appended.push('\n');
    fs::write(&root, appended).with_context(|| format!("updating {}", root.display()))
}

fn add_component(name: &str, project: &Path) -> anyhow::Result<()> {
    validate_identifier(name)?;
    let dir = project.join("src").join("components");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let file = dir.join(format!("{}.rs", name.to_ascii_lowercase()));
    if file.exists() {
        bail!("component `{}` already exists at {}", name, file.display());
    }
    fs::write(&file, templates::component_rs(name, &kebab_case(name)))?;
    declare_module(project, "components")?;
    println!("Created component `{}` at {}", name, file.display());
    Ok(())
}

fn add_page(name: &str, project: &Path) -> anyhow::Result<()> {
    validate_identifier(name)?;
    let dir = project.join("src").join("pages");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let file = dir.join(format!("{}.rs", name.to_ascii_lowercase()));
    if file.exists() {
        bail!("page `{}` already exists at {}", name, file.display());
    }
    fs::write(&file, templates::page_rs(name, &kebab_case(name)))?;
    declare_module(project, "pages")?;
    println!("Created page `{}` at {}", name, file.display());
    Ok(())
}

/// Rejects names that aren't usable as a Rust identifier root (PascalCase
/// component/page names become both a file name and a `mod` segment).
fn validate_identifier(name: &str) -> anyhow::Result<()> {
    if name.is_empty()
        || name.contains(['/', '\\', '.', '-', ' '])
        || !name.chars().next().unwrap().is_alphabetic()
    {
        bail!("invalid name `{name}`: use a PascalCase identifier (e.g. `UserBadge`)");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// dev
// ---------------------------------------------------------------------------

fn dev(
    desktop: bool,
    web: bool,
    tui: bool,
    desktop_webview: bool,
    no_reload: bool,
    devtools: bool,
    project: &Path,
) -> anyhow::Result<()> {
    match (desktop, web, tui, desktop_webview) {
        (true, true, _, _)
        | (true, _, true, _)
        | (_, true, true, _)
        | (_, _, true, true)
        | (true, _, _, true)
        | (_, true, _, true) => {
            bail!("pass only one of --desktop, --web, --tui, or --desktop-webview")
        }
        (true, false, false, false) => {
            if no_reload {
                run_in(project, "cargo", &["run"], devtools)
            } else {
                dev_desktop_watch(project, devtools)
            }
        }
        (false, true, false, false) => run_in(project, "trunk", &["serve"], devtools)
            .context("failed to run `trunk serve` — install it with `cargo install trunk`"),
        (false, false, true, false) => {
            // `--tui` runs a native `cargo run`; with hot reload it watches the
            // crate's source and restarts on change (same poll-restart loop as
            // `--desktop`), `--no-reload` runs a single `cargo run`.
            if no_reload {
                run_in(project, "cargo", &["run"], devtools)
            } else {
                dev_watch(vec![project.to_path_buf()], move || {
                    spawn_cargo_run(project, devtools)
                })
            }
        }
        (false, false, false, true) => {
            // `--desktop-webview` hosts a `tpt-appfront-dom` trunk app from `ui/`.
            // Bail early with a clear message if that `ui/index.html` is absent
            // rather than silently running the host with nothing to display.
            let ui = ui_dir(project).ok_or_else(|| {
                anyhow::anyhow!(
                    "{} has no `ui/index.html` — `--desktop-webview` needs a trunk app in `ui/`. \
                     Scaffold one with `tpt-appfront init <name> --target dom` inside `ui/`.",
                    project.display()
                )
            })?;
            if no_reload {
                run_in(&ui, "trunk", &["build"], devtools).context(
                    "failed to run `trunk build` — install it with `cargo install trunk`",
                )?;
                run_in(project, "cargo", &["run"], devtools)
            } else {
                // Hot reload: watch both the host crate and the nested `ui/`
                // trunk app, rebuild `ui/` on change, and restart the host.
                let ui_clone = ui.clone();
                let host_clone = project.to_path_buf();
                dev_watch(
                    vec![project.to_path_buf(), ui.clone()],
                    move || -> anyhow::Result<Child> {
                        run_in(&ui_clone, "trunk", &["build"], devtools).context(
                            "failed to run `trunk build` — install it with `cargo install trunk`",
                        )?;
                        spawn_cargo_run(&host_clone, devtools)
                    },
                )
            }
        }
        (false, false, false, false) => {
            bail!(
                "specify --desktop (native window), --web (browser dev server), --tui (terminal), or --desktop-webview"
            )
        }
    }
}

/// Returns the `ui/` subdirectory (a `trunk` app) of a webview host project,
/// if it exists.
fn ui_dir(project: &Path) -> Option<PathBuf> {
    let ui = project.join("ui");
    if ui.join("index.html").exists() {
        Some(ui)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// dev --desktop watch/reload loop
// ---------------------------------------------------------------------------

/// Poll-based file watcher over a project's source tree. It re-scans on each
/// call (cheap for the small trees a dev session has) and reports whether the
/// watched set changed since the previous snapshot. A kernel-level watcher
/// (`notify`) would avoid the polling, but a dependency-free poll is enough for
/// a dev-time restart loop.
struct Watcher {
    roots: Vec<PathBuf>,
}

impl Watcher {
    fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// Collect the files to watch: every `.rs` under `src/`, plus `Cargo.toml`
    /// at the project root. `Cargo.lock` is deliberately excluded — `cargo run`
    /// rewrites/updates it on first run and on dependency resolution, which
    /// would (falsely) look like a source change and trigger a reload loop.
    /// Returns `(path, content_hash)` pairs, sorted by path for stable compare.
    fn snapshot(&self) -> Vec<(PathBuf, u64)> {
        let mut out = Vec::new();
        for root in &self.roots {
            collect_rs(&root.join("src"), &mut out);
            let p = root.join("Cargo.toml");
            if let Some(h) = file_hash(&p) {
                out.push((p, h));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// True if the watched set or any content differs from `prev`. On a change
    /// it advances `prev` to the latest snapshot so the next call restarts
    /// from there; an unchanged call leaves `prev` untouched.
    fn changed(&self, prev: &mut Vec<(PathBuf, u64)>) -> bool {
        let now = self.snapshot();
        if now != *prev {
            *prev = now;
            true
        } else {
            false
        }
    }
}

fn collect_rs(dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Some(h) = file_hash(&path) {
                out.push((path, h));
            }
        }
    }
}

/// A cheap, non-crypto hash of a file's length + contents. Used to detect
/// edits reliably across filesystems whose mtime resolution is too coarse to
/// catch rapid back-to-back saves. A read failure (e.g. a file being rewritten
/// mid-save) yields `None`, so the file drops out of the snapshot and the next
/// compare reports it as a change — a safe, idempotent outcome for a dev loop.
fn file_hash(path: &Path) -> Option<u64> {
    let data = fs::read(path).ok()?;
    let mut hasher = DefaultHasher::new();
    data.len().hash(&mut hasher);
    data.hash(&mut hasher);
    Some(hasher.finish())
}

/// `dev --desktop` watch/reload loop: spawn `cargo run`, watch the project's
/// source for changes, and restart the child process on a debounced change.
/// Compile errors don't abort the loop — the failing `cargo run` child exits,
/// the watcher keeps running, and the next save retries the build.
fn dev_desktop_watch(project: &Path, devtools: bool) -> anyhow::Result<()> {
    dev_watch(vec![project.to_path_buf()], move || {
        spawn_cargo_run(project, devtools)
    })
}

/// Generalized watch/reload loop used by every `dev` target that supports
/// hot reload (`--desktop`, `--tui`, `--desktop-webview`). `spawn` (re)starts the
/// child process; `roots` are the directories watched for source change. For
/// `--desktop-webview` that's both the host crate and the nested `ui/` trunk
/// app; `--desktop`/`--tui` watch only their single crate.
fn dev_watch(roots: Vec<PathBuf>, spawn: impl Fn() -> anyhow::Result<Child>) -> anyhow::Result<()> {
    let roots_display = roots
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let watcher = Watcher::new(roots);
    let mut baseline = watcher.snapshot();
    println!("watching {} for changes (Ctrl-C to stop)…", roots_display);

    let mut child = spawn()?;
    let poll_interval = Duration::from_millis(400);
    let debounce = Duration::from_millis(150);

    loop {
        thread::sleep(poll_interval);
        if watcher.changed(&mut baseline) {
            // Wait until changes settle before restarting, so a burst of saves
            // (or a file being rewritten mid-write) only triggers one rebuild.
            while watcher.changed(&mut baseline) {
                thread::sleep(debounce);
            }
            println!("↻ change detected — restarting…");
            kill_child(&mut child);
            child = spawn()?;
        }
    }
}

fn spawn_cargo_run(project: &Path, devtools: bool) -> anyhow::Result<Child> {
    let mut cmd = Process::new("cargo");
    cmd.args(["run"])
        .current_dir(project)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if devtools {
        cmd.env("TPT_APPFRONT_DEVTOOLS", "1");
    }
    cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!(
                "failed to spawn `cargo run`: {e} — {}",
                missing_tool_hint("cargo")
            )
        } else {
            anyhow::anyhow!("failed to spawn `cargo run` in {}: {e}", project.display())
        }
    })
}

/// Kill the `cargo run` child and its whole process tree. `cargo run` spawns a
/// child `cargo` which spawns `rustc`; killing only the top process would
/// orphan the compilers, leaving them holding the `target/` lock and stalling
/// the next build. The child deliberately stays in the CLI's own process group
/// (so a Ctrl-C still terminates it), which is why we kill the tree explicitly
/// here rather than relying on group signalling.
fn kill_child(child: &mut Child) {
    let pid = child.id();
    #[cfg(windows)]
    {
        let _ = Process::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output();
    }
    #[cfg(unix)]
    {
        kill_tree_unix(pid);
    }
    // Reap the direct child regardless of whether the OS tree-kill above ran.
    let _ = child.kill();
    let _ = child.wait();
}

/// Walk `/proc` to collect every descendant of `root` (Linux only) and SIGKILL
/// the whole tree via the `kill` binary. On non-Linux Unix (e.g. macOS, where
/// `/proc` is absent) this finds no children and the direct-child `kill()` in
/// `kill_child` remains the fallback.
#[cfg(unix)]
fn kill_tree_unix(root: u32) {
    let mut pids = vec![root];
    let mut i = 0;
    while i < pids.len() {
        let p = pids[i];
        i += 1;
        let children = format!("/proc/{p}/task/{p}/children");
        if let Ok(s) = fs::read_to_string(&children) {
            for c in s.split_whitespace() {
                if let Ok(c) = c.parse::<u32>() {
                    pids.push(c);
                }
            }
        }
    }
    for p in pids {
        let _ = Process::new("kill").args(["-9", &p.to_string()]).output();
    }
}

// ---------------------------------------------------------------------------
// build
// ---------------------------------------------------------------------------

/// A single build action produced by [`resolve_build_steps`]. Shared by the
/// `build` and `optimize` commands so the target→command mapping lives in one
/// place and can't drift between the two (todo.md Phase 11 review).
struct BuildStep {
    program: &'static str,
    args: &'static [&'static str],
    /// Only run when an `index.html` (trunk app) exists; otherwise `build`
    /// bails and `optimize` skips.
    needs_trunk_index: bool,
    /// Build the nested `ui/` trunk app first (webview host).
    builds_ui: bool,
    /// Report the largest release binary size after building.
    report_size: bool,
}

/// Maps a single target alias to its build steps. Library targets (`html`,
/// `ssr`, `ai-schema`) and the `all` aggregate are handled by the callers.
fn resolve_build_steps(target: &str) -> anyhow::Result<Vec<BuildStep>> {
    let steps = match target {
        "canvas" | "desktop" => vec![BuildStep {
            program: "cargo",
            args: &["build", "--release"],
            needs_trunk_index: false,
            builds_ui: false,
            report_size: true,
        }],
        "tui" | "terminal" => vec![BuildStep {
            program: "cargo",
            args: &["build", "--release"],
            needs_trunk_index: false,
            builds_ui: false,
            report_size: false,
        }],
        "dom" | "wasm" => vec![BuildStep {
            program: "trunk",
            args: &["build", "--release"],
            needs_trunk_index: true,
            builds_ui: false,
            report_size: false,
        }],
        "webview" => vec![BuildStep {
            program: "cargo",
            args: &["build", "--release"],
            needs_trunk_index: false,
            builds_ui: true,
            report_size: true,
        }],
        "server" => vec![BuildStep {
            program: "cargo",
            args: &["build", "--release"],
            needs_trunk_index: false,
            builds_ui: false,
            report_size: true,
        }],
        other => bail!("unknown target `{other}`; use: dom, canvas, webview, server, or all"),
    };
    Ok(steps)
}

/// Runs one [`BuildStep`] in `project`. When `strict`, a missing `index.html`
/// for a `needs_trunk_index` step is an error; otherwise it is skipped.
fn run_step(project: &Path, step: &BuildStep, strict: bool) -> anyhow::Result<()> {
    if step.builds_ui {
        if let Some(ui) = ui_dir(project) {
            run_in(&ui, "trunk", &["build", "--release"], false)
                .context("failed to run `trunk build` — install it with `cargo install trunk`")?;
        }
    }
    if step.needs_trunk_index && !project.join("index.html").exists() {
        if strict {
            bail!(
                "no `index.html` in {} — a dom/wasm target needs a trunk app",
                project.display()
            );
        }
        println!(
            "skipping {} target: no `index.html` in {}",
            step.program,
            project.display()
        );
        return Ok(());
    }
    run_in(project, step.program, step.args, false)?;
    if step.report_size {
        report_release_size(project);
    }
    Ok(())
}

fn build(target: Option<String>, project: &Path, bundle: bool) -> anyhow::Result<()> {
    let target = target.as_deref().unwrap_or("all");
    match target {
        "html" | "ssr" => {
            println!("`tpt-appfront-html`/`tpt-appfront-server` are libraries embedded in your own server binary — build your project's server crate directly, e.g. `cargo build --release -p <your-server-crate>`.");
            return Ok(());
        }
        "ai-schema" => {
            println!("`ai-schema` has no standalone build artifact — it's served at runtime via tpt-appfront-server, or embedded via `tpt_appfront_ai_schema::both(&ui)`.");
            return Ok(());
        }
        "all" => {
            println!("== canvas (native) ==");
            run_in(project, "cargo", &["build", "--release"], false)?;
            report_release_size(project);
            if project.join("index.html").exists() {
                println!("== dom (wasm) ==");
                run_in(project, "trunk", &["build", "--release"], false).context(
                    "failed to run `trunk build` — install it with `cargo install trunk`",
                )?;
            }
        }
        t => {
            for step in resolve_build_steps(t)? {
                run_step(project, &step, true)?;
            }
        }
    }
    if bundle {
        run_bundler(project)?;
    }
    Ok(())
}

/// Runs `cargo bench` in the project directory. Benchmarks must be defined by
/// the project's own crate(s) (e.g. via `#[bench]`/`criterion`); this command
/// is just the uniform `tpt-appfront` entry point for the CI pipeline.
fn benchmark(project: &Path) -> anyhow::Result<()> {
    run_in(project, "cargo", &["bench"], false)
        .context("failed to run `cargo bench` — does this crate define any benchmarks?")
}

/// Builds the project with the size-optimized profile and reports the
/// resulting artifact size. With `--bundle`, also produces installers via
/// `cargo packager`. This is the `tpt-appfront optimize --auto` CI command from
/// `todo.md` Phase 11.
fn optimize(target: &str, project: &Path, auto: bool, bundle: bool) -> anyhow::Result<()> {
    if auto {
        // The dom/wasm template already ships a size-optimized `[profile.release]`
        // (opt-level=z, lto, codegen-units=1, strip); native (canvas/webview)
        // builds use the crate's own `[profile.release]`. We don't silently
        // claim a size profile the native build doesn't use.
        println!(
            "Building release artifacts and reporting sizes. The dom/wasm template is already \
             size-optimized (opt-level=z, lto, strip); native (canvas/webview) builds use the \
             crate's [profile.release] — set opt-level = \"z\" there for minimal size."
        );
    }
    let targets: Vec<&str> = if target == "all" {
        // Size matters most for the shipped artifacts: native canvas/webview
        // binaries and the wasm bundle. html/ai-schema are libraries.
        vec!["canvas", "dom", "webview"]
    } else {
        vec![target]
    };
    for t in targets {
        for step in resolve_build_steps(t)? {
            run_step(project, &step, false)?;
        }
    }
    if bundle {
        run_bundler(project)?;
    }
    Ok(())
}

/// `tpt-appfront optimize --analyze`: a deterministic, no-AST/LLM heuristic
/// scan that flags (1) `List`/`DataGrid` usages that may need virtual scrolling
/// for large collections, and (2) a release profile missing the CLI's own
/// size-optimization flags (`opt-level`/`lto`/`codegen-units`/`strip`). Prints a
/// report; it's informational only and always returns `Ok` so it can't break a
/// CI pipeline that simply runs it (todo.md cross-cutting follow-through).
fn optimize_analyze(project: &Path) -> anyhow::Result<()> {
    let mut findings: Vec<String> = Vec::new();

    // 1) Release profile size flags.
    let cargo_toml = project.join("Cargo.toml");
    match fs::read_to_string(&cargo_toml) {
        Ok(text) => {
            let size_flags = ["opt-level", "lto", "codegen-units", "strip"];
            if !text.contains("[profile.release]") {
                findings.push(
                    "Cargo.toml has no `[profile.release]` — add one with \
                     opt-level=\"z\", lto=true, codegen-units=1, strip=true for minimal size."
                        .to_string(),
                );
            } else {
                for flag in size_flags {
                    if !text.contains(flag) {
                        findings.push(format!(
                            "Cargo.toml `[profile.release]` is missing `{flag}` (recommended for size)."
                        ));
                    }
                }
            }
        }
        Err(_) => findings.push("No Cargo.toml found in the project directory.".to_string()),
    }

    // 2) Potential unvirtualized long lists / grids.
    let mut list_hits: Vec<(PathBuf, usize)> = Vec::new();
    collect_rs_matches(&project.join("src"), &mut list_hits);
    for (path, line) in list_hits {
        findings.push(format!(
            "{}:{} — `List`/`DataGrid` usage; consider `.virtual_scroll(...)` for large collections.",
            path.display(),
            line
        ));
    }

    if findings.is_empty() {
        println!(
            "analyze: no issues found. Release profile is size-optimized and no \
             unvirtualized list usage detected."
        );
    } else {
        println!("analyze found {} item(s) to review:", findings.len());
        for f in &findings {
            println!("  - {f}");
        }
        println!("analyze: done (heuristic — review the flags above).");
    }
    Ok(())
}

/// `tpt-appfront doctor --a11y`: a deterministic, no-AST heuristic scan that
/// flags `UITree` usages in the project's `src/` that are likely missing an
/// accessible name — images with empty `alt`, links with empty text, and
/// buttons with an empty label. Prints a report; informational only and always
/// returns `Ok` so it can't break a CI pipeline that simply runs it (fits the
/// `optimize --analyze` heuristic-scan pattern, todo.md #20).
fn doctor_a11y(project: &Path) -> anyhow::Result<()> {
    let mut hits: Vec<(PathBuf, usize, String)> = Vec::new();
    collect_a11y_matches(&project.join("src"), &mut hits);

    if hits.is_empty() {
        println!(
            "a11y: no obvious issues found in src/ (heuristic scan). \
             Review form controls for an associated <label for> and DataGrid/Select roles."
        );
    } else {
        println!("a11y lint found {} item(s) to review:", hits.len());
        for (path, line, msg) in &hits {
            println!("  - {}:{} — {msg}", path.display(), line);
        }
        println!("a11y: done (heuristic — review the flags above).");
    }
    Ok(())
}

/// Recursively scans `.rs` files under `dir` for accessibility smells,
/// recording each as `(path, line, message)`. The checks are line-based and
/// intentionally dumb (no parsing of the `UITree` AST): they look for the
/// common empty-string-literal mistakes on `image`/`link`/`button` builder
/// calls.
fn collect_a11y_matches(dir: &Path, hits: &mut Vec<(PathBuf, usize, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_a11y_matches(&path, hits);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(content) = fs::read_to_string(&path) {
                for (i, line) in content.lines().enumerate() {
                    if let Some(msg) = a11y_flag(line) {
                        hits.push((path.clone(), i + 1, msg));
                    }
                }
            }
        }
    }
}

/// Returns an accessibility warning for a single source line, or `None`.
/// Heuristic only — flags the most common missing-accessible-name mistakes in
/// `UITree` builder calls.
fn a11y_flag(line: &str) -> Option<String> {
    let t = line.trim();
    // `image(src, alt)`: an empty alt string (`""`) is the second argument.
    if t.contains(".image(") && (t.contains(", \"\"") || t.contains(",\"\"") || t.contains(", \"\")") || t.contains(",\"\")")) {
        return Some(
            "image with empty alt text (`\"\"`) — confirm it's decorative, or give it a description"
                .to_string(),
        );
    }
    // `link(href, text)`: an empty link text is the second argument.
    if t.contains(".link(") && (t.contains(", \"\"") || t.contains(",\"\"") || t.contains(", \"\")") || t.contains(",\"\")")) {
        return Some(
            "link with empty text — links need accessible text, not just an href".to_string(),
        );
    }
    // `button(label)`: an empty label is the (only) argument.
    if t.contains(".button(") && (t.contains(".button(\"\")") || t.contains(".button( \"\")")) {
        return Some("button with empty label — buttons need an accessible name".to_string());
    }
    None
}

/// Recursively scans `.rs` files under `dir` for `List`/`DataGrid` usage marks
/// (`.list(`/`.data_grid(`/`.rows(`), recording each matching `path:line`.
fn collect_rs_matches(dir: &Path, hits: &mut Vec<(PathBuf, usize)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_matches(&path, hits);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(content) = fs::read_to_string(&path) {
                for (i, line) in content.lines().enumerate() {
                    if line.contains(".list(")
                        || line.contains(".data_grid(")
                        || line.contains(".rows(")
                    {
                        hits.push((path.clone(), i + 1));
                    }
                }
            }
        }
    }
}

/// Prints the size of the largest release binary built in `target/release/`,
/// so a CI run can watch the artifact-size trend over time.
fn report_release_size(project: &Path) {
    let release_dir = project.join("target").join("release");
    let Ok(entries) = fs::read_dir(&release_dir) else {
        return;
    };
    let mut largest: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Ok(meta) = entry.metadata() {
                let size = meta.len();
                if largest.as_ref().is_none_or(|(s, _)| size > *s) {
                    largest = Some((size, path));
                }
            }
        }
    }
    if let Some((size, path)) = largest {
        println!(
            "release artifact: {} — {:.2} MiB",
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size as f64 / (1024.0 * 1024.0)
        );
    }
}

/// Ensures a `packager.toml` exists (writing the template if not) and shells
/// out to `cargo packager` to produce signed installers for the host's
/// target triple. Closes the Tauri DX packaging gap (todo.md Phase 11).
fn run_bundler(project: &Path) -> anyhow::Result<()> {
    let config = project.join("packager.toml");
    if !config.exists() {
        let name = project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "app".to_string());
        fs::write(&config, templates::packager_toml(&name))
            .with_context(|| format!("writing {}", config.display()))?;
        println!("wrote {}", config.display());
    }
    run_in(project, "cargo", &["packager"], false)
        .context("failed to run `cargo packager` — install it with `cargo install cargo-packager`")
}

// ---------------------------------------------------------------------------
// fmt / lint — repo-wide hygiene wrappers
// ---------------------------------------------------------------------------

/// Returns the directories under `examples/` that contain a `Cargo.toml`. Each
/// example is a standalone crate excluded from the workspace, so it must be
/// formatted/linted with its own `--manifest-path` rather than via the
/// workspace-level commands.
fn example_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let root = PathBuf::from("examples");
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && p.join("Cargo.toml").exists() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// `tpt-appfront fmt` — formats the workspace and every standalone example.
///
/// Uses `cargo +nightly fmt`: `rustfmt.toml` enables unstable options
/// (`group_imports` / `imports_granularity` / `format_code_in_doc_comments`)
/// that the stable formatter silently ignores, so nightly must be the formatter
/// or CI's `fmt` job will reject the diff.
fn fmt_cmd() -> anyhow::Result<()> {
    run_in(
        Path::new("."),
        "cargo",
        &["+nightly", "fmt", "--all"],
        false,
    )?;
    for ex in example_dirs() {
        run_in(&ex, "cargo", &["+nightly", "fmt"], false)
            .with_context(|| format!("formatting example {}", ex.display()))?;
    }
    println!("formatted workspace + examples with `cargo +nightly fmt`");
    Ok(())
}

/// `tpt-appfront lint` — lints the workspace (webview excluded, as in CI) and
/// every standalone example with `clippy -D warnings`.
fn lint_cmd() -> anyhow::Result<()> {
    run_in(
        Path::new("."),
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--exclude",
            "tpt-appfront-webview",
            "--",
            "-D",
            "warnings",
        ],
        false,
    )?;
    for ex in example_dirs() {
        run_in(
            &ex,
            "cargo",
            &["clippy", "--all-targets", "--", "-D", "warnings"],
            false,
        )
        .with_context(|| format!("linting example {}", ex.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// doctor — pre-flight environment check
// ---------------------------------------------------------------------------

/// Result of one `doctor` check.
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

/// Runs a pre-flight environment check, printing one line per check and exiting
/// non-zero if any required check failed. Mirrors what `cargo`/toolchain checks
/// surface early instead of failing mid-build with an obscure error.
fn doctor(project: &Path) -> anyhow::Result<()> {
    let mut checks: Vec<Check> = Vec::new();

    let on_path = |prog: &str| Process::new(prog).arg("--version").output().is_ok();

    checks.push(Check {
        name: "cargo",
        ok: on_path("cargo"),
        detail: "Rust toolchain".into(),
    });

    // `trunk` is only required for dom/wasm builds.
    checks.push(Check {
        name: "trunk",
        ok: on_path("trunk"),
        detail: if on_path("trunk") {
            "installed (dom/wasm builds supported)".into()
        } else {
            "not found — `cargo install trunk` (needed for --web / dom builds)".into()
        },
    });

    // `wasm32-unknown-unknown` target, queried via `rustc`.
    let wasm_target = Process::new("rustc")
        .args(["--print", "target-list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("wasm32-unknown-unknown"))
        .unwrap_or(false);
    checks.push(Check {
        name: "wasm32-unknown-unknown target",
        ok: wasm_target,
        detail: if wasm_target {
            "installed".into()
        } else {
            "missing — `rustup target add wasm32-unknown-unknown`".into()
        },
    });

    // `cargo-packager` only needed for --bundle installers.
    checks.push(Check {
        name: "cargo-packager",
        ok: on_path("cargo"), // `cargo packager` subcommand; can't easily probe, report version-free
        detail: if on_path("cargo") {
            "available via `cargo packager` (install with `cargo install cargo-packager`)".into()
        } else {
            "unavailable".into()
        },
    });

    // Install mode: path deps (monorepo checkout) vs version deps (published).
    checks.push(Check {
        name: "dependency mode",
        ok: true,
        detail: if is_workspace_checkout() {
            "monorepo checkout — scaffolds use path deps".into()
        } else {
            "published install — scaffolds use version deps (requires crates.io publish)".into()
        },
    });

    let _ = project;

    let mut failed = false;
    for c in &checks {
        let mark = if c.ok { "ok  " } else { "FAIL" };
        println!("[{mark}] {:<28} {}", c.name, c.detail);
        if !c.ok {
            failed = true;
        }
    }

    // Informational hygiene checks — reported but never fail `doctor`, since a
    // dev may legitimately not have the local pre-commit hook installed or may
    // be mid-edit on an unformatted tree.
    let mut info: Vec<Check> = Vec::new();

    let rustfmt_ok = Process::new("rustfmt").arg("--version").output().is_ok();
    info.push(Check {
        name: "rustfmt",
        ok: rustfmt_ok,
        detail: if rustfmt_ok {
            "installed (formatting checks available)".into()
        } else {
            "missing — `rustup component add rustfmt`".into()
        },
    });

    // `tpt-appfront fmt` and CI's `fmt` job use `cargo +nightly fmt` because
    // `rustfmt.toml` enables unstable options. Report its presence (WARN, not
    // FAIL — a dev can still build/test without it).
    let nightly_fmt = Process::new("cargo")
        .args(["+nightly", "fmt", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    info.push(Check {
        name: "rustfmt (nightly)",
        ok: nightly_fmt,
        detail: if nightly_fmt {
            "installed (required for `tpt-appfront fmt` + CI `fmt` job)".into()
        } else {
            "missing — `rustup toolchain install nightly` (needed for import grouping)".into()
        },
    });

    let hooks_path = Process::new("git")
        .args(["config", "--get", "core.hooksPath"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    info.push(Check {
        name: "pre-commit hook",
        ok: hooks_path.as_deref() == Some("scripts/git-hooks"),
        detail: match hooks_path {
            Some(ref p) if p == "scripts/git-hooks" => "installed (scripts/git-hooks)".into(),
            Some(ref p) => format!("core.hooksPath = `{p}` (not the repo hook)"),
            None => "not installed — run `scripts/install-hooks.sh`".into(),
        },
    });

    let fmt_clean = Process::new("cargo")
        .args(["+nightly", "fmt", "--all", "--check"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    info.push(Check {
        name: "workspace formatted",
        ok: fmt_clean,
        detail: if fmt_clean {
            "cargo fmt --all --check passes".into()
        } else {
            "formatting drift — run `tpt-appfront fmt`".into()
        },
    });

    for c in &info {
        let mark = if c.ok { "ok  " } else { "WARN" };
        println!("[{mark}] {:<28} {}", c.name, c.detail);
    }

    if failed {
        bail!("doctor found missing required tooling — see above");
    }
    println!("doctor: environment looks good");
    Ok(())
}

// ---------------------------------------------------------------------------
// process helper
// ---------------------------------------------------------------------------

/// Builds a friendly "is `<tool>` installed?" hint, used when a spawn fails
/// because the binary isn't on `PATH` (a common, confusing failure mode for
/// `trunk`/`cargo-packager`).
fn missing_tool_hint(tool: &str) -> String {
    match tool {
        "trunk" => "is `trunk` installed? install it with `cargo install trunk`".to_string(),
        "cargo" => "is the Rust toolchain on your PATH?".to_string(),
        "cargo-packager" | "cargo packager" => {
            "is `cargo-packager` installed? install it with `cargo install cargo-packager`"
                .to_string()
        }
        other => format!("is `{other}` installed and on your PATH?"),
    }
}

fn run_in(dir: &Path, program: &str, args: &[&str], devtools: bool) -> anyhow::Result<()> {
    let mut cmd = Process::new(program);
    cmd.args(args).current_dir(dir);
    if devtools {
        cmd.env("TPT_APPFRONT_DEVTOOLS", "1");
    }
    match cmd.status() {
        Ok(status) => {
            if !status.success() {
                bail!("`{program} {}` exited with {status}", args.join(" "));
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "failed to run `{program}`: {e} — {}",
                missing_tool_hint(program)
            )
        }
        Err(e) => bail!("failed to spawn `{program}` in {}: {e}", dir.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_label_covers_every_variant() {
        assert_eq!(target_label(InitTarget::Dom), "dom");
        assert_eq!(target_label(InitTarget::Canvas), "canvas");
        assert_eq!(target_label(InitTarget::Tui), "tui");
        assert_eq!(target_label(InitTarget::Both), "canvas + dom");
    }

    #[test]
    fn dep_path_uses_forward_slashes_and_ends_with_crate_name() {
        let path = dep_path("tpt-appfront-core");
        assert!(path.ends_with("tpt-appfront-core"));
        assert!(!path.contains('\\'));
    }

    #[test]
    fn dep_ref_is_path_dep_inside_checkout_and_version_when_installed() {
        // In this repo the sibling crates exist, so we get a `path` dep.
        if is_workspace_checkout() {
            let r = dep_ref("tpt-appfront-core");
            assert!(r.starts_with("{ path ="), "expected path dep, got {r}");
        } else {
            let r = dep_ref("tpt-appfront-core");
            assert!(
                r.starts_with('"') && r.ends_with('"'),
                "expected version dep, got {r}"
            );
        }
    }

    #[test]
    fn dev_rejects_conflicting_flags() {
        let dir = PathBuf::from(".");
        assert!(dev(true, true, false, false, false, false, &dir).is_err());
        assert!(dev(true, false, true, false, false, false, &dir).is_err());
        assert!(dev(false, true, true, false, false, false, &dir).is_err());
        assert!(dev(true, false, false, true, false, false, &dir).is_err());
        assert!(dev(false, true, false, true, false, false, &dir).is_err());
        assert!(dev(false, false, true, true, false, false, &dir).is_err());
    }

    #[test]
    fn dev_requires_at_least_one_flag() {
        let dir = PathBuf::from(".");
        assert!(dev(false, false, false, false, false, false, &dir).is_err());
    }

    #[test]
    fn dev_devtools_flag_parses() {
        assert!(Cli::try_parse_from(["tpt-appfront", "dev", "--desktop", "--devtools"]).is_ok());
    }

    #[test]
    fn missing_tool_hint_names_the_right_install() {
        assert!(missing_tool_hint("trunk").contains("cargo install trunk"));
        assert!(missing_tool_hint("cargo-packager").contains("cargo install cargo-packager"));
        assert!(missing_tool_hint("cargo").contains("Rust toolchain"));
    }

    #[test]
    fn fmt_and_lint_subcommands_parse() {
        assert!(Cli::try_parse_from(["tpt-appfront", "fmt"]).is_ok());
        assert!(Cli::try_parse_from(["tpt-appfront", "lint"]).is_ok());
    }

    #[test]
    fn dev_webview_requires_a_ui_trunk_app() {
        // Without a `ui/index.html`, `--desktop-webview` must bail with a clear
        // message rather than silently running an empty host.
        let dir = std::env::temp_dir().join(format!("tpt-dev-webview-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let err = dev(false, false, false, true, false, false, &dir)
            .expect_err("expected an error for missing ui/ trunk app");
        assert!(err.to_string().contains("ui/index.html"), "got: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scaffolded_templates_emit_devtools_inspect_block() {
        // The env-gated devtools inspector must be present in every scaffold so
        // `tpt-appfront dev --devtools` can print the UITree.
        assert!(templates::dom_lib_rs("App").contains("TPT_APPFRONT_DEVTOOLS"));
        assert!(templates::canvas_main_rs("App").contains("TPT_APPFRONT_DEVTOOLS"));
        assert!(templates::tui_main_rs("App").contains("TPT_APPFRONT_DEVTOOLS"));
        assert!(templates::dom_lib_rs("App").contains("devtools::inspect_tree"));
    }

    #[test]
    fn dev_no_reload_flag_parses() {
        assert!(Cli::try_parse_from(["tpt-appfront", "dev", "--desktop", "--no-reload"]).is_ok());
        assert!(Cli::try_parse_from(["tpt-appfront", "dev", "--desktop"]).is_ok());
    }

    #[test]
    fn build_rejects_unknown_target() {
        let dir = PathBuf::from(".");
        assert!(build(Some("bogus".to_string()), &dir, false).is_err());
    }

    #[test]
    fn init_rejects_invalid_names_before_touching_disk() {
        // These names all fail validation and `bail!` before `init` creates
        // any directory, so no cwd sandboxing is needed to keep this hermetic.
        assert!(init("", InitTarget::Canvas, None, &[]).is_err());
        assert!(init("../escape", InitTarget::Canvas, None, &[]).is_err());
        assert!(init("a/b", InitTarget::Canvas, None, &[]).is_err());
        assert!(init("a\\b", InitTarget::Canvas, None, &[]).is_err());
        assert!(init("..", InitTarget::Canvas, None, &[]).is_err());
    }

    #[test]
    fn init_rejects_unknown_preset_before_touching_disk() {
        // An unknown preset name fails the `Preset::from_str` lookup and `bail!`s
        // before any directory is created.
        assert!(init("myapp", InitTarget::Both, Some("nope"), &[]).is_err());
    }

    #[test]
    fn init_with_and_preset_together_is_rejected_before_touching_disk() {
        assert!(init(
            "myapp",
            InitTarget::Both,
            Some("login"),
            &["settings".to_string()],
        )
        .is_err());
    }

    #[test]
    fn init_with_scaffolds_dom_crate_composing_pieces() {
        let dir = std::env::temp_dir().join(format!("tpt-init-with-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pieces = vec![presets::WithPiece::Dashboard, presets::WithPiece::Settings];
        assert!(scaffold_with_crate(&dir.join("myapp"), "myapp", "myapp", &pieces).is_ok());
        let root = dir.join("myapp");
        let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("tpt-appfront-templates"));
        let lib = fs::read_to_string(root.join("src").join("lib.rs")).unwrap();
        assert!(lib.contains("dashboard_shell"));
        assert!(lib.contains("settings_list"));
        // No `login` piece selected → the login form must not appear.
        assert!(!lib.contains("login_form"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_with_flag_parses() {
        assert!(Cli::try_parse_from([
            "tpt-appfront",
            "init",
            "myapp",
            "--with",
            "dashboard,settings"
        ])
        .is_ok());
        assert!(Cli::try_parse_from(["tpt-appfront", "init", "myapp", "--with", "login"]).is_ok());
    }

    #[test]
    fn init_preset_scaffolds_dom_crate_with_templates_dep() {
        let dir = std::env::temp_dir().join(format!("tpt-init-preset-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(scaffold_preset_crate(
            &dir.join("myapp"),
            "myapp",
            "myapp",
            presets::Preset::Dashboard
        )
        .is_ok());
        let root = dir.join("myapp");
        let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("tpt-appfront-templates"));
        let lib = fs::read_to_string(root.join("src").join("lib.rs")).unwrap();
        assert!(lib.contains("dashboard_shell"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_presets_cmd_prints_each_preset_name() {
        // Just ensure it runs without panicking; output goes to stdout.
        list_presets_cmd();
    }

    #[test]
    fn optimize_analyze_flag_parses_and_runs() {
        assert!(Cli::try_parse_from(["tpt-appfront", "optimize", "--analyze"]).is_ok());
        assert!(Cli::try_parse_from([
            "tpt-appfront",
            "optimize",
            "--target",
            "canvas",
            "--analyze"
        ])
        .is_ok());
    }

    #[test]
    fn analyze_flags_missing_release_profile_and_list_usage() {
        let dir = std::env::temp_dir().join(format!("tpt-analyze-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("src").join("main.rs"),
            "fn main() { let _ = container.list(|c| { c.text(\"a\"); }); }\n",
        )
        .unwrap();
        // Informational only — must not bail/panic.
        assert!(optimize_analyze(&dir).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_benchmark_optimize_and_bundle_flags_parse() {
        // Ensure the added subcommands and `--bundle` flag are wired into the
        // clap parser (no args actually executed). These only check parsing.
        assert!(Cli::try_parse_from(["tpt-appfront", "benchmark", "--project", "."]).is_ok());
        assert!(Cli::try_parse_from([
            "tpt-appfront",
            "optimize",
            "--target",
            "canvas",
            "--bundle"
        ])
        .is_ok());
        assert!(
            Cli::try_parse_from(["tpt-appfront", "build", "--target", "webview", "--bundle"])
                .is_ok()
        );
        assert!(Cli::try_parse_from(["tpt-appfront", "optimize", "--target", "bogus"]).is_ok());
    }

    #[test]
    fn generate_flags_parse() {
        assert!(Cli::try_parse_from(["tpt-appfront", "generate", "--prompt", "a counter"]).is_ok());
        assert!(Cli::try_parse_from([
            "tpt-appfront",
            "generate",
            "--prompt",
            "a counter",
            "--out",
            "ui.rs"
        ])
        .is_ok());
        // New `--llm` / `--provider` / `--model` flags parse alongside `--prompt`.
        assert!(Cli::try_parse_from([
            "tpt-appfront",
            "generate",
            "--prompt",
            "a dashboard",
            "--llm",
            "--provider",
            "anthropic",
            "--model",
            "claude-sonnet-4-5"
        ])
        .is_ok());
        assert!(Cli::try_parse_from(["tpt-appfront", "generate"]).is_err());
    }

    #[cfg(not(feature = "llm"))]
    #[test]
    fn generate_llm_bails_without_feature() {
        // Without the `llm` feature compiled in, `--llm` must error loudly
        // rather than silently producing the offline snippet.
        let err = generate_ui("a dashboard", None, true, "anthropic", "claude-sonnet-4-5", None)
            .expect_err("expected a feature error without the `llm` feature");
        assert!(err.to_string().contains("`llm` feature"), "got: {err}");
    }

    #[test]
    fn doctor_flags_parse() {
        assert!(Cli::try_parse_from(["appfront", "doctor"]).is_ok());
        assert!(Cli::try_parse_from(["appfront", "doctor", "--project", "."]).is_ok());
        assert!(Cli::try_parse_from(["appfront", "doctor", "--a11y"]).is_ok());
    }

    #[test]
    fn a11y_flag_flags_missing_accessible_names() {
        assert_eq!(
            a11y_flag(r#"    c.image("logo.png", "")"#),
            Some(
                "image with empty alt text (`\"\"`) — confirm it's decorative, or give it a description"
                    .to_string()
            )
        );
        assert_eq!(
            a11y_flag(r#"c.link("/x", "")"#),
            Some("link with empty text — links need accessible text, not just an href".to_string())
        );
        assert_eq!(
            a11y_flag(r#"c.button("")"#),
            Some("button with empty label — buttons need an accessible name".to_string())
        );
        // Decorative-but-named and non-empty cases are clean.
        assert_eq!(a11y_flag(r#"c.image("x", "a cat")"#), None);
        assert_eq!(a11y_flag(r#"c.button("Click")"#), None);
        assert_eq!(a11y_flag(r#"c.data_grid(...)"#), None);
    }

    #[test]
    fn doctor_a11y_recurses_src_and_reports_findings() {
        let dir = std::env::temp_dir().join(format!("tpt-a11y-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src").join("widgets")).unwrap();
        fs::write(
            dir.join("src").join("main.rs"),
            "fn main() { let _ = c.image(\"a.png\", \"\"); }\n",
        )
        .unwrap();
        fs::write(
            dir.join("src").join("widgets").join("nav.rs"),
            "fn nav() { let _ = c.link(\"/home\", \"\"); let _ = c.button(\"Go\"); }\n",
        )
        .unwrap();
        fs::write(
            dir.join("src").join("ok.rs"),
            "fn ok() { let _ = c.image(\"b\", \"chart\"); }\n",
        )
        .unwrap();
        // Informational only — must not bail/panic.
        assert!(doctor_a11y(&dir).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn watcher_detects_source_change_and_ignores_cargo_lock() {
        // Content-hash based, so this is reliable even when both writes land in
        // the same filesystem mtime tick (unlike an mtime-only watcher).
        let dir =
            std::env::temp_dir().join(format!("tpt-appfront-watch-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        let file = dir.join("src").join("main.rs");
        fs::write(&file, "fn main() {}\n").unwrap();

        let w = Watcher::new(vec![dir.clone()]);
        let mut snap = w.snapshot();
        assert!(!w.changed(&mut snap), "identical snapshot is not a change");

        fs::write(&file, "fn main() { let x = 1; }\n").unwrap();
        assert!(w.changed(&mut snap), "a source content edit is detected");

        // Adding Cargo.toml is a legit watched change.
        fs::write(dir.join("Cargo.toml"), "[package]\n").unwrap();
        assert!(w.changed(&mut snap), "adding Cargo.toml is detected");

        // Cargo.lock is excluded: creating it and then editing it alone must
        // NOT trigger a reload (the spurious-reload bug from review).
        let lock = dir.join("Cargo.lock");
        fs::write(&lock, "version = 3\n").unwrap();
        assert!(!w.changed(&mut snap), "adding Cargo.lock does not reload");
        fs::write(&lock, "version = 3\n# cargo rewrote this\n").unwrap();
        assert!(
            !w.changed(&mut snap),
            "editing Cargo.lock alone does not reload"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_subcommands_parse() {
        assert!(Cli::try_parse_from([
            "tpt-appfront",
            "add",
            "component",
            "UserBadge",
            "--project",
            "."
        ])
        .is_ok());
        assert!(
            Cli::try_parse_from(["tpt-appfront", "add", "page", "Settings", "--project", "."])
                .is_ok()
        );
    }

    #[test]
    fn add_component_scaffolds_file_and_declares_module() {
        let dir = std::env::temp_dir().join(format!("tpt-add-component-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src").join("lib.rs"), "// root\n").unwrap();

        assert!(add_component("UserBadge", &dir).is_ok());

        let file = dir.join("src").join("components").join("userbadge.rs");
        assert!(file.exists(), "component file created");
        let src = fs::read_to_string(&file).unwrap();
        assert!(src.contains("pub fn UserBadge() -> UITree<Msg>"));
        assert!(src.contains("class=\"user-badge\""));

        let root = fs::read_to_string(dir.join("src").join("lib.rs")).unwrap();
        assert!(
            root.contains("mod components;"),
            "module decl appended: {root}"
        );

        assert!(
            add_component("UserBadge", &dir).is_err(),
            "duplicate rejected"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_page_scaffolds_file_and_declares_module() {
        let dir = std::env::temp_dir().join(format!("tpt-add-page-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src").join("main.rs"), "// root\n").unwrap();

        assert!(add_page("Settings", &dir).is_ok());

        let file = dir.join("src").join("pages").join("settings.rs");
        assert!(file.exists(), "page file created");
        let src = fs::read_to_string(&file).unwrap();
        assert!(src.contains("pub fn Settings() -> UITree<Msg>"));
        assert!(src.contains("class=\"settings\""));

        let root = fs::read_to_string(dir.join("src").join("main.rs")).unwrap();
        assert!(root.contains("mod pages;"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_rejects_non_identifier_names() {
        let dir = std::env::temp_dir().join(format!("tpt-add-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        assert!(add_component("user-badge", &dir).is_err());
        assert!(add_component("1bad", &dir).is_err());
        assert!(add_page("", &dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_declares_module_only_once() {
        let dir = std::env::temp_dir().join(format!("tpt-add-dup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src").join("lib.rs"), "mod components;\n").unwrap();
        assert!(add_component("Widget", &dir).is_ok());
        let root = fs::read_to_string(dir.join("src").join("lib.rs")).unwrap();
        assert_eq!(
            root.matches("mod components;").count(),
            1,
            "no duplicate decl"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
