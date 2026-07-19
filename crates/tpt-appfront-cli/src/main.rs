mod generate;
mod templates;

use std::fs;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "tpt-appfront", about = "Unified UI framework for web, desktop, and AI")]
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
    /// Scaffold a new tpt-appfront project.
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
    /// Run the desktop webview shell (`tpt-appfront-webview`) via `cargo run`,
    /// hosting the `ui/` trunk build inside the OS webview.
    #[arg(long)]
    desktop_webview: bool,
    /// Disable the watch/reload loop for `--desktop` and run a single plain
    /// `cargo run` (useful when you manage reloading externally).
    #[arg(long)]
    no_reload: bool,
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
    /// Generate a `view!` UI scaffold from a text prompt. Offline and
    /// rule-based (keyword-matched against known patterns) — not a live LLM
    /// call, so it needs no API key or network access.
    Generate {
        /// Description of the UI to scaffold, e.g. "a login form".
        #[arg(long)]
        prompt: String,
        /// Write the generated snippet to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { name, target } => init(&name, target),
        Command::Dev { desktop, web, tui, desktop_webview, no_reload, project } => {
            dev(desktop, web, tui, desktop_webview, no_reload, &project)
        }
        Command::Build { target, project, bundle } => build(target, &project, bundle),
        Command::Benchmark { project } => benchmark(&project),
        Command::Optimize { target, project, auto, bundle } => {
            optimize(&target, &project, auto, bundle)
        }
        Command::Generate { prompt, out } => generate_ui(&prompt, out.as_deref()),
    }
}

// ---------------------------------------------------------------------------
// generate
// ---------------------------------------------------------------------------

fn generate_ui(prompt: &str, out: Option<&Path>) -> anyhow::Result<()> {
    let snippet = generate::generate(prompt);
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
    crates_dir().join(crate_name).to_string_lossy().replace('\\', "/")
}

/// True when this CLI is running against the `tpt-appfront` monorepo checkout
/// that built it — i.e. the sibling `crates/tpt-appfront-core/Cargo.toml` exists
/// on disk. A `cargo install tpt-appfront-cli` has no such sibling, and its
/// `CARGO_MANIFEST_DIR` points at the (absent) build-time source dir, so this
/// is false and we fall back to version dependencies instead.
fn is_workspace_checkout() -> bool {
    crates_dir().join("tpt-appfront-core").join("Cargo.toml").exists()
}

/// The version to require for crates on a published install, overridable via
/// the `TPT_APPFRONT_DEP_VERSION` env var (e.g. pinning a pre-release). Defaults to
/// this CLI's own `CARGO_PKG_VERSION`.
fn published_version() -> String {
    std::env::var("TPT_APPFRONT_DEP_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
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
        templates::canvas_cargo_toml(pkg_name, &dep_ref("tpt-appfront-core"), &dep_ref("tpt-appfront-canvas")),
    )?;
    fs::write(dir.join("src").join("main.rs"), templates::canvas_main_rs(app_title))?;
    Ok(())
}

fn scaffold_dom_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::dom_cargo_toml(pkg_name, &dep_ref("tpt-appfront-core"), &dep_ref("tpt-appfront-dom")),
    )?;
    fs::write(dir.join("src").join("lib.rs"), templates::dom_lib_rs(app_title))?;
    fs::write(dir.join("index.html"), templates::index_html(app_title))?;
    Ok(())
}

fn scaffold_tui_crate(dir: &Path, pkg_name: &str, app_title: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("Cargo.toml"),
        templates::tui_cargo_toml(pkg_name, &dep_ref("tpt-appfront-core"), &dep_ref("tpt-appfront-tui")),
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
    no_reload: bool,
    project: &Path,
) -> anyhow::Result<()> {
    match (desktop, web, tui, desktop_webview) {
        (true, true, _, _) | (true, _, true, _) | (_, true, true, _) | (_, _, true, true)
        | (true, _, _, true) | (_, true, _, true) => {
            bail!("pass only one of --desktop, --web, --tui, or --desktop-webview")
        }
        (true, false, false, false) => {
            if no_reload {
                run_in(project, "cargo", &["run"])
            } else {
                dev_desktop_watch(project)
            }
        }
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
// dev --desktop watch/reload loop
// ---------------------------------------------------------------------------

/// Poll-based file watcher over a project's source tree. It re-scans on each
/// call (cheap for the small trees a dev session has) and reports whether the
/// watched set changed since the previous snapshot. A kernel-level watcher
/// (`notify`) would avoid the polling, but a dependency-free poll is enough for
/// a dev-time restart loop.
struct Watcher {
    root: PathBuf,
}

impl Watcher {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Collect the files to watch: every `.rs` under `src/`, plus `Cargo.toml`
    /// at the project root. `Cargo.lock` is deliberately excluded — `cargo run`
    /// rewrites/updates it on first run and on dependency resolution, which
    /// would (falsely) look like a source change and trigger a reload loop.
    /// Returns `(path, content_hash)` pairs, sorted by path for stable compare.
    fn snapshot(&self) -> Vec<(PathBuf, u64)> {
        let mut out = Vec::new();
        collect_rs(&self.root.join("src"), &mut out);
        let p = self.root.join("Cargo.toml");
        if let Some(h) = file_hash(&p) {
            out.push((p, h));
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
fn dev_desktop_watch(project: &Path) -> anyhow::Result<()> {
    let watcher = Watcher::new(project.to_path_buf());
    let mut baseline = watcher.snapshot();
    println!(
        "watching {} for changes (Ctrl-C to stop)…",
        project.display()
    );

    let mut child = spawn_cargo_run(project)?;
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
            child = spawn_cargo_run(project)?;
        }
    }
}

fn spawn_cargo_run(project: &Path) -> anyhow::Result<Child> {
    Process::new("cargo")
        .args(["run"])
        .current_dir(project)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn `cargo run` in {}", project.display()))
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
        other => bail!("unknown target `{other}`; use: dom, canvas, webview, or all"),
    };
    Ok(steps)
}

/// Runs one [`BuildStep`] in `project`. When `strict`, a missing `index.html`
/// for a `needs_trunk_index` step is an error; otherwise it is skipped.
fn run_step(project: &Path, step: &BuildStep, strict: bool) -> anyhow::Result<()> {
    if step.builds_ui {
        if let Some(ui) = ui_dir(project) {
            run_in(&ui, "trunk", &["build", "--release"])
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
    run_in(project, step.program, step.args)?;
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
            run_in(project, "cargo", &["build", "--release"])?;
            report_release_size(project);
            if project.join("index.html").exists() {
                println!("== dom (wasm) ==");
                run_in(project, "trunk", &["build", "--release"])
                    .context("failed to run `trunk build` — install it with `cargo install trunk`")?;
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
    run_in(project, "cargo", &["bench"])
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
            assert!(r.starts_with('"') && r.ends_with('"'), "expected version dep, got {r}");
        }
    }

    #[test]
    fn dev_rejects_conflicting_flags() {
        let dir = PathBuf::from(".");
        assert!(dev(true, true, false, false, false, &dir).is_err());
        assert!(dev(true, false, true, false, false, &dir).is_err());
        assert!(dev(false, true, true, false, false, &dir).is_err());
        assert!(dev(true, false, false, true, false, &dir).is_err());
        assert!(dev(false, true, false, true, false, &dir).is_err());
        assert!(dev(false, false, true, true, false, &dir).is_err());
    }

    #[test]
    fn dev_requires_at_least_one_flag() {
        let dir = PathBuf::from(".");
        assert!(dev(false, false, false, false, false, &dir).is_err());
    }

    #[test]
    fn dev_no_reload_flag_parses() {
        assert!(Cli::try_parse_from([
            "tpt-appfront", "dev", "--desktop", "--no-reload"
        ])
        .is_ok());
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
        assert!(Cli::try_parse_from(["tpt-appfront", "benchmark", "--project", "."]).is_ok());
        assert!(Cli::try_parse_from([
            "tpt-appfront", "optimize", "--target", "canvas", "--bundle"
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "tpt-appfront", "build", "--target", "webview", "--bundle"
        ])
        .is_ok());
        assert!(Cli::try_parse_from(["tpt-appfront", "optimize", "--target", "bogus"]).is_ok());
    }

    #[test]
    fn generate_flags_parse() {
        assert!(Cli::try_parse_from(["tpt-appfront", "generate", "--prompt", "a counter"]).is_ok());
        assert!(Cli::try_parse_from([
            "tpt-appfront", "generate", "--prompt", "a counter", "--out", "ui.rs"
        ])
        .is_ok());
        assert!(Cli::try_parse_from(["tpt-appfront", "generate"]).is_err());
    }

    #[test]
    fn watcher_detects_source_change_and_ignores_cargo_lock() {
        // Content-hash based, so this is reliable even when both writes land in
        // the same filesystem mtime tick (unlike an mtime-only watcher).
        let dir = std::env::temp_dir().join(format!("tpt-appfront-watch-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        let file = dir.join("src").join("main.rs");
        fs::write(&file, "fn main() {}\n").unwrap();

        let w = Watcher::new(dir.clone());
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
        assert!(!w.changed(&mut snap), "editing Cargo.lock alone does not reload");

        let _ = fs::remove_dir_all(&dir);
    }
}
