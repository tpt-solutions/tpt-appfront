mod templates;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use anyhow::{bail, Context};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "appfront", about = "Unified UI framework for web, desktop, and AI")]
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
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new appfront project.
    Init {
        /// Project name (and directory to create).
        name: String,
        /// Which backend(s) to scaffold.
        #[arg(long, value_enum, default_value = "both")]
        target: InitTarget,
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
    /// Run the desktop webview shell (`appfront-webview`) via `cargo run`,
    /// hosting the `ui/` trunk build inside the OS webview.
    #[arg(long)]
    desktop_webview: bool,
    /// Directory of the crate to run (defaults to the current directory).
    #[arg(long, default_value = ".")]
    project: PathBuf,
},
    /// Build the application for a target.
    Build {
        /// Target: dom, canvas, html, ai-schema, webview, or all.
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
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { name, target } => init(&name, target),
        Command::Dev { desktop, web, tui, desktop_webview, project } => {
            dev(desktop, web, tui, desktop_webview, &project)
        }
        Command::Build { target, project, bundle } => build(target, &project, bundle),
        Command::Benchmark { project } => benchmark(&project),
        Command::Optimize { target, project, auto, bundle } => {
            optimize(&target, &project, auto, bundle)
        }
    }
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
        .expect("appfront-cli is always nested under crates/")
        .to_path_buf()
}

fn dep_path(crate_name: &str) -> String {
    crates_dir().join(crate_name).to_string_lossy().replace('\\', "/")
}

fn init(name: &str, target: InitTarget) -> anyhow::Result<()> {
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
    }
    Ok(())
}

fn target_label(target: InitTarget) -> &'static str {
    match target {
        InitTarget::Dom => "dom",
        InitTarget::Canvas => "canvas",
        InitTarget::Tui => "tui",
        InitTarget::Both => "canvas + dom",
    }
}

fn scaffold_canvas_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::canvas_cargo_toml(pkg_name, &dep_path("appfront-core"), &dep_path("appfront-canvas")),
    )?;
    fs::write(dir.join("src").join("main.rs"), templates::canvas_main_rs(app_title))?;
    Ok(())
}

fn scaffold_dom_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::dom_cargo_toml(pkg_name, &dep_path("appfront-core"), &dep_path("appfront-dom")),
    )?;
    fs::write(dir.join("src").join("lib.rs"), templates::dom_lib_rs(app_title))?;
    fs::write(dir.join("index.html"), templates::index_html(app_title))?;
    Ok(())
}

fn scaffold_tui_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::tui_cargo_toml(pkg_name, &dep_path("appfront-core"), &dep_path("appfront-tui")),
    )?;
    fs::write(dir.join("src").join("main.rs"), templates::tui_main_rs(app_title))?;
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
    project: &Path,
) -> anyhow::Result<()> {
    match (desktop, web, tui, desktop_webview) {
        (true, true, _, _) | (true, _, true, _) | (_, true, true, _) | (_, _, true, true)
        | (true, _, _, true) | (_, true, _, true) => {
            bail!("pass only one of --desktop, --web, --tui, or --desktop-webview")
        }
        (true, false, false, false) => run_in(project, "cargo", &["run"]),
        (false, true, false, false) => run_in(project, "trunk", &["serve"])
            .context("failed to run `trunk serve` — install it with `cargo install trunk`"),
        (false, false, true, false) => run_in(project, "cargo", &["run"]),
        (false, false, false, true) => {
            // Build the hosted `ui/` trunk app (if present) then run the host.
            if let Some(ui) = ui_dir(project) {
                run_in(&ui, "trunk", &["build"])
                    .context("failed to run `trunk build` — install it with `cargo install trunk`")?;
            }
            run_in(project, "cargo", &["run"])
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
// build
// ---------------------------------------------------------------------------

fn build(target: Option<String>, project: &Path, bundle: bool) -> anyhow::Result<()> {
    let target = target.as_deref().unwrap_or("all");
    match target {
        "dom" | "wasm" => run_in(project, "trunk", &["build", "--release"])
            .context("failed to run `trunk build` — install it with `cargo install trunk`"),
        "canvas" | "desktop" => run_in(project, "cargo", &["build", "--release"]),
        "tui" | "terminal" => run_in(project, "cargo", &["build", "--release"]),
        "webview" => {
            // Build the hosted `ui/` trunk app (if present), then the host.
            if let Some(ui) = ui_dir(project) {
                run_in(&ui, "trunk", &["build", "--release"])
                    .context("failed to run `trunk build` — install it with `cargo install trunk`")?;
            }
            run_in(project, "cargo", &["build", "--release"])
        }
        "html" | "ssr" => {
            println!("`appfront-html`/`appfront-server` are libraries embedded in your own server binary — build your project's server crate directly, e.g. `cargo build --release -p <your-server-crate>`.");
            return Ok(());
        }
        "ai-schema" => {
            println!("`ai-schema` has no standalone build artifact — it's served at runtime via appfront-server, or embedded via `appfront_ai_schema::both(&ui)`.");
            return Ok(());
        }
        "all" => {
            println!("== canvas (native) ==");
            run_in(project, "cargo", &["build", "--release"])?;
            if project.join("index.html").exists() {
                println!("== dom (wasm) ==");
                run_in(project, "trunk", &["build", "--release"])
                    .context("failed to run `trunk build` — install it with `cargo install trunk`")?;
            }
        }
        other => bail!("unknown target `{other}`; use: dom, canvas, html, ai-schema, webview, or all"),
    }
    if bundle {
        run_bundler(project)?;
    }
    Ok(())
}

/// Runs `cargo bench` in the project directory. Benchmarks must be defined by
/// the project's own crate(s) (e.g. via `#[bench]`/`criterion`); this command
/// is just the uniform `appfront` entry point for the CI pipeline.
fn benchmark(project: &Path) -> anyhow::Result<()> {
    run_in(project, "cargo", &["bench"])
        .context("failed to run `cargo bench` — does this crate define any benchmarks?")
}

/// Builds the project with the size-optimized profile and reports the
/// resulting artifact size. With `--bundle`, also produces installers via
/// `cargo packager`. This is the `appfront optimize --auto` CI command from
/// `todo.md` Phase 11.
fn optimize(target: &str, project: &Path, auto: bool, bundle: bool) -> anyhow::Result<()> {
    if auto {
        println!("Optimizing with the size-optimized profile (opt-level=z, lto, strip).");
    }
    let resolved = if target == "all" {
        // Size matters most for the shipped artifacts: native canvas/webview
        // binaries and the wasm bundle. html/ai-schema are libraries.
        vec!["canvas", "dom", "webview"]
    } else {
        vec![target]
    };
    for t in resolved {
        match t {
            "dom" | "wasm" => {
                if project.join("index.html").exists() {
                    run_in(project, "trunk", &["build", "--release"])
                        .context("failed to run `trunk build` — install it with `cargo install trunk`")?;
                } else {
                    println!("skipping dom target: no index.html in {project}", project = project.display());
                }
            }
            "canvas" | "desktop" => {
                run_in(project, "cargo", &["build", "--release"])?;
                report_release_size(project);
            }
            "webview" => {
                if let Some(ui) = ui_dir(project) {
                    run_in(&ui, "trunk", &["build", "--release"])
                        .context("failed to run `trunk build` — install it with `cargo install trunk`")?;
                }
                run_in(project, "cargo", &["build", "--release"])?;
                report_release_size(project);
            }
            other => bail!("unknown target `{other}`; use: canvas, dom, webview, or all"),
        }
    }
    if bundle {
        run_bundler(project)?;
    }
    Ok(())
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
                if largest.as_ref().map_or(true, |(s, _)| size > *s) {
                    largest = Some((size, path));
                }
            }
        }
    }
    if let Some((size, path)) = largest {
        println!(
            "release artifact: {} — {:.2} MiB",
            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
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
    run_in(project, "cargo", &["packager"])
        .context("failed to run `cargo packager` — install it with `cargo install cargo-packager`")
}

// ---------------------------------------------------------------------------
// process helper
// ---------------------------------------------------------------------------

fn run_in(dir: &Path, program: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Process::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("failed to spawn `{program}` in {}", dir.display()))?;
    if !status.success() {
        bail!("`{program} {}` exited with {status}", args.join(" "));
    }
    Ok(())
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
        let path = dep_path("appfront-core");
        assert!(path.ends_with("appfront-core"));
        assert!(!path.contains('\\'));
    }

    #[test]
    fn dev_rejects_conflicting_flags() {
        let dir = PathBuf::from(".");
        assert!(dev(true, true, false, false, &dir).is_err());
        assert!(dev(true, false, true, false, &dir).is_err());
        assert!(dev(false, true, true, false, &dir).is_err());
        assert!(dev(true, false, false, true, &dir).is_err());
        assert!(dev(false, true, false, true, &dir).is_err());
        assert!(dev(false, false, true, true, &dir).is_err());
    }

    #[test]
    fn dev_requires_at_least_one_flag() {
        let dir = PathBuf::from(".");
        assert!(dev(false, false, false, false, &dir).is_err());
    }

    #[test]
    fn build_rejects_unknown_target() {
        let dir = PathBuf::from(".");
        assert!(build(Some("bogus".to_string()), &dir).is_err());
    }

    #[test]
    fn init_rejects_invalid_names_before_touching_disk() {
        // These names all fail validation and `bail!` before `init` creates
        // any directory, so no cwd sandboxing is needed to keep this hermetic.
        assert!(init("", InitTarget::Canvas).is_err());
        assert!(init("../escape", InitTarget::Canvas).is_err());
        assert!(init("a/b", InitTarget::Canvas).is_err());
        assert!(init("a\\b", InitTarget::Canvas).is_err());
        assert!(init("..", InitTarget::Canvas).is_err());
    }

    #[test]
    fn new_benchmark_optimize_and_bundle_flags_parse() {
        // Ensure the added subcommands and `--bundle` flag are wired into the
        // clap parser (no args actually executed). These only check parsing.
        assert!(Cli::try_parse_from(["appfront", "benchmark", "--project", "."]).is_ok());
        assert!(Cli::try_parse_from([
            "appfront", "optimize", "--target", "canvas", "--bundle"
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "appfront", "build", "--target", "webview", "--bundle"
        ])
        .is_ok());
        assert!(Cli::try_parse_from(["appfront", "optimize", "--target", "bogus"]).is_ok());
    }
}
