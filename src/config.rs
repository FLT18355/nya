use std::fs;
use std::path::PathBuf;

use crate::ui;
use crate::util;

const DEFAULT_CONFIG: &str = r#"# nya config
# 路径支持 ~ 开头。cookies 留空表示不使用。

[download]
dir = "~/Downloads"
cookies = "~/cookies.txt"
playlist = false

[video]
format = "bestvideo*+bestaudio/best"
merge = "mp4"

[audio]
format = "mp3"
quality = "0"

[output]
template = "%(title)s [%(id)s].%(ext)s"

[tool]
yt_dlp = "yt-dlp"
"#;

#[derive(Clone)]
pub struct Config {
    pub download_dir: PathBuf,
    pub cookies: Option<PathBuf>,
    pub video_format: String,
    pub audio_format: String,
    pub audio_quality: String,
    pub merge_format: Option<String>,
    pub output_template: String,
    pub yt_dlp: String,
    pub playlist: bool,
    pub path: PathBuf,
    pub exists: bool,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let path = config_path();
        let mut config = Self::defaults(path.clone());
        if path.is_file() {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("读取配置失败: {}: {error}", path.display()))?;
            config.apply(&text)?;
            config.exists = true;
        } else if path.exists() {
            return Err(format!("配置路径不是文件: {}", path.display()));
        }
        Ok(config)
    }

    fn defaults(path: PathBuf) -> Self {
        let home = util::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            download_dir: home.join("Downloads"),
            cookies: Some(home.join("cookies.txt")),
            video_format: "bestvideo*+bestaudio/best".to_owned(),
            audio_format: "mp3".to_owned(),
            audio_quality: "0".to_owned(),
            merge_format: Some("mp4".to_owned()),
            output_template: "%(title)s [%(id)s].%(ext)s".to_owned(),
            yt_dlp: "yt-dlp".to_owned(),
            playlist: false,
            path,
            exists: false,
        }
    }

    fn apply(&mut self, text: &str) -> Result<(), String> {
        let mut section: Option<String> = None;
        for (index, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            let line_number = index + 1;
            if let Some(rest) = line.strip_prefix('[') {
                let Some(name) = rest.strip_suffix(']') else {
                    return Err(format!("配置区块格式错误（第 {line_number} 行）: {raw}"));
                };
                let name = name.trim();
                if !matches!(name, "download" | "video" | "audio" | "output" | "tool") {
                    return Err(format!("未知配置区块（第 {line_number} 行）: {name}"));
                }
                section = Some(name.to_owned());
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("配置格式错误（第 {line_number} 行）: {raw}"));
            };
            let key = key.trim();
            let value = parse_value(value);
            let (section_name, key) = match section.as_deref() {
                Some(section) => (section, key),
                None => match key {
                    "download_dir" => ("download", "dir"),
                    "cookies" => ("download", "cookies"),
                    "playlist" => ("download", "playlist"),
                    "video_format" => ("video", "format"),
                    "merge_format" => ("video", "merge"),
                    "audio_format" => ("audio", "format"),
                    "audio_quality" => ("audio", "quality"),
                    "output_template" => ("output", "template"),
                    "yt_dlp" => ("tool", "yt_dlp"),
                    other => {
                        return Err(format!(
                            "未知配置项（第 {line_number} 行）: {other}\n配置项需要放在 [download] / [video] / [audio] / [output] / [tool] 区块内"
                        ));
                    }
                },
            };
            match (section_name, key) {
                ("download", "dir") => {
                    require_value(&value, "[download].dir", line_number)?;
                    self.download_dir = util::expand_tilde(&value);
                }
                ("download", "cookies") => {
                    self.cookies = if value.is_empty() || value.eq_ignore_ascii_case("none") {
                        None
                    } else {
                        Some(util::expand_tilde(&value))
                    };
                }
                ("download", "playlist") => {
                    self.playlist = parse_bool(&value).ok_or_else(|| {
                        format!("[download].playlist 必须是 true/false（第 {line_number} 行）")
                    })?;
                }
                ("video", "format") => {
                    require_value(&value, "[video].format", line_number)?;
                    self.video_format = value;
                }
                ("video", "merge") => {
                    self.merge_format = if value.is_empty() { None } else { Some(value) };
                }
                ("audio", "format") => {
                    require_value(&value, "[audio].format", line_number)?;
                    self.audio_format = value;
                }
                ("audio", "quality") => {
                    require_value(&value, "[audio].quality", line_number)?;
                    self.audio_quality = value;
                }
                ("output", "template") => {
                    require_value(&value, "[output].template", line_number)?;
                    self.output_template = value;
                }
                ("tool", "yt_dlp") => {
                    require_value(&value, "[tool].yt_dlp", line_number)?;
                    self.yt_dlp = util::expand_tilde(&value).to_string_lossy().into_owned();
                }
                _ => {
                    return Err(format!(
                        "未知配置项（第 {line_number} 行，[{section_name}]）: {key}"
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn init() -> Result<PathBuf, String> {
        let path = config_path();
        if path.exists() {
            return Err(format!("配置文件已存在: {}", path.display()));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败: {error}"))?;
        }
        fs::write(&path, DEFAULT_CONFIG)
            .map_err(|error| format!("写入配置失败: {}: {error}", path.display()))?;
        Ok(path)
    }
}

pub fn config_path() -> PathBuf {
    if let Some(path) = util::env_path("NYA_CONFIG") {
        return path;
    }
    util::config_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nya/config.toml")
}

pub fn run(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }
    if let Some(extra) = args.first() {
        return match extra.as_str() {
            "init" => {
                let path = Config::init()?;
                ui::success("配置已创建喵。");
                field("路径", util::pretty_path(&path));
                println!();
                Ok(())
            }
            other => Err(format!("未知参数喵: {other}\n用法: nya config [init]")),
        };
    }

    let config = Config::load()?;
    show(&config);
    Ok(())
}

fn show(config: &Config) {
    ui::header(ui::ICON_SETTINGS, "配置", &util::pretty_path(&config.path));
    let cookies = config
        .cookies
        .as_ref()
        .map(|path| {
            let state = if path.is_file() { "存在" } else { "缺失" };
            format!("{} ({state})", util::pretty_path(path))
        })
        .unwrap_or_else(|| "不使用".to_owned());
    field("下载目录", util::pretty_path(&config.download_dir));
    field("Cookies 文件", cookies);
    field(
        "播放列表",
        if config.playlist {
            "开启".to_owned()
        } else {
            "关闭".to_owned()
        },
    );
    field("视频格式", config.video_format.clone());
    field(
        "合并容器",
        config
            .merge_format
            .clone()
            .unwrap_or_else(|| "不指定".to_owned()),
    );
    field("音频格式", config.audio_format.clone());
    field("音频质量", config.audio_quality.clone());
    field("输出模板", config.output_template.clone());
    field("yt-dlp 路径", config.yt_dlp.clone());
    println!();
    if !config.exists {
        ui::hint("未找到配置文件，当前使用默认值喵。运行 nya config init 创建。");
    }
    println!();
}

fn field(label: &str, value: String) {
    let padding = 12usize.saturating_sub(util::display_width(label));
    println!(
        "  {}{}  {}",
        ui::paint(label, ui::OVERLAY1),
        " ".repeat(padding),
        ui::paint(value, ui::TEXT)
    );
}

fn print_help() {
    println!();
    println!(
        "  {}  {}",
        ui::paint(ui::ICON_SETTINGS, ui::MAUVE),
        ui::paint_bold("配置", ui::TEXT)
    );
    println!();
    println!("  {}", ui::paint("用法: nya config [init]", ui::TEXT));
    println!();
    println!(
        "  {}",
        ui::paint("config      显示当前配置与配置文件路径", ui::TEXT)
    );
    println!("  {}", ui::paint("config init 写入默认配置文件", ui::TEXT));
    println!();
    println!("  {}", ui::paint_bold("配置区块", ui::LAVENDER));
    println!(
        "    {}",
        ui::paint("[download]  下载目录 / Cookies / 播放列表", ui::TEXT)
    );
    println!(
        "    {}",
        ui::paint("[video]     视频格式 / 合并容器", ui::TEXT)
    );
    println!(
        "    {}",
        ui::paint("[audio]     音频格式 / 音频质量", ui::TEXT)
    );
    println!("    {}", ui::paint("[output]    输出模板", ui::TEXT));
    println!("    {}", ui::paint("[tool]      yt-dlp 路径", ui::TEXT));
    println!();
}

fn require_value(value: &str, key: &str, line: usize) -> Result<(), String> {
    if value.is_empty() {
        Err(format!("{key} 不能为空（第 {line} 行）"))
    } else {
        Ok(())
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => Some(true),
        "false" | "no" | "0" | "off" => Some(false),
        _ => None,
    }
}

fn parse_value(raw: &str) -> String {
    let raw = strip_comment(raw).trim();
    if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        raw[1..raw.len() - 1].to_owned()
    } else {
        raw.to_owned()
    }
}

fn strip_comment(raw: &str) -> &str {
    let mut single = false;
    let mut double = false;
    for (index, c) in raw.char_indices() {
        match c {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '#' if !single && !double => return &raw[..index],
            _ => {}
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config::defaults(PathBuf::from("/tmp/nya-test-config.toml"))
    }

    #[test]
    fn parses_sections_and_comments() {
        let mut config = test_config();
        config
            .apply(
                r#"
                [download]
                dir = "~/Videos" # comment
                cookies = ""
                playlist = true

                [video]
                format = "best"
                merge = ""

                [audio]
                format = 'flac'
                quality = "5"

                [output]
                template = "%(title)s.%(ext)s"

                [tool]
                yt_dlp = "/tmp/yt-dlp"
                "#,
            )
            .unwrap();
        assert!(config.download_dir.ends_with("Videos"));
        assert!(config.cookies.is_none());
        assert!(config.playlist);
        assert_eq!(config.video_format, "best");
        assert!(config.merge_format.is_none());
        assert_eq!(config.audio_format, "flac");
        assert_eq!(config.audio_quality, "5");
        assert_eq!(config.output_template, "%(title)s.%(ext)s");
        assert_eq!(config.yt_dlp, "/tmp/yt-dlp");
    }

    #[test]
    fn parses_legacy_flat_keys() {
        let mut config = test_config();
        config
            .apply(
                r#"
                download_dir = "~/Videos"
                cookies = ""
                audio_format = 'flac'
                playlist = true
                "#,
            )
            .unwrap();
        assert!(config.download_dir.ends_with("Videos"));
        assert!(config.cookies.is_none());
        assert_eq!(config.audio_format, "flac");
        assert!(config.playlist);
    }

    #[test]
    fn rejects_unknown_keys_and_sections() {
        let mut config = test_config();
        assert!(config.apply("typo_key = \"x\"").is_err());
        assert!(config.apply("[download]\ntypo_key = \"x\"").is_err());
        assert!(config.apply("[nope]\nkey = \"x\"").is_err());
    }
}
