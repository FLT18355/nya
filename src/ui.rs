use std::env;
use std::io::{self, IsTerminal, Write};
use std::sync::LazyLock;

use crossterm::terminal;

use crate::util::display_width;

pub type Rgb = (u8, u8, u8);

pub const MAUVE: Rgb = (203, 166, 247);
pub const LAVENDER: Rgb = (180, 190, 254);
pub const PINK: Rgb = (245, 194, 231);
pub const PEACH: Rgb = (250, 179, 135);
pub const TEXT: Rgb = (205, 214, 244);
pub const OVERLAY2: Rgb = (147, 153, 178);
pub const OVERLAY1: Rgb = (127, 132, 156);
pub const OVERLAY0: Rgb = (108, 112, 134);
pub const SURFACE1: Rgb = (69, 71, 90);
pub const GREEN: Rgb = (166, 227, 161);
pub const YELLOW: Rgb = (249, 226, 175);
pub const RED: Rgb = (243, 139, 168);

pub const ICON_TERMINAL: &str = "\u{f120}";
pub const ICON_TRASH: &str = "\u{f1f8}";
pub const ICON_ARCHIVE: &str = "\u{f187}";
pub const ICON_MUSIC: &str = "\u{f001}";
pub const ICON_DOWNLOAD: &str = "\u{f019}";
pub const ICON_SETTINGS: &str = "\u{f013}";
pub const ICON_INFO: &str = "\u{f05a}";
pub const ICON_WARNING: &str = "\u{f071}";
pub const ICON_CHECK: &str = "\u{f00c}";
pub const ICON_CHECK_SQUARE: &str = "\u{f046}";
pub const ICON_SQUARE: &str = "\u{f096}";
pub const ICON_CARET_RIGHT: &str = "\u{f0da}";
pub const ICON_COOKIE: &str = "\u{f563}";
pub const ICON_FOLDER: &str = "\u{f07b}";

const RESET: &str = "\x1b[0m";
const SPINNERS: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn colors_enabled() -> bool {
    static ENABLED: LazyLock<bool> = LazyLock::new(|| {
        if env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
            return false;
        }
        if env::var("TERM").is_ok_and(|term| term == "dumb") {
            return false;
        }
        io::stdout().is_terminal()
    });
    *ENABLED
}

pub fn is_tty() -> bool {
    io::stdout().is_terminal()
}

pub fn terminal_width() -> usize {
    terminal::size()
        .map(|(width, _)| width as usize)
        .unwrap_or(80)
}

fn fg(color: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2)
}

pub fn paint(text: impl AsRef<str>, color: Rgb) -> String {
    if colors_enabled() {
        format!("{}{}{}", fg(color), text.as_ref(), RESET)
    } else {
        text.as_ref().to_owned()
    }
}

pub fn paint_bold(text: impl AsRef<str>, color: Rgb) -> String {
    if colors_enabled() {
        format!(
            "\x1b[1;38;2;{};{};{}m{}{}",
            color.0,
            color.1,
            color.2,
            text.as_ref(),
            RESET
        )
    } else {
        text.as_ref().to_owned()
    }
}

fn sample(stops: &[Rgb], t: f64) -> Rgb {
    if stops.len() == 1 {
        return stops[0];
    }
    let position = t.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
    let index = (position.floor() as usize).min(stops.len() - 2);
    let local = position - index as f64;
    let a = stops[index];
    let b = stops[index + 1];
    let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * local).round() as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

pub fn gradient(text: &str, stops: &[Rgb]) -> String {
    if !colors_enabled() || text.is_empty() || stops.is_empty() {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let denominator = chars.len().saturating_sub(1).max(1) as f64;
    let mut out = String::new();
    for (index, c) in chars.iter().enumerate() {
        let color = sample(stops, index as f64 / denominator);
        out.push_str(&fg(color));
        out.push(*c);
    }
    out.push_str(RESET);
    out
}

pub fn spinner(frame: usize) -> &'static str {
    SPINNERS[frame % SPINNERS.len()]
}

pub fn divider(width: usize) -> String {
    paint("─".repeat(width), SURFACE1)
}

pub fn header(icon: &str, title: &str, subtitle: &str) {
    println!();
    println!("  {}  {}", paint(icon, MAUVE), paint_bold(title, TEXT));
    println!("  {}", paint(subtitle, OVERLAY2));
    println!();
}

pub fn home() {
    println!();
    println!(
        "  {}  {}",
        paint(ICON_TERMINAL, MAUVE),
        paint_bold("nya", PINK)
    );
    println!(
        "  {}",
        gradient(
            "喵呜，怎么会有这么可爱的终端！",
            &[LAVENDER, MAUVE, PINK, PEACH]
        )
    );
    println!();
    command(ICON_TRASH, "cache", "清理包管理器缓存");
    command(ICON_DOWNLOAD, "bili", "下载 Bilibili 音视频");
    command(ICON_ARCHIVE, "x", "解压 tar / zip / 7z / rar");
    command(ICON_MUSIC, "audio", "转换音频格式");
    command(ICON_SETTINGS, "config", "查看或初始化配置");
    command(ICON_INFO, "help", "完整帮助");
    println!();
    println!("  {}", paint("nya help 查看完整帮助", OVERLAY1));
    println!();
}

fn command(icon: &str, name: &str, description: &str) {
    println!(
        "  {}  {}  {}",
        paint(icon, MAUVE),
        paint_bold(format!("{name:<8}"), MAUVE),
        paint(description, TEXT)
    );
}

pub fn help() {
    println!();
    println!(
        "  {}  {}",
        paint(ICON_TERMINAL, MAUVE),
        paint_bold("nya", TEXT)
    );
    println!("  {}", paint("终端工具", OVERLAY2));
    println!();
    println!("  {}", paint_bold("用法", LAVENDER));
    println!("    {}", paint("nya <命令> [选项]", TEXT));
    println!();
    println!("  {}", paint_bold("命令", LAVENDER));
    command(ICON_TRASH, "cache", "选择并清理包管理器缓存");
    command(ICON_DOWNLOAD, "bili", "下载 Bilibili 音视频");
    command(ICON_ARCHIVE, "x", "解压 tar / zip / 7z / rar 压缩包");
    command(ICON_MUSIC, "audio", "用 ffmpeg 转换音频格式");
    command(ICON_SETTINGS, "config", "查看或初始化配置");
    command(ICON_INFO, "help", "显示这份帮助");
    println!();
    println!("  {}", paint_bold("cache", LAVENDER));
    println!(
        "    {}",
        paint("nya cache [名称...]   默认不选，交互选择喵", TEXT)
    );
    println!();
    println!("  {}", paint_bold("bili", LAVENDER));
    println!(
        "    {}",
        paint(
            "nya bili [视频ID|链接] [--audio] [--cookies 路径] [--no-cookies]",
            TEXT
        )
    );
    println!(
        "    {}",
        paint(
            "-o 下载目录  --format 格式选择器  --playlist / --no-playlist",
            TEXT
        )
    );
    println!("    {}", paint("默认列出可用清晰度让你选喵", OVERLAY1));
    println!();
    println!("  {}", paint_bold("x", LAVENDER));
    println!("    {}", paint("nya x <压缩包> [-o 输出目录]", TEXT));
    println!();
    println!("  {}", paint_bold("audio", LAVENDER));
    println!(
        "    {}",
        paint(
            "nya audio <音频> [输出] [-f 格式] [-b 码率] [--force]",
            TEXT
        )
    );
    println!(
        "    {}",
        paint("格式: mp3 flac wav ogg opus m4a aac aiff", OVERLAY1)
    );
    println!();
    println!("  {}", paint_bold("config", LAVENDER));
    println!("    {}", paint("nya config [init]", TEXT));
    println!(
        "    {}",
        paint(
            "配置区块: [download] [video] [audio] [output] [tool]",
            OVERLAY1
        )
    );
    println!();
    println!(
        "  {}",
        paint(
            "缓存清理只删除已知缓存目录喵，不删除项目文件或虚拟环境。",
            OVERLAY1
        )
    );
    println!();
}

pub fn unknown(command: &str) {
    eprintln!(
        "  {} {}",
        paint(ICON_WARNING, RED),
        paint_bold(format!("未知命令: {command}"), TEXT)
    );
    eprintln!("  {}", paint("运行 nya help 查看用法喵", OVERLAY1));
}

pub fn fail(message: &str) {
    let mut lines = message.lines();
    if let Some(first) = lines.next() {
        eprintln!("  {} {}", paint(ICON_WARNING, RED), paint_bold(first, TEXT));
    }
    for line in lines {
        eprintln!("    {}", paint(line, OVERLAY1));
    }
}

pub fn hint(message: &str) {
    eprintln!(
        "  {} {}",
        paint(ICON_INFO, OVERLAY0),
        paint(message, OVERLAY1)
    );
}

pub fn success(message: &str) {
    println!(
        "  {} {}",
        paint(ICON_CHECK, GREEN),
        paint_bold(message, TEXT)
    );
}

pub fn live_line(line: &str) {
    if !is_tty() {
        return;
    }
    let mut stdout = io::stdout().lock();
    let _ = write!(stdout, "\r\x1b[2K{line}");
    let _ = stdout.flush();
}

pub fn end_live_line() {
    if !is_tty() {
        return;
    }
    let mut stdout = io::stdout().lock();
    let _ = write!(stdout, "\r\x1b[2K");
    let _ = stdout.flush();
}

fn bar(ratio: f64, width: usize) -> String {
    let ratio = ratio.clamp(0.0, 1.0);
    let filled = ((ratio * width as f64).round() as usize).min(width);
    let mut out = String::new();
    if filled > 0 {
        out.push_str(&paint("━".repeat(filled), MAUVE));
    }
    if filled < width {
        out.push_str(&paint("╸", PINK));
        let rest = width - filled - 1;
        if rest > 0 {
            out.push_str(&paint("─".repeat(rest), SURFACE1));
        }
    }
    out
}

pub fn progress_line(label: &str, ratio: f64, detail: &str, frame: usize) -> String {
    let label_width = display_width(label);
    let detail_width = display_width(detail);
    let fixed = 2 + 1 + 1 + label_width + 2 + 2 + 4 + 2 + detail_width;
    let bar_width = terminal_width().saturating_sub(fixed).clamp(6, 36);
    format!(
        "  {} {}  {}  {}  {}",
        paint(spinner(frame), PINK),
        paint(label, MAUVE),
        bar(ratio, bar_width),
        paint(format!("{:>3.0}%", ratio.clamp(0.0, 1.0) * 100.0), TEXT),
        paint(detail, OVERLAY1)
    )
}

pub fn indeterminate_line(label: &str, detail: &str, frame: usize) -> String {
    let label_width = display_width(label);
    let detail_width = display_width(detail);
    let fixed = 2 + 1 + 1 + label_width + 2 + 2 + detail_width;
    let width = terminal_width().saturating_sub(fixed).clamp(6, 24);
    let travel = width + 8;
    let head = (frame * 2) % travel;
    let start = head.saturating_sub(5);
    let mut line = String::new();
    for index in 0..width {
        if index >= start && index < start + 5 {
            line.push_str(&paint("━", MAUVE));
        } else {
            line.push_str(&paint("─", SURFACE1));
        }
    }
    format!(
        "  {} {}  {}  {}",
        paint(spinner(frame), PINK),
        paint(label, MAUVE),
        line,
        paint(detail, OVERLAY1)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_keeps_plain_text() {
        let text = "喵呜";
        assert!(gradient(text, &[LAVENDER, PINK]).contains(text));
    }
}
