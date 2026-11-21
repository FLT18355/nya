use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::config::Config;
use crate::ui;
use crate::util;

#[derive(Default)]
struct Options {
    target: Option<String>,
    audio: bool,
    cookies: Option<PathBuf>,
    no_cookies: bool,
    output_dir: Option<PathBuf>,
    format: Option<String>,
    playlist: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FormatKind {
    Combined,
    VideoOnly,
    AudioOnly,
}

struct FormatOption {
    id: String,
    ext: String,
    width: Option<u64>,
    height: Option<u64>,
    fps: Option<f64>,
    vcodec: String,
    acodec: String,
    size: Option<u64>,
    note: String,
    abr: Option<f64>,
    kind: FormatKind,
}

struct MediaInfo {
    title: String,
    uploader: String,
    duration: Option<f64>,
    formats: Vec<FormatOption>,
}

struct FormatSelection {
    selector: String,
    label: String,
}

enum DownloadEvent {
    Stdout(String),
    Stderr(String),
    StdoutDone,
    StderrDone,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let Some(options) = options else {
        return Ok(());
    };

    let config = Config::load()?;
    let mut target = match options.target {
        Some(target) => target,
        None => prompt_target()?,
    };
    target = normalize_target(&target)?;

    let download_dir = options
        .output_dir
        .unwrap_or_else(|| config.download_dir.clone());
    if download_dir.exists() && !download_dir.is_dir() {
        return Err(format!("下载目录不是目录喵: {}", download_dir.display()));
    }
    fs::create_dir_all(&download_dir).map_err(|error| format!("创建下载目录失败喵: {}", error))?;

    if options.no_cookies && options.cookies.is_some() {
        return Err("--cookies 与 --no-cookies 不能同时使用喵".to_owned());
    }

    let mut cookies_missing = false;
    let cookies = if options.no_cookies {
        None
    } else if let Some(path) = options.cookies {
        if !path.is_file() {
            return Err(format!("Cookies 文件不存在喵: {}", path.display()));
        }
        Some(path)
    } else if let Some(path) = config.cookies.clone() {
        if path.is_file() {
            Some(path)
        } else {
            cookies_missing = true;
            None
        }
    } else {
        None
    };

    let playlist = options.playlist.unwrap_or(config.playlist);

    ui::header(
        ui::ICON_DOWNLOAD,
        "Bilibili 下载喵",
        if options.audio {
            "音频喵"
        } else {
            "视频喵"
        },
    );
    field("目标喵", target.clone());
    field("目录喵", util::pretty_path(&download_dir));
    let cookie_text = match &cookies {
        Some(path) => format!("{} {}", ui::ICON_COOKIE, util::pretty_path(path)),
        None if cookies_missing => "未找到，不使用".to_owned(),
        None => "不使用".to_owned(),
    };
    field("Cookies 喵", cookie_text);
    if cookies_missing {
        ui::hint("Cookies 文件不存在喵，继续不使用 Cookies。");
    }

    let selection = if let Some(format) = options.format {
        FormatSelection {
            selector: format,
            label: "自定义".to_owned(),
        }
    } else if !playlist && io::stdin().is_terminal() {
        choose_format(&config, cookies.as_deref(), &target, options.audio)?
    } else {
        if playlist && io::stdin().is_terminal() {
            ui::hint("播放列表模式使用默认格式喵，可用 --format 指定。");
        } else if !io::stdin().is_terminal() {
            ui::hint("非交互环境使用默认格式喵，可用 --format 指定。");
        }
        FormatSelection {
            selector: if options.audio {
                "bestaudio/best".to_owned()
            } else {
                config.video_format.clone()
            },
            label: "默认".to_owned(),
        }
    };

    println!();
    let mode = if options.audio {
        format!(
            "音频 · {} · {} · {}",
            config.audio_format, selection.label, selection.selector
        )
    } else {
        format!("视频 · {} · {}", selection.label, selection.selector)
    };
    field("模式喵", mode);
    println!();

    let mut command = Command::new(&config.yt_dlp);
    command
        .arg("--newline")
        .arg("--progress")
        .arg("--progress-template")
        .arg("download:NYA|%(progress._percent_str)s|%(progress._total_bytes_str)s|%(progress._speed_str)s|%(progress._eta_str)s")
        .arg(if playlist {
            "--yes-playlist"
        } else {
            "--no-playlist"
        })
        .arg("--print")
        .arg("after_move:filepath")
        .arg("-P")
        .arg(&download_dir)
        .arg("-o")
        .arg(&config.output_template)
        .arg("-f")
        .arg(&selection.selector);
    if options.audio {
        command
            .arg("-x")
            .arg("--audio-format")
            .arg(&config.audio_format)
            .arg("--audio-quality")
            .arg(&config.audio_quality);
    } else if let Some(merge_format) = &config.merge_format {
        command.arg("--merge-output-format").arg(merge_format);
    }
    if let Some(cookies) = &cookies {
        command.arg("--cookies").arg(cookies);
    }
    command.arg(&target);

    let mut child = match command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "找不到 {}，请先安装 yt-dlp喵\n配置项 [tool].yt_dlp 可指定可执行文件路径",
                config.yt_dlp
            ));
        }
        Err(error) => return Err(format!("启动 yt-dlp 失败: {error}")),
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut stdout_done = stdout.is_none();
    let mut stderr_done = stderr.is_none();
    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = stdout {
        let sender = sender.clone();
        std::thread::spawn(move || {
            pump_lines(stdout, sender.clone(), false);
            let _ = sender.send(DownloadEvent::StdoutDone);
        });
    }
    if let Some(stderr) = stderr {
        let sender = sender.clone();
        std::thread::spawn(move || {
            pump_lines(stderr, sender.clone(), true);
            let _ = sender.send(DownloadEvent::StderrDone);
        });
    }
    drop(sender);

    let mut paths = Vec::new();
    let mut stderr_text = String::new();
    let mut progress: Option<(f64, String)> = None;
    let mut started = false;
    let mut frame = 0usize;
    let mut last_draw: Option<Instant> = None;
    while !stdout_done || !stderr_done {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(DownloadEvent::Stdout(line)) => {
                if line.contains("Destination:") || line.contains("Downloading") {
                    started = true;
                }
                if let Some((ratio, detail)) = parse_progress(&line) {
                    progress = Some((ratio, detail));
                } else if let Some(path) = parse_output_path(&line, &download_dir)
                    && !paths.contains(&path)
                {
                    paths.push(path);
                }
            }
            Ok(DownloadEvent::Stderr(line)) => {
                if line.contains("Destination:") || line.contains("Downloading") {
                    started = true;
                }
                if let Some((ratio, detail)) = parse_progress(&line) {
                    progress = Some((ratio, detail));
                }
                if !stderr_text.is_empty() {
                    stderr_text.push('\n');
                }
                stderr_text.push_str(&line);
            }
            Ok(DownloadEvent::StdoutDone) => stdout_done = true,
            Ok(DownloadEvent::StderrDone) => stderr_done = true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if last_draw.is_none_or(|last| last.elapsed() >= Duration::from_millis(80)) {
            if let Some((ratio, detail)) = &progress {
                let max_detail = ui::terminal_width().saturating_sub(28).clamp(12, 50);
                let detail = util::truncate_width(detail, max_detail);
                ui::live_line(&ui::progress_line("下载喵", *ratio, &detail, frame));
            } else {
                ui::live_line(&ui::indeterminate_line(
                    "下载喵",
                    if started {
                        "下载已开始喵，等待进度喵..."
                    } else {
                        "正在连接喵..."
                    },
                    frame,
                ));
            }
            frame = frame.wrapping_add(1);
            last_draw = Some(Instant::now());
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("等待 yt-dlp 失败喵: {error}"))?;
    ui::end_live_line();

    if !status.success() {
        let detail = util::tail_lines(stderr_text.trim(), 10);
        if detail.is_empty() {
            return Err(format!("下载失败喵（退出码 {:?}）", status.code()));
        }
        return Err(format!("下载失败喵\n{detail}"));
    }

    println!();
    ui::success("下载完成喵。");
    if paths.is_empty() {
        println!(
            "  {}  {}",
            ui::paint("目录喵", ui::OVERLAY1),
            ui::paint(util::pretty_path(&download_dir), ui::MAUVE)
        );
    } else {
        for path in paths {
            let size = fs::metadata(&path)
                .map(|meta| util::human_size(meta.len()))
                .unwrap_or_else(|_| "未知大小喵".to_owned());
            println!(
                "  {}  {}  {}",
                ui::paint(ui::ICON_FOLDER, ui::OVERLAY1),
                ui::paint(util::pretty_path(&path), ui::MAUVE),
                ui::paint(size, ui::OVERLAY2)
            );
        }
    }
    println!();
    Ok(())
}

fn parse_args(args: &[String]) -> Result<Option<Options>, String> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(None);
    }
    let mut options = Options::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "-a" | "-x" | "--audio" => options.audio = true,
            "-c" | "--cookies" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("-c/--cookies 后面需要文件路径喵".to_owned());
                };
                options.cookies = Some(util::expand_tilde(value));
            }
            "-n" | "--no-cookies" => options.no_cookies = true,
            "-o" | "--output" | "--dir" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("-o 后面需要下载目录喵".to_owned());
                };
                options.output_dir = Some(util::expand_tilde(value));
            }
            "-f" | "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("-f/--format 后面需要 yt-dlp 格式选择器喵".to_owned());
                };
                options.format = Some(value.clone());
            }
            "-p" | "--playlist" => options.playlist = Some(true),
            "--no-playlist" => options.playlist = Some(false),
            arg if arg.starts_with('-') => {
                return Err(format!("未知选项喵: {arg}"));
            }
            arg => {
                if options.target.is_some() {
                    return Err("一次只能下载一个视频喵".to_owned());
                }
                options.target = Some(arg.to_owned());
            }
        }
        index += 1;
    }
    Ok(Some(options))
}

fn prompt_target() -> Result<String, String> {
    if !io::stdin().is_terminal() {
        return Err("未提供视频 ID 或链接喵".to_owned());
    }
    print!(
        "  {}  ",
        ui::paint("视频 ID / 链接喵（输入后回车）", ui::OVERLAY1)
    );
    io::stdout()
        .flush()
        .map_err(|error| format!("输出提示失败: {error}"))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("读取输入失败: {error}"))?;
    let input = input.trim().to_owned();
    if input.is_empty() {
        return Err("未提供视频 ID 或链接喵".to_owned());
    }
    Ok(input)
}

fn normalize_target(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("未提供视频 ID 或链接喵".to_owned());
    }
    let lower = input.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if is_bilibili_url(&lower) {
            return Ok(input.to_owned());
        }
        return Err("仅支持 Bilibili 链接喵".to_owned());
    }
    if lower.starts_with("www.bilibili.com/")
        || lower.starts_with("m.bilibili.com/")
        || lower.starts_with("bilibili.com/")
        || lower.starts_with("b23.tv/")
    {
        return Ok(format!("https://{input}"));
    }
    if let Some(rest) = input
        .strip_prefix("BV")
        .or_else(|| input.strip_prefix("bv"))
    {
        if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("BV 号格式无效喵".to_owned());
        }
        return Ok(format!("https://www.bilibili.com/video/BV{rest}"));
    }
    if let Some(rest) = input
        .strip_prefix("av")
        .or_else(|| input.strip_prefix("AV"))
    {
        if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
            return Err("av 号格式无效喵".to_owned());
        }
        return Ok(format!("https://www.bilibili.com/video/av{rest}"));
    }
    if input.chars().all(|c| c.is_ascii_digit()) {
        return Ok(format!("https://www.bilibili.com/video/av{input}"));
    }
    Err("仅支持 Bilibili BV 号、av 号、纯数字 av 号或 Bilibili 链接喵".to_owned())
}

fn is_bilibili_url(lower: &str) -> bool {
    let Some((_, rest)) = lower.split_once("://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    matches!(
        host,
        "bilibili.com" | "www.bilibili.com" | "m.bilibili.com" | "b23.tv"
    )
}

fn pump_lines<R: Read>(mut reader: R, sender: mpsc::Sender<DownloadEvent>, stderr: bool) {
    let mut buffer = [0u8; 8192];
    let mut line = Vec::new();
    while let Ok(read) = reader.read(&mut buffer) {
        if read == 0 {
            break;
        }
        for &byte in &buffer[..read] {
            if byte == b'\n' || byte == b'\r' {
                if line.is_empty() {
                    continue;
                }
                let text = String::from_utf8_lossy(&line).trim().to_owned();
                line.clear();
                if text.is_empty() {
                    continue;
                }
                let event = if stderr {
                    DownloadEvent::Stderr(text)
                } else {
                    DownloadEvent::Stdout(text)
                };
                if sender.send(event).is_err() {
                    return;
                }
            } else {
                line.push(byte);
            }
        }
    }
    if !line.is_empty() {
        let text = String::from_utf8_lossy(&line).trim().to_owned();
        if !text.is_empty() {
            let event = if stderr {
                DownloadEvent::Stderr(text)
            } else {
                DownloadEvent::Stdout(text)
            };
            let _ = sender.send(event);
        }
    }
}

fn choose_format(
    config: &Config,
    cookies: Option<&Path>,
    target: &str,
    audio: bool,
) -> Result<FormatSelection, String> {
    let info = fetch_media_info(config, cookies, target)?;
    println!();
    field("标题喵", info.title.clone());
    field("UP 主喵", info.uploader.clone());
    if let Some(duration) = info.duration {
        field("时长喵", util::format_duration(duration));
    }

    let choices: Vec<&FormatOption> = info
        .formats
        .iter()
        .filter(|format| {
            if audio {
                format.kind == FormatKind::AudioOnly
            } else {
                format.kind != FormatKind::AudioOnly
            }
        })
        .collect();
    if choices.is_empty() {
        ui::hint(if audio {
            "没有可用音频格式喵，使用默认最佳音频。"
        } else {
            "没有可用视频格式喵，使用默认最佳视频。"
        });
        return Ok(FormatSelection {
            selector: if audio {
                "bestaudio/best".to_owned()
            } else {
                config.video_format.clone()
            },
            label: "默认最佳".to_owned(),
        });
    }

    println!();
    println!(
        "  {}  {}",
        ui::paint(
            if audio {
                ui::ICON_MUSIC
            } else {
                ui::ICON_DOWNLOAD
            },
            ui::MAUVE
        ),
        ui::paint_bold(
            if audio {
                "可选音频格式喵"
            } else {
                "可选清晰度喵"
            },
            ui::TEXT
        )
    );
    println!();
    let width = ui::terminal_width();
    for (index, format) in choices.iter().enumerate() {
        println!("{}", format_line(index + 1, format, audio, width));
    }
    println!();

    loop {
        print!(
            "  {}  ",
            ui::paint(
                if audio {
                    "选择音频格式 [编号，回车=1，q 取消喵]"
                } else {
                    "选择清晰度 [编号，回车=1，q 取消喵]"
                },
                ui::LAVENDER
            )
        );
        io::stdout()
            .flush()
            .map_err(|error| format!("输出提示失败: {error}"))?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|error| format!("读取输入失败: {error}"))?;
        let input = input.trim();
        if input.eq_ignore_ascii_case("q") {
            return Err("已取消喵。".to_owned());
        }
        let index = if input.is_empty() {
            0
        } else {
            match input.parse::<usize>() {
                Ok(number) if (1..=choices.len()).contains(&number) => number - 1,
                _ => {
                    ui::hint(&format!("编号不对喵，请输入 1-{}。", choices.len()));
                    continue;
                }
            }
        };
        let format = choices[index];
        return Ok(FormatSelection {
            selector: format_selector(format),
            label: format_label(format),
        });
    }
}

fn fetch_media_info(
    config: &Config,
    cookies: Option<&Path>,
    target: &str,
) -> Result<MediaInfo, String> {
    let mut command = Command::new(&config.yt_dlp);
    command
        .arg("--dump-single-json")
        .arg("--no-playlist")
        .arg("--no-warnings");
    if let Some(cookies) = cookies {
        command.arg("--cookies").arg(cookies);
    }
    command.arg(target);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!(
                "找不到 {}，请先安装 yt-dlp喵\n配置项 [tool].yt_dlp 可指定可执行文件路径",
                config.yt_dlp
            )
        } else {
            format!("启动 yt-dlp 失败喵: {error}")
        }
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut stdout) = stdout {
            let _ = stdout.read_to_end(&mut bytes);
        }
        bytes
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut text);
        }
        text
    });
    let mut frame = 0usize;
    let mut last_draw: Option<Instant> = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if last_draw.is_none_or(|last| last.elapsed() >= Duration::from_millis(80)) {
                    ui::live_line(&ui::indeterminate_line(
                        "获取喵",
                        "正在获取视频信息喵...",
                        frame,
                    ));
                    frame = frame.wrapping_add(1);
                    last_draw = Some(Instant::now());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(format!("等待 yt-dlp 失败喵: {error}")),
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    ui::end_live_line();
    if !status.success() {
        let detail = util::tail_lines(stderr.trim(), 8);
        if detail.is_empty() {
            return Err(format!("获取视频信息失败喵（退出码 {:?}）", status.code()));
        }
        return Err(format!("获取视频信息失败喵\n{detail}"));
    }
    let value: Value =
        serde_json::from_slice(&stdout).map_err(|error| format!("解析视频信息失败喵: {error}"))?;
    Ok(parse_media_info(&value))
}

fn parse_media_info(value: &Value) -> MediaInfo {
    let value = if value.get("formats").and_then(Value::as_array).is_some() {
        value
    } else {
        value
            .get("entries")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .unwrap_or(value)
    };
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("未知标题")
        .to_owned();
    let uploader = value
        .get("uploader")
        .and_then(Value::as_str)
        .or_else(|| value.get("channel").and_then(Value::as_str))
        .unwrap_or("未知 UP 主")
        .to_owned();
    let duration = value
        .get("duration")
        .and_then(Value::as_f64)
        .filter(|duration| *duration > 0.0);
    let mut seen = HashSet::new();
    let mut formats = Vec::new();
    if let Some(items) = value.get("formats").and_then(Value::as_array) {
        for item in items {
            let Some(id) = item.get("format_id").and_then(Value::as_str) else {
                continue;
            };
            if id.is_empty() || !seen.insert(id.to_owned()) {
                continue;
            }
            let vcodec = item.get("vcodec").and_then(Value::as_str).unwrap_or("none");
            let acodec = item.get("acodec").and_then(Value::as_str).unwrap_or("none");
            let kind = match (vcodec != "none", acodec != "none") {
                (true, true) => FormatKind::Combined,
                (true, false) => FormatKind::VideoOnly,
                (false, true) => FormatKind::AudioOnly,
                (false, false) => continue,
            };
            formats.push(FormatOption {
                id: id.to_owned(),
                ext: item
                    .get("ext")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_owned(),
                width: positive_u64(item.get("width")),
                height: positive_u64(item.get("height")),
                fps: positive_f64(item.get("fps")),
                vcodec: vcodec.to_owned(),
                acodec: acodec.to_owned(),
                size: positive_u64(item.get("filesize"))
                    .or_else(|| positive_u64(item.get("filesize_approx"))),
                note: item
                    .get("format_note")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                abr: positive_f64(item.get("abr")).or_else(|| positive_f64(item.get("tbr"))),
                kind,
            });
        }
    }
    formats.sort_by(compare_formats);
    MediaInfo {
        title,
        uploader,
        duration,
        formats,
    }
}

fn compare_formats(a: &FormatOption, b: &FormatOption) -> Ordering {
    b.height
        .unwrap_or(0)
        .cmp(&a.height.unwrap_or(0))
        .then_with(|| kind_rank(a.kind).cmp(&kind_rank(b.kind)))
        .then_with(|| {
            b.fps
                .unwrap_or(0.0)
                .partial_cmp(&a.fps.unwrap_or(0.0))
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| {
            b.abr
                .unwrap_or(0.0)
                .partial_cmp(&a.abr.unwrap_or(0.0))
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| b.size.unwrap_or(0).cmp(&a.size.unwrap_or(0)))
}

fn kind_rank(kind: FormatKind) -> u8 {
    match kind {
        FormatKind::Combined => 0,
        FormatKind::VideoOnly => 1,
        FormatKind::AudioOnly => 2,
    }
}

fn kind_label(kind: FormatKind) -> &'static str {
    match kind {
        FormatKind::Combined => "视频+音频",
        FormatKind::VideoOnly => "仅视频 · 自动配最佳音频",
        FormatKind::AudioOnly => "仅音频",
    }
}

fn format_selector(format: &FormatOption) -> String {
    match format.kind {
        FormatKind::VideoOnly => format!("{}+bestaudio/best", format.id),
        FormatKind::Combined | FormatKind::AudioOnly => format.id.clone(),
    }
}

fn format_label(format: &FormatOption) -> String {
    if !format.note.is_empty() {
        format.note.clone()
    } else if let (Some(width), Some(height)) = (format.width, format.height) {
        format!("{width}x{height}")
    } else {
        format.ext.clone()
    }
}

fn format_line(index: usize, format: &FormatOption, audio: bool, width: usize) -> String {
    let mut details = Vec::new();
    if let (Some(w), Some(h)) = (format.width, format.height) {
        details.push(format!("{w}x{h}"));
    }
    if let Some(fps) = format.fps {
        details.push(format!("{fps:.0}fps"));
    }
    let codec = if audio {
        &format.acodec
    } else {
        &format.vcodec
    };
    if !codec.is_empty() && codec != "none" {
        details.push(short_codec(codec).to_owned());
    }
    details.push(kind_label(format.kind).to_owned());
    if audio && let Some(abr) = format.abr {
        details.push(format!("{abr:.0} kbps"));
    }
    if let Some(size) = format.size {
        details.push(util::human_size(size));
    }
    details.push(format!("格式 {}", format.id));

    let label = format_label(format);
    let label_padding = 12usize.saturating_sub(util::display_width(&label));
    let label = format!("{label}{}", " ".repeat(label_padding));
    let max_details = width
        .saturating_sub(util::display_width(&label) + 10)
        .max(16);
    let details = util::truncate_width(&details.join(" · "), max_details);
    format!(
        "  {}  {}  {}",
        ui::paint(format!("{index:>2}."), ui::MAUVE),
        ui::paint_bold(label, ui::LAVENDER),
        ui::paint(details, ui::OVERLAY1)
    )
}

fn short_codec(codec: &str) -> &str {
    codec.split('.').next().unwrap_or(codec)
}

fn positive_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64).filter(|value| *value > 0)
}

fn positive_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|value| *value > 0.0)
}

fn parse_progress(line: &str) -> Option<(f64, String)> {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix("NYA|") {
        let mut parts = rest.split('|');
        let percent = parts
            .next()?
            .trim()
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()?;
        let detail = parts
            .map(str::trim)
            .filter(|part| !part.is_empty() && *part != "NA")
            .collect::<Vec<_>>()
            .join(" · ");
        return Some((
            (percent / 100.0).clamp(0.0, 1.0),
            format!("{percent:.0}% · {detail}"),
        ));
    }
    let rest = line.strip_prefix("[download]")?.trim();
    let percent_end = rest.find('%')?;
    let percent = rest[..percent_end].trim().parse::<f64>().ok()?;
    let detail = rest[percent_end + 1..].trim().to_owned();
    Some((
        (percent / 100.0).clamp(0.0, 1.0),
        format!("{percent:.0}% · {detail}"),
    ))
}

fn parse_output_path(line: &str, download_dir: &Path) -> Option<PathBuf> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('[') {
        return None;
    }
    let candidate = PathBuf::from(line);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        download_dir.join(candidate)
    };
    path.is_file().then_some(path)
}

fn field(key: &str, value: String) {
    let padding = 12usize.saturating_sub(util::display_width(key));
    println!(
        "  {}{}  {}",
        ui::paint(key, ui::OVERLAY1),
        " ".repeat(padding),
        ui::paint(value, ui::TEXT)
    );
}

fn print_help() {
    println!();
    println!(
        "  {}  {}",
        ui::paint(ui::ICON_DOWNLOAD, ui::MAUVE),
        ui::paint_bold("Bilibili 下载喵", ui::TEXT)
    );
    println!();
    println!(
        "  {}",
        ui::paint(
            "用法: nya bili [视频ID|链接] [-a|-x] [-c Cookies] [-n] [-o 目录] [-f 选择器] [-p]",
            ui::TEXT
        )
    );
    println!(
        "  {}",
        ui::paint(
            "        [--audio] [--cookies 路径] [--no-cookies] [--format 选择器] [--playlist|--no-playlist]",
            ui::TEXT
        )
    );
    println!();
    println!("  {}", ui::paint_bold("视频 ID / 链接喵", ui::LAVENDER));
    println!(
        "    {}",
        ui::paint("BV 号、av 号、纯数字 av 号或 Bilibili 链接", ui::TEXT)
    );
    println!();
    println!("  {}", ui::paint_bold("选项喵", ui::LAVENDER));
    println!(
        "    {}",
        ui::paint("-a, -x, --audio   仅下载音频并转码", ui::TEXT)
    );
    println!(
        "    {}",
        ui::paint(
            "-c, --cookies     指定 Cookies 文件，默认读取配置",
            ui::TEXT
        )
    );
    println!(
        "    {}",
        ui::paint("-n, --no-cookies  本次不使用 Cookies", ui::TEXT)
    );
    println!(
        "    {}",
        ui::paint("-o, --output      覆盖下载目录", ui::TEXT)
    );
    println!(
        "    {}",
        ui::paint(
            "-f, --format      跳过清晰度选择，直接使用 yt-dlp 选择器",
            ui::TEXT
        )
    );
    println!(
        "    {}",
        ui::paint("-p, --playlist    下载播放列表", ui::TEXT)
    );
    println!(
        "    {}",
        ui::paint("    --no-playlist 只下载单个视频", ui::TEXT)
    );
    println!();
    println!(
        "  {}",
        ui::paint(
            "不提供 ID 时会提示输入喵。默认列出可用清晰度让你选。仅支持 Bilibili，配置见 nya config。",
            ui::OVERLAY1
        )
    );
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_bilibili_ids() {
        assert_eq!(
            normalize_target("BV1xx411c7mD").unwrap(),
            "https://www.bilibili.com/video/BV1xx411c7mD"
        );
        assert_eq!(
            normalize_target("av12345").unwrap(),
            "https://www.bilibili.com/video/av12345"
        );
        assert_eq!(
            normalize_target("12345").unwrap(),
            "https://www.bilibili.com/video/av12345"
        );
        assert_eq!(
            normalize_target("b23.tv/abc").unwrap(),
            "https://b23.tv/abc"
        );
        assert_eq!(
            normalize_target("https://www.bilibili.com/video/BV1xx411c7mD").unwrap(),
            "https://www.bilibili.com/video/BV1xx411c7mD"
        );
        assert!(normalize_target("https://youtube.com/watch?v=1").is_err());
        assert!(normalize_target("https://www.bilibili.com.evil.test/x").is_err());
    }

    #[test]
    fn parses_and_sorts_formats() {
        let value: Value = serde_json::from_str(
            r#"{
                "title": "测试视频",
                "uploader": "UP 主",
                "duration": 61.5,
                "formats": [
                    {
                        "format_id": "18",
                        "ext": "mp4",
                        "width": 640,
                        "height": 360,
                        "fps": 30,
                        "vcodec": "avc1",
                        "acodec": "mp4a",
                        "filesize": 500,
                        "format_note": "360P"
                    },
                    {
                        "format_id": "80",
                        "ext": "mp4",
                        "width": 1920,
                        "height": 1080,
                        "fps": 30,
                        "vcodec": "avc1.640028",
                        "acodec": "none",
                        "filesize": 1000,
                        "format_note": "1080P"
                    },
                    {
                        "format_id": "30280",
                        "ext": "m4a",
                        "vcodec": "none",
                        "acodec": "mp4a.40.2",
                        "abr": 128,
                        "filesize": 200,
                        "format_note": "128k"
                    },
                    {
                        "format_id": "80",
                        "ext": "mp4",
                        "width": 1920,
                        "height": 1080,
                        "vcodec": "avc1",
                        "acodec": "none"
                    }
                ]
            }"#,
        )
        .unwrap();
        let info = parse_media_info(&value);
        assert_eq!(info.title, "测试视频");
        assert_eq!(info.uploader, "UP 主");
        assert_eq!(info.formats.len(), 3);
        assert_eq!(info.formats[0].id, "80");
        assert_eq!(info.formats[0].kind, FormatKind::VideoOnly);
        assert_eq!(format_selector(&info.formats[0]), "80+bestaudio/best");
        assert_eq!(format_label(&info.formats[0]), "1080P");
        assert!(format_line(1, &info.formats[0], false, 100).contains("仅视频"));
        assert_eq!(format_selector(&info.formats[1]), "18");
        assert_eq!(format_selector(&info.formats[2]), "30280");
    }

    #[test]
    fn parses_progress() {
        let (ratio, detail) =
            parse_progress("[download]  42.5% of 100.00MiB at 1.00MiB/s").unwrap();
        assert!((ratio - 0.425).abs() < 0.001);
        assert!(detail.contains("100.00MiB"));

        let (ratio, detail) = parse_progress("NYA| 50.0%| 1.00MiB| 1.00MiB/s|00:01").unwrap();
        assert!((ratio - 0.5).abs() < 0.001);
        assert!(detail.contains("1.00MiB"));
    }
}
