use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn home_dir() -> Option<PathBuf> {
    env_path("HOME").or_else(|| env_path("USERPROFILE"))
}

pub fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn cache_home() -> Option<PathBuf> {
    if let Some(path) = env_path("XDG_CACHE_HOME") {
        return Some(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        return Some(home.join("Library/Caches"));
    }
    #[cfg(windows)]
    if let Some(path) = env_path("LOCALAPPDATA") {
        return Some(path);
    }
    home_dir().map(|home| home.join(".cache"))
}

pub fn data_home() -> Option<PathBuf> {
    if let Some(path) = env_path("XDG_DATA_HOME") {
        return Some(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        return Some(home.join("Library/Application Support"));
    }
    #[cfg(windows)]
    if let Some(path) = env_path("LOCALAPPDATA") {
        return Some(path);
    }
    home_dir().map(|home| home.join(".local/share"))
}

pub fn config_home() -> Option<PathBuf> {
    if let Some(path) = env_path("XDG_CONFIG_HOME") {
        return Some(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        return Some(home.join("Library/Application Support"));
    }
    #[cfg(windows)]
    if let Some(path) = env_path("APPDATA") {
        return Some(path);
    }
    home_dir().map(|home| home.join(".config"))
}

pub fn expand_tilde(value: &str) -> PathBuf {
    let value = value.trim();
    if value == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().unwrap_or_else(|| PathBuf::from(".")).join(rest);
    }
    PathBuf::from(value)
}

pub fn pretty_path(path: &Path) -> String {
    if let Some(home) = home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_owned();
        }
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

pub fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn is_zero_width(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x200B..=0x200F
        | 0x20D0..=0x20FF
        | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F
        | 0xE0100..=0xE01EF
    )
}

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE10..=0xFE19
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD
    )
}

pub fn char_width(c: char) -> usize {
    if is_zero_width(c) {
        0
    } else if is_wide(c) {
        2
    } else {
        1
    }
}

pub fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

pub fn truncate_width(text: &str, max: usize) -> String {
    if display_width(text) <= max {
        return text.to_owned();
    }
    if max <= 1 {
        return "…".to_owned();
    }
    let mut out = String::new();
    let mut width = 0;
    for c in text.chars() {
        let next = width + char_width(c);
        if next + 1 > max {
            break;
        }
        out.push(c);
        width = next;
    }
    out.push('…');
    out
}

pub fn tail_lines(text: &str, count: usize) -> String {
    let mut lines: Vec<&str> = text.lines().rev().take(count).collect();
    lines.reverse();
    lines.join("\n")
}

pub fn command_exists(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| is_executable(&dir.join(name)))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    if fs::metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
    {
        return true;
    }
    let extensions = env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
    extensions.split(';').any(|ext| {
        let candidate = path.with_extension(ext.trim_start_matches('.'));
        fs::metadata(candidate)
            .map(|meta| meta.is_file())
            .unwrap_or(false)
    })
}

pub fn run_with_spinner(command: &mut Command, label: &str) -> Result<(), String> {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let program = command.get_program().to_string_lossy();
            return Err(format!("找不到 {program}，请先安装"));
        }
        Err(error) => return Err(format!("启动外部命令失败: {error}")),
    };

    let stderr = child.stderr.take();
    let reader = stderr.map(|stderr| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut text = String::new();
            let mut stderr = stderr;
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });

    let started = Instant::now();
    let mut frame = 0usize;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if crate::ui::is_tty() {
                    let line = format!(
                        "  {} {}  {:.0}s",
                        crate::ui::paint(crate::ui::spinner(frame), crate::ui::PINK),
                        crate::ui::paint(label, crate::ui::MAUVE),
                        started.elapsed().as_secs_f64()
                    );
                    crate::ui::live_line(&line);
                }
                frame = frame.wrapping_add(1);
                std::thread::sleep(Duration::from_millis(80));
            }
            Err(error) => return Err(format!("等待外部命令失败: {error}")),
        }
    };

    let stderr = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    crate::ui::end_live_line();

    if status.success() {
        Ok(())
    } else {
        let detail = tail_lines(stderr.trim(), 8);
        if detail.is_empty() {
            Err(format!("{label}失败（退出码 {:?}）", status.code()))
        } else {
            Err(format!("{label}失败\n{detail}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_readable() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(12 * 1024 * 1024), "12 MiB");
    }

    #[test]
    fn cjk_counts_as_two_columns() {
        assert_eq!(display_width("喵a"), 3);
        assert_eq!(truncate_width("喵喵喵", 5), "喵喵…");
    }
}
