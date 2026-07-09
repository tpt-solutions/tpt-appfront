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
        /// Directory of the crate to run (defaults to the current directory).
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Build the application for a target.
    Build {
        /// Target: dom, canvas, html, ai-schema, or all.
        #[arg(long)]
        target: Option<String>,
        /// Directory of the crate to build (defaults to the current directory).
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { name, target } => init(&name, target),
        Command::Dev { desktop, web, project } => dev(desktop, web, &project),
        Command::Build { target, project } => build(target, &project),
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
    }
    Ok(())
}

fn target_label(target: InitTarget) -> &'static str {
    match target {
        InitTarget::Dom => "dom",
        InitTarget::Canvas => "canvas",
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

// ---------------------------------------------------------------------------
// dev
// ---------------------------------------------------------------------------

fn dev(desktop: bool, web: bool, project: &Path) -> anyhow::Result<()> {
    match (desktop, web) {
        (true, true) => bail!("pass only one of --desktop or --web"),
        (true, false) => run_in(project, "cargo", &["run"]),
        (false, true) => run_in(project, "trunk", &["serve"])
            .context("failed to run `trunk serve` — install it with `cargo install trunk`"),
        (false, false) => bail!("specify --desktop (native window) or --web (browser dev server)"),
    }
}

// ---------------------------------------------------------------------------
// build
// ---------------------------------------------------------------------------

fn build(target: Option<String>, project: &Path) -> anyhow::Result<()> {
    let target = target.as_deref().unwrap_or("all");
    match target {
        "dom" | "wasm" => run_in(project, "trunk", &["build", "--release"])
            .context("failed to run `trunk build` — install it with `cargo install trunk`"),
        "canvas" | "desktop" => run_in(project, "cargo", &["build", "--release"]),
        "html" | "ssr" => {
            println!("`appfront-html`/`appfront-server` are libraries embedded in your own server binary — build your project's server crate directly, e.g. `cargo build --release -p <your-server-crate>`.");
            Ok(())
        }
        "ai-schema" => {
            println!("`ai-schema` has no standalone build artifact — it's served at runtime via appfront-server, or embedded via `appfront_ai_schema::both(&ui)`.");
            Ok(())
        }
        "all" => {
            println!("== canvas (native) ==");
            run_in(project, "cargo", &["build", "--release"])?;
            if project.join("index.html").exists() {
                println!("== dom (wasm) ==");
                run_in(project, "trunk", &["build", "--release"])
                    .context("failed to run `trunk build` — install it with `cargo install trunk`")?;
            }
            Ok(())
        }
        other => bail!("unknown target `{other}`; use: dom, canvas, html, ai-schema, or all"),
    }
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
