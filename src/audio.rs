use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::ui;
use crate::util;

const FORMATS: &[&str] = &["mp3", "flac", "wav", "ogg", "opus", "m4a", "aac", "aiff"];

pub fn run(args: &[String]) -> Result<(), String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut requested_format: Option<String> = None;
    let mut bitrate = "192k".to_owned();
    let mut force = false;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-f" | "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("-f 后面需要格式".to_owned());
                };
                requested_format = Some(normalize_format(value)?);
            }
            "-b" | "--bitrate" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("-b 后面需要码率".to_owned());
                };
                bitrate = normalize_bitrate(value)?;
            }
            "--force" => force = true,
            arg if arg.starts_with('-') => {
                return Err(format!("未知选项: {arg}"));
            }
            arg => {
                if input.is_none() {
                    input = Some(PathBuf::from(arg));
                } else if output.is_none() {
                    output = Some(PathBuf::from(arg));
                } else {
                    return Err("参数过多".to_owned());
                }
            }
        }
        index += 1;
    }

    let Some(input) = input else {
        print_help();
        return Ok(());
    };
    if !input.is_file() {
        return Err(format!("找不到音频: {}", input.display()));
    }

    let output_extension = output
        .as_ref()
        .and_then(|path| path.extension())
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    let format = match (&requested_format, &output_extension) {
        (Some(requested), Some(extension)) => {
            let normalized = normalize_format(extension)?;
            if &normalized != requested {
                return Err(format!(
                    "输出后缀 .{extension} 与格式 {requested} 不一致\n更换输出名，或去掉 -f"
                ));
            }
            requested.clone()
        }
        (Some(requested), None) => requested.clone(),
        (None, Some(extension)) => normalize_format(extension)?,
        (None, None) => "mp3".to_owned(),
    };

    let mut output = output.unwrap_or_else(|| input.with_extension(&format));
    if output.extension().is_none() {
        output.set_extension(&format);
    }
    if same_file(&input, &output) {
        return Err("输入和输出是同一个文件，请更换输出名".to_owned());
    }
    if output.is_dir() {
        return Err(format!("输出位置是目录: {}", output.display()));
    }
    if output.exists() && !force {
        return Err(format!(
            "输出文件已存在: {}\n覆盖请加 --force",
            output.display()
        ));
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        return Err(format!("输出目录不存在: {}", parent.display()));
    }

    let duration = probe_duration(&input);
    let output_existed = output.exists();

    ui::header(ui::ICON_MUSIC, "音频转换喵", "ffmpeg 喵");
    println!(
        "  {}  {}",
        ui::paint("输入喵", ui::OVERLAY1),
        ui::paint(util::pretty_path(&input), ui::TEXT)
    );
    println!(
        "  {}  {}",
        ui::paint("输出喵", ui::OVERLAY1),
        ui::paint(util::pretty_path(&output), ui::MAUVE)
    );
    println!(
        "  {}  {}",
        ui::paint("格式喵", ui::OVERLAY1),
        ui::paint(codec_label(&format, &bitrate), ui::OVERLAY2)
    );
    println!();

    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-stats_period")
        .arg("0.1")
        .arg("-i")
        .arg(&input)
        .arg("-map")
        .arg("0:a:0")
        .arg("-vn")
        .arg("-map_metadata")
        .arg("0")
        .arg("-progress")
        .arg("pipe:1")
        .arg("-nostats");
    for arg in codec_args(&format, &bitrate) {
        command.arg(arg);
    }
    command
        .arg(if force { "-y" } else { "-n" })
        .arg(&output)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err("找不到 ffmpeg，请先安装".to_owned());
        }
        Err(error) => return Err(format!("启动 ffmpeg 失败: {error}")),
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stderr_reader = stderr.map(|stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let mut stderr = stderr;
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });

    let mut current = 0.0f64;
    let mut frame = 0usize;
    let mut last_draw = Instant::now();
    if let Some(stdout) = stdout {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            if let Some(value) = line.strip_prefix("out_time_us=") {
                if let Ok(micros) = value.parse::<f64>() {
                    current = (micros / 1_000_000.0).max(0.0);
                }
            } else if let Some(value) = line.strip_prefix("out_time_ms=")
                && let Ok(micros) = value.parse::<f64>()
            {
                current = (micros / 1_000_000.0).max(0.0);
            }
            if last_draw.elapsed() >= Duration::from_millis(80) {
                if let Some(duration) = duration.filter(|duration| *duration > 0.0) {
                    let ratio = (current / duration).clamp(0.0, 1.0);
                    let detail = format!(
                        "{} / {}",
                        util::format_duration(current),
                        util::format_duration(duration)
                    );
                    ui::live_line(&ui::progress_line("转码喵", ratio, &detail, frame));
                } else {
                    let detail = format!("已处理 {}", util::format_duration(current));
                    ui::live_line(&ui::indeterminate_line("转码喵", &detail, frame));
                }
                frame = frame.wrapping_add(1);
                last_draw = Instant::now();
            }
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("等待 ffmpeg 失败: {error}"))?;
    let stderr = stderr_reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    ui::end_live_line();

    if !status.success() {
        if !output_existed {
            let _ = fs::remove_file(&output);
        }
        let detail = util::tail_lines(stderr.trim(), 8);
        if detail.is_empty() {
            return Err(format!("转换失败（退出码 {:?}）", status.code()));
        }
        return Err(format!("转换失败\n{detail}"));
    }

    if !output.is_file() {
        return Err("ffmpeg 已完成，但输出文件不存在".to_owned());
    }

    let size = fs::metadata(&output)
        .map(|meta| util::human_size(meta.len()))
        .unwrap_or_else(|_| "未知大小".to_owned());
    println!();
    ui::success("转换完成喵。");
    println!(
        "  {}  {}  {}",
        ui::paint("位置喵", ui::OVERLAY1),
        ui::paint(util::pretty_path(&output), ui::MAUVE),
        ui::paint(size, ui::OVERLAY2)
    );
    println!();
    Ok(())
}

fn print_help() {
    println!();
    println!(
        "  {}  {}",
        ui::paint(ui::ICON_MUSIC, ui::MAUVE),
        ui::paint_bold("音频转换喵", ui::TEXT)
    );
    println!();
    println!(
        "  {}",
        ui::paint(
            "用法: nya audio <音频> [输出] [-f 格式] [-b 码率] [--force]",
            ui::TEXT
        )
    );
    println!();
    println!("  {}", ui::paint_bold("格式", ui::LAVENDER));
    println!(
        "    {}",
        ui::paint("mp3  flac  wav  ogg  opus  m4a  aac  aiff", ui::TEXT)
    );
    println!();
    println!("  {}", ui::paint_bold("例子", ui::LAVENDER));
    println!("    {}", ui::paint("nya audio song.flac", ui::TEXT));
    println!(
        "    {}",
        ui::paint("nya audio song.flac out.ogg -b 128k", ui::TEXT)
    );
    println!();
    println!(
        "  {}",
        ui::paint("不写输出名时，默认转换为同目录的 mp3。", ui::OVERLAY1)
    );
    println!();
}

fn normalize_format(value: &str) -> Result<String, String> {
    let value = value.trim().trim_start_matches('.').to_ascii_lowercase();
    let value = if value == "aif" { "aiff" } else { &value };
    if FORMATS.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err(format!("未知格式: {value}\n可用: {}", FORMATS.join(" ")))
    }
}

fn normalize_bitrate(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    let number = value.strip_suffix('k').unwrap_or(&value);
    let number: u32 = number
        .parse()
        .map_err(|_| format!("码率格式应为 128k / 192k，当前为「{value}」"))?;
    if !(8..=512).contains(&number) {
        return Err("码率范围为 8k 到 512k".to_owned());
    }
    Ok(format!("{number}k"))
}

fn codec_label(format: &str, bitrate: &str) -> String {
    if is_lossless(format) {
        format.to_owned()
    } else {
        format!("{format} · {bitrate}")
    }
}

fn is_lossless(format: &str) -> bool {
    matches!(format, "flac" | "wav" | "aiff")
}

fn codec_args(format: &str, bitrate: &str) -> Vec<OsString> {
    let args: Vec<&str> = match format {
        "mp3" => vec!["-c:a", "libmp3lame", "-b:a", bitrate],
        "m4a" => vec!["-c:a", "aac", "-b:a", bitrate, "-movflags", "+faststart"],
        "aac" => vec!["-c:a", "aac", "-b:a", bitrate, "-f", "adts"],
        "opus" => vec!["-c:a", "libopus", "-b:a", bitrate],
        "ogg" => vec!["-c:a", "libvorbis", "-b:a", bitrate],
        "flac" => vec!["-c:a", "flac"],
        "wav" => vec!["-c:a", "pcm_s16le"],
        "aiff" => vec!["-c:a", "pcm_s16be"],
        _ => vec!["-c:a", "libmp3lame", "-b:a", bitrate],
    };
    args.into_iter().map(OsString::from).collect()
}

fn probe_duration(input: &Path) -> Option<f64> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=nw=1:nk=1")
        .arg(input)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|duration| *duration > 0.0)
}

fn same_file(input: &Path, output: &Path) -> bool {
    if input == output {
        return true;
    }
    match (fs::canonicalize(input), fs::canonicalize(output)) {
        (Ok(input), Ok(output)) => input == output,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_bitrates_are_normalized() {
        assert_eq!(normalize_format(".AIF").unwrap(), "aiff");
        assert!(normalize_format("midi").is_err());
        assert_eq!(normalize_bitrate("192K").unwrap(), "192k");
        assert!(normalize_bitrate("999").is_err());
    }

    #[test]
    fn lossless_formats_skip_bitrate() {
        assert!(is_lossless("flac"));
        assert!(!is_lossless("mp3"));
    }
}
