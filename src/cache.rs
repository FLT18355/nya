use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{execute, queue};

use crate::ui;
use crate::util;

#[derive(Clone)]
struct CacheSpec {
    name: &'static str,
    label: &'static str,
    paths: Vec<PathBuf>,
}

struct CacheItem {
    spec: CacheSpec,
    size: u64,
    selected: bool,
}

#[derive(Default)]
struct RemoveStats {
    bytes: u64,
    files: u64,
    errors: u64,
}

pub fn run(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }
    if let Some(flag) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(format!("未知选项喵: {flag}"));
    }

    let requested: Vec<String> = args.iter().map(|arg| arg.to_lowercase()).collect();
    if requested.is_empty() && (!io::stdin().is_terminal() || !io::stdout().is_terminal()) {
        return Err("交互选择需要终端喵\n可直接指定: nya cache npm uv pip".to_owned());
    }
    let all = all_specs();
    let known: Vec<&str> = all.iter().map(|spec| spec.name).collect();
    if let Some(unknown) = requested
        .iter()
        .find(|name| !known.contains(&name.as_str()))
    {
        return Err(format!(
            "未知缓存喵: {unknown}\n可用名称: {}",
            known.join(" ")
        ));
    }

    let selected_specs: Vec<CacheSpec> = all
        .into_iter()
        .filter(|spec| requested.is_empty() || requested.contains(&spec.name.to_string()))
        .collect();

    if selected_specs.iter().all(|spec| spec.paths.is_empty()) {
        ui::header(ui::ICON_TRASH, "缓存清理", "选好再清理喵；默认全不选。");
        println!("  {}", ui::paint("没有可清理的缓存喵。", ui::OVERLAY1));
        println!();
        return Ok(());
    }

    ui::header(ui::ICON_TRASH, "缓存清理", "选好再清理喵；默认全不选。");
    let mut items = scan_items(selected_specs);

    if !requested.is_empty() {
        let missing: Vec<String> = requested
            .iter()
            .filter(|name| {
                !items
                    .iter()
                    .any(|item| item.spec.name.eq_ignore_ascii_case(name))
            })
            .cloned()
            .collect();
        for name in missing {
            ui::hint(&format!("{name}: 未找到缓存，跳过喵。"));
        }
        if items.is_empty() {
            println!("  {}", ui::paint("没有可清理的缓存喵。", ui::OVERLAY1));
            println!();
            return Ok(());
        }
        for item in &mut items {
            item.selected = requested.iter().any(|name| name == item.spec.name);
        }
    } else {
        if pick(&mut items)?.is_none() {
            println!(
                "  {}",
                ui::paint("已取消，未删除任何内容喵。", ui::OVERLAY1)
            );
            println!();
            return Ok(());
        }
    }

    let chosen: Vec<&CacheItem> = items.iter().filter(|item| item.selected).collect();
    if chosen.is_empty() {
        println!("  {}", ui::paint("未选择缓存喵。", ui::OVERLAY1));
        println!();
        return Ok(());
    }
    clean(&chosen);
    Ok(())
}

fn print_help() {
    println!();
    println!(
        "  {}  {}",
        ui::paint(ui::ICON_TRASH, ui::MAUVE),
        ui::paint_bold("缓存清理", ui::TEXT)
    );
    println!();
    println!("  {}", ui::paint("用法: nya cache [名称...]", ui::TEXT));
    println!(
        "  {}",
        ui::paint(
            "不带名称时进入交互选择，默认不选择任何缓存喵。",
            ui::OVERLAY1
        )
    );
    println!();
    println!("  {}", ui::paint_bold("可用名称", ui::LAVENDER));
    let names: Vec<&str> = all_specs().iter().map(|spec| spec.name).collect();
    for chunk in names.chunks(6) {
        println!("    {}", ui::paint(chunk.join("  "), ui::TEXT));
    }
    println!();
    println!(
        "  {}",
        ui::paint(
            "只删除已知缓存目录喵，不删除项目文件、配置或虚拟环境。",
            ui::OVERLAY1
        )
    );
    println!();
}

fn all_specs() -> Vec<CacheSpec> {
    let home = util::home_dir();
    let cache = util::cache_home();
    let data = util::data_home();
    let local = util::env_path("LOCALAPPDATA");

    vec![
        spec(
            "bun",
            "bun",
            vec![
                util::env_path("BUN_INSTALL").map(|path| path.join("install/cache")),
                home.as_ref().map(|path| path.join(".bun/install/cache")),
                cache.as_ref().map(|path| path.join("bun")),
            ],
        ),
        spec(
            "npm",
            "npm",
            vec![
                util::env_path("npm_config_cache")
                    .or_else(|| util::env_path("NPM_CONFIG_CACHE"))
                    .map(|path| path.join("_cacache")),
                home.as_ref().map(|path| path.join(".npm/_cacache")),
            ],
        ),
        spec(
            "pnpm",
            "pnpm",
            vec![
                data.as_ref().map(|path| path.join("pnpm/store")),
                home.as_ref()
                    .map(|path| path.join(".local/share/pnpm/store")),
                home.as_ref().map(|path| path.join("Library/pnpm/store")),
                local.as_ref().map(|path| path.join("pnpm/store")),
                home.as_ref().map(|path| path.join(".pnpm-store")),
            ],
        ),
        spec(
            "yarn",
            "yarn",
            vec![
                util::env_path("YARN_CACHE_FOLDER"),
                cache.as_ref().map(|path| path.join("yarn")),
                home.as_ref().map(|path| path.join(".yarn/berry/cache")),
                local.as_ref().map(|path| path.join("Yarn/Cache")),
            ],
        ),
        spec(
            "deno",
            "deno",
            vec![
                util::env_path("DENO_DIR"),
                cache.as_ref().map(|path| path.join("deno")),
            ],
        ),
        spec(
            "go",
            "go",
            vec![
                util::env_path("GOMODCACHE"),
                home.as_ref().map(|path| path.join("go/pkg/mod")),
                util::env_path("GOCACHE"),
                cache.as_ref().map(|path| path.join("go-build")),
            ],
        ),
        spec(
            "uv",
            "uv",
            vec![
                util::env_path("UV_CACHE_DIR"),
                cache.as_ref().map(|path| path.join("uv")),
            ],
        ),
        spec(
            "pip",
            "pip",
            vec![
                util::env_path("PIP_CACHE_DIR"),
                cache.as_ref().map(|path| path.join("pip")),
            ],
        ),
        spec(
            "poetry",
            "poetry",
            vec![
                // Poetry keeps virtualenvs under the same root; never touch those.
                cache.as_ref().map(|path| path.join("pypoetry/cache")),
                cache.as_ref().map(|path| path.join("pypoetry/artifacts")),
            ],
        ),
        spec("cargo", "cargo", {
            let cargo_home = util::env_path("CARGO_HOME")
                .or_else(|| home.as_ref().map(|path| path.join(".cargo")));
            vec![
                cargo_home.as_ref().map(|path| path.join("registry/cache")),
                cargo_home.as_ref().map(|path| path.join("git/db")),
            ]
        }),
        spec("rustup", "rustup", {
            let rustup_home = util::env_path("RUSTUP_HOME")
                .or_else(|| home.as_ref().map(|path| path.join(".rustup")));
            vec![
                rustup_home.as_ref().map(|path| path.join("downloads")),
                rustup_home.as_ref().map(|path| path.join("tmp")),
            ]
        }),
        spec(
            "composer",
            "composer",
            vec![
                util::env_path("COMPOSER_CACHE_DIR"),
                cache.as_ref().map(|path| path.join("composer")),
            ],
        ),
        spec(
            "gradle",
            "gradle",
            vec![
                util::env_path("GRADLE_USER_HOME")
                    .or_else(|| home.as_ref().map(|path| path.join(".gradle")))
                    .map(|path| path.join("caches/modules-2")),
            ],
        ),
        spec(
            "brew",
            "brew",
            vec![
                util::env_path("HOMEBREW_CACHE"),
                cache.as_ref().map(|path| path.join("Homebrew")),
                home.as_ref()
                    .map(|path| path.join("Library/Caches/Homebrew")),
            ],
        ),
    ]
}

fn spec(name: &'static str, label: &'static str, candidates: Vec<Option<PathBuf>>) -> CacheSpec {
    let mut paths = Vec::new();
    for path in candidates.into_iter().flatten() {
        if path.exists() && !paths.contains(&path) && safe_to_delete(&path) {
            paths.push(path);
        }
    }
    CacheSpec { name, label, paths }
}

fn scan_items(specs: Vec<CacheSpec>) -> Vec<CacheItem> {
    let specs: Vec<CacheSpec> = specs
        .into_iter()
        .filter(|spec| !spec.paths.is_empty())
        .collect();
    let total = specs.len();
    let mut items = Vec::with_capacity(total);
    for (index, spec) in specs.into_iter().enumerate() {
        let size = if ui::is_tty() {
            scan_with_spinner(&spec, index, total)
        } else {
            stats_for_paths(&spec.paths)
        };
        items.push(CacheItem {
            spec,
            size,
            selected: false,
        });
    }
    ui::end_live_line();
    items
}

fn scan_with_spinner(spec: &CacheSpec, index: usize, total: usize) -> u64 {
    let paths = spec.paths.clone();
    let (sender, receiver) = mpsc::channel();
    let mut result = None;
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let _ = sender.send(stats_for_paths(&paths));
        });
        let started = Instant::now();
        let mut frame = 0usize;
        loop {
            match receiver.recv_timeout(Duration::from_millis(80)) {
                Ok(stats) => {
                    result = Some(stats);
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let line = format!(
                        "  {} {}  {}",
                        ui::paint(ui::spinner(frame), ui::PINK),
                        ui::paint(
                            format!("扫描 {}/{} · {} 缓存喵", index + 1, total, spec.label),
                            ui::MAUVE
                        ),
                        ui::paint(
                            format!("{:.0}s", started.elapsed().as_secs_f64()),
                            ui::OVERLAY1
                        )
                    );
                    ui::live_line(&line);
                    frame = frame.wrapping_add(1);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    result.unwrap_or(0)
}

fn stats_for_paths(paths: &[PathBuf]) -> u64 {
    let mut bytes = 0;
    for path in paths {
        stats_for_path(path, &mut bytes);
    }
    bytes
}

fn stats_for_path(path: &Path, bytes: &mut u64) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        *bytes = bytes.saturating_add(meta.len());
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        stats_for_path(&entry.path(), bytes);
    }
}

fn pick(items: &mut [CacheItem]) -> Result<Option<()>, String> {
    let (width, height) = terminal::size().map_err(|error| format!("读取终端大小失败: {error}"))?;
    if width < 48 || height < 10 {
        return Err("终端过小，请放大终端后重试".to_owned());
    }
    enable_raw_mode().map_err(|error| format!("进入交互模式失败: {error}"))?;
    let _guard = ScreenGuard;
    execute!(io::stdout(), EnterAlternateScreen, Hide)
        .map_err(|error| format!("打开选择界面失败: {error}"))?;
    pick_loop(items)
}

struct ScreenGuard;

impl Drop for ScreenGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
    }
}

fn pick_loop(items: &mut [CacheItem]) -> Result<Option<()>, String> {
    let mut current = 0usize;
    let mut offset = 0usize;
    let mut note: Option<String> = None;
    loop {
        draw(items, current, &mut offset, note.as_deref())
            .map_err(|error| format!("绘制选择界面失败: {error}"))?;
        let event = event::read().map_err(|error| format!("读取按键失败: {error}"))?;
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
            KeyCode::Up | KeyCode::Char('k') => {
                current = if current == 0 {
                    items.len() - 1
                } else {
                    current - 1
                };
                note = None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                current = (current + 1) % items.len();
                note = None;
            }
            KeyCode::Char(' ') => {
                items[current].selected = !items[current].selected;
                note = None;
            }
            KeyCode::Char('a') => {
                let all = items.iter().all(|item| item.selected);
                for item in items.iter_mut() {
                    item.selected = !all;
                }
                note = None;
            }
            KeyCode::Enter => {
                if items.iter().any(|item| item.selected) {
                    return Ok(Some(()));
                }
                note = Some("未选择缓存喵。按 space 选择，enter 清理。".to_owned());
            }
            _ => {
                note = None;
            }
        }
    }
}

fn draw(
    items: &[CacheItem],
    current: usize,
    offset: &mut usize,
    note: Option<&str>,
) -> io::Result<()> {
    let (width, height) = terminal::size()?;
    let width = width as usize;
    let height = height as usize;
    let visible = height.saturating_sub(7).max(1);
    if current < *offset {
        *offset = current;
    }
    if current >= *offset + visible {
        *offset = current + 1 - visible;
    }

    let selected_count = items.iter().filter(|item| item.selected).count();
    let selected_size: u64 = items
        .iter()
        .filter(|item| item.selected)
        .map(|item| item.size)
        .sum();

    let mut out = io::stdout().lock();
    queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;
    write!(
        out,
        "  {}  {}\r\n",
        ui::paint(ui::ICON_TRASH, ui::MAUVE),
        ui::paint_bold("缓存清理", ui::TEXT)
    )?;
    write!(
        out,
        "  {}\r\n\r\n",
        ui::paint("选择要清理的缓存喵。默认不选。", ui::OVERLAY2)
    )?;

    for index in (*offset..items.len()).take(visible) {
        let item = &items[index];
        let path_text = display_paths(&item.spec.paths);
        let max_path = width.saturating_sub(30);
        let path_text = if max_path >= 8 {
            util::truncate_width(&path_text, max_path)
        } else {
            String::new()
        };
        let marker = if index == current {
            ui::ICON_CARET_RIGHT
        } else {
            " "
        };
        let check = if item.selected {
            ui::ICON_CHECK_SQUARE
        } else {
            ui::ICON_SQUARE
        };
        let check_color = if item.selected {
            ui::MAUVE
        } else {
            ui::OVERLAY0
        };
        let name_color = if index == current {
            ui::LAVENDER
        } else {
            ui::TEXT
        };
        let size = util::human_size(item.size);
        write!(
            out,
            "  {} {} {}  {}  {}\r\n",
            ui::paint(marker, ui::PINK),
            ui::paint(check, check_color),
            ui::paint_bold(format!("{:<10}", item.spec.label), name_color),
            ui::paint(format!("{size:>9}"), ui::OVERLAY2),
            ui::paint(path_text, ui::OVERLAY0)
        )?;
    }

    for _ in items.len().saturating_sub(*offset).min(visible)..visible {
        write!(out, "\r\n")?;
    }

    write!(out, "\r\n  {}\r\n", ui::divider(width.saturating_sub(4)))?;
    write!(
        out,
        "  {} {}   {}\r\n",
        ui::paint("已选", ui::OVERLAY1),
        ui::paint_bold(format!("{selected_count} 项"), ui::MAUVE),
        ui::paint(
            format!("可释放 {}", util::human_size(selected_size)),
            ui::OVERLAY2
        )
    )?;
    if let Some(note) = note {
        write!(out, "  {}\r\n", ui::paint(note, ui::YELLOW))?;
    } else {
        let hints = if width >= 70 {
            "↑↓ 移动  ·  space 选择  ·  a 全选  ·  enter 清理  ·  q 取消喵"
        } else {
            "↑↓  space  a  enter  q 喵"
        };
        write!(out, "  {}\r\n", ui::paint(hints, ui::OVERLAY1))?;
    }
    out.flush()
}

fn display_paths(paths: &[PathBuf]) -> String {
    let Some(first) = paths.first() else {
        return String::new();
    };
    let mut text = util::pretty_path(first);
    if paths.len() > 1 {
        text.push_str(&format!("  +{}", paths.len() - 1));
    }
    text
}

fn clean(items: &[&CacheItem]) {
    let mut saved = 0u64;
    let mut files = 0u64;
    let mut errors = 0u64;

    for item in items {
        let mut stats = RemoveStats::default();
        let mut processed = 0u64;
        let mut frame = 0usize;
        let mut last_draw = Instant::now();
        if ui::is_tty() {
            let detail = format!(
                "{} · 0 B / {}",
                item.spec.label,
                util::human_size(item.size)
            );
            ui::live_line(&ui::progress_line("清理喵", 0.0, &detail, frame));
        }
        for path in &item.spec.paths {
            if !safe_to_delete(path) {
                stats.errors += 1;
                continue;
            }
            remove_path(path, &mut stats, &mut |bytes| {
                processed = processed.saturating_add(bytes);
                if last_draw.elapsed() >= Duration::from_millis(60) {
                    let ratio = if item.size == 0 {
                        0.0
                    } else {
                        processed as f64 / item.size as f64
                    };
                    let detail = format!(
                        "{} · {} / {}",
                        item.spec.label,
                        util::human_size(processed),
                        util::human_size(item.size)
                    );
                    ui::live_line(&ui::progress_line("清理喵", ratio, &detail, frame));
                    frame = frame.wrapping_add(1);
                    last_draw = Instant::now();
                }
            });
        }
        ui::end_live_line();
        saved = saved.saturating_add(stats.bytes);
        files = files.saturating_add(stats.files);
        errors = errors.saturating_add(stats.errors);

        let (mark, color) = if stats.errors == 0 {
            (ui::ICON_CHECK, ui::GREEN)
        } else {
            (ui::ICON_WARNING, ui::YELLOW)
        };
        let note = if stats.errors == 0 {
            "已清理喵".to_owned()
        } else {
            format!("{} 个文件未删除喵", stats.errors)
        };
        println!(
            "  {} {:<10} {:>9}  {}",
            ui::paint(mark, color),
            ui::paint_bold(item.spec.label, ui::TEXT),
            ui::paint(util::human_size(stats.bytes), ui::OVERLAY2),
            ui::paint(note, color)
        );
    }

    println!();
    ui::success(&format!(
        "清理完成喵。释放 {}，共 {} 个文件。",
        util::human_size(saved),
        files
    ));
    if errors > 0 {
        println!(
            "  {}",
            ui::paint(
                format!("有 {errors} 个文件未能删除喵，可能是权限问题。"),
                ui::YELLOW
            )
        );
    }
    println!();
}

fn safe_to_delete(path: &Path) -> bool {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return false;
    }
    let normal = path
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count();
    if normal < 2 {
        return false;
    }

    let home = util::home_dir();
    if let Some(home) = home.as_deref()
        && (path == home || home.starts_with(path))
    {
        return false;
    }

    if let Ok(cwd) = std::env::current_dir() {
        if cwd.starts_with(path) {
            return false;
        }
        let cwd_is_broad = home
            .as_deref()
            .is_some_and(|home| cwd == home || home.starts_with(&cwd));
        if path.starts_with(&cwd) && !cwd_is_broad {
            return false;
        }
    }

    if is_protected(path) {
        return false;
    }
    if has_project_marker(path) || has_git_ancestor(path, home.as_deref()) {
        return false;
    }
    true
}

fn is_protected(path: &Path) -> bool {
    let mut protected = Vec::new();
    if let Some(home) = util::home_dir() {
        protected.push(home.clone());
        for relative in [
            ".cache",
            ".config",
            ".local",
            ".local/share",
            ".local/share/pnpm",
            ".cargo",
            ".rustup",
            ".gradle",
            ".npm",
            ".bun",
            ".yarn",
            "go",
            "Downloads",
            "Documents",
            "Desktop",
        ] {
            protected.push(home.join(relative));
        }
    }
    for path in [util::cache_home(), util::data_home(), util::config_home()]
        .into_iter()
        .flatten()
    {
        protected.push(path);
    }
    if let Ok(cwd) = std::env::current_dir() {
        protected.push(cwd);
    }
    protected.iter().any(|protected| protected == path)
}

fn has_project_marker(path: &Path) -> bool {
    [
        ".git",
        ".hg",
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "composer.json",
    ]
    .iter()
    .any(|marker| path.join(marker).exists())
}

fn has_git_ancestor(path: &Path, home: Option<&Path>) -> bool {
    for ancestor in path.ancestors().skip(1) {
        if home.is_some_and(|home| ancestor == home) {
            break;
        }
        if ancestor.join(".git").exists() || ancestor.join(".hg").exists() {
            return true;
        }
    }
    false
}

fn remove_path(path: &Path, stats: &mut RemoveStats, on_bytes: &mut dyn FnMut(u64)) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        let bytes = meta.len();
        let removed = fs::remove_file(path).or_else(|_| fs::remove_dir(path));
        match removed {
            Ok(()) => {
                stats.bytes = stats.bytes.saturating_add(bytes);
                stats.files += 1;
                on_bytes(bytes);
            }
            Err(_) => stats.errors += 1,
        }
        return;
    }

    let Ok(entries) = fs::read_dir(path) else {
        stats.errors += 1;
        return;
    };
    for entry in entries {
        match entry {
            Ok(entry) => remove_path(&entry.path(), stats, on_bytes),
            Err(_) => stats.errors += 1,
        }
    }
    if fs::remove_dir(path).is_err() {
        stats.errors += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_counts_files_and_bytes() {
        let root = std::env::temp_dir().join(format!(
            "nya-cache-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("one"), b"1234").unwrap();
        fs::write(nested.join("two"), b"12").unwrap();

        let mut stats = RemoveStats::default();
        let mut seen = 0;
        remove_path(&root, &mut stats, &mut |bytes| seen += bytes);

        assert!(!root.exists());
        assert_eq!(stats.files, 2);
        assert_eq!(stats.bytes, 6);
        assert_eq!(seen, 6);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn refuses_broad_paths() {
        assert!(!safe_to_delete(Path::new("/")));
        assert!(!safe_to_delete(Path::new("/tmp")));
        assert!(safe_to_delete(Path::new("/tmp/nya-cache")));
    }

    #[test]
    fn refuses_paths_inside_projects() {
        let root = std::env::temp_dir().join(format!(
            "nya-project-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cache = root.join(".yarn/cache");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();

        assert!(!safe_to_delete(&cache));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn refuses_project_root_markers() {
        let root = std::env::temp_dir().join(format!(
            "nya-marker-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("package.json"), b"{}").unwrap();

        assert!(!safe_to_delete(&root));

        fs::remove_dir_all(&root).unwrap();
    }
}
