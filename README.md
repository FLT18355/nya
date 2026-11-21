# nya

一只喵里喵气的终端工具箱，Catppuccin Mocha 紫系配色。

```text
nya v0.1.1 喵
喵呜，怎么会有这么可爱的终端！
```

## 功能

- `cache`：清理 Bun / npm / pnpm / yarn / Go / uv / pip / Poetry / Cargo / Rustup / Deno / Composer / Gradle / Homebrew 缓存。默认全不选，终端里自己勾。
- `bili`：下载 Bilibili 视频或音频。支持 BV / av / 链接，拉取可用清晰度后手动选。
- `x`：解压 tar 系列、zip、7z、rar。
- `audio`：用 ffmpeg 转换音频格式。

## 安装

```bash
cargo install --path .
```

或直接运行：

```bash
cargo run --release -- --help
```

## 用法

```bash
nya
nya --version
nya -v

nya cache
nya cache bun
nya cache npm uv pip

nya bili
nya bili BV1xx411c7mD
nya bili BV1xx411c7mD -a
nya bili BV1xx411c7mD -x
nya bili BV1xx411c7mD -f 80+bestaudio/best
nya bili BV1xx411c7mD -c ~/cookies.txt
nya bili BV1xx411c7mD -n
nya bili BV1xx411c7mD -o ~/Videos

nya x archive.tar.gz
nya x archive.7z -o ~/Downloads/unpack
nya audio input.wav output.mp3
```

### bili 短选项

| 短选项 | 长选项 | 作用 |
|---|---|---|
| `-a` / `-x` | `--audio` | 仅音频并转码 |
| `-c` | `--cookies` | 指定 Cookies 文件 |
| `-n` | `--no-cookies` | 本次不使用 Cookies |
| `-o` | `--output` | 覆盖下载目录 |
| `-f` | `--format` | 跳过清晰度选择 |
| `-p` | `--playlist` | 下载播放列表 |

`--no-playlist` 只下载单个视频。

## 配置

配置文件默认在：

```text
~/.config/nya/config.toml
```

首次创建：

```bash
nya config init
```

配置按区块组织：

```toml
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
```

`cookies` 留空表示不使用。默认读取 `~/cookies.txt`。

## 构建

```bash
cargo build --release
```

二进制：

```text
target/release/nya
```

## GitHub Actions

`.github/workflows/linux.yml` 构建两个 Linux 目标：

- `x86_64-unknown-linux-gnu` → `nya-linux-x86_64`
- `aarch64-unknown-linux-gnu` → `nya-linux-arm64`

x86_64 原生构建，arm64 通过 `cross` 构建。推送 `v*` 标签时自动上传 release 资产。

## 依赖

- Rust 1.85+（edition 2024）
- Bilibili 下载：`yt-dlp`，视频合并/音频转换需要 `ffmpeg`
- 解压：`tar`、`7z`，rar 需要 7-Zip 支持
- 音频：`ffmpeg`、`ffprobe`

## 注意

- `cache` 只删已知缓存目录；Poetry 保留 `virtualenvs/`，Cargo 只清 `registry/cache` 和 `git/db`，Gradle 只清 `caches/modules-2`。
- 项目内的 `.git` / `.yarn/cache` 等缓存会被拒绝清理。
- Bun 缓存包含 `~/.cache/bun` 和 `~/.bun/install/cache`。